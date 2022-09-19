mod canvas;
mod layout;

use self::layout::{CompositorHandle, Instance, InstanceKey, ViewOptions, ViewerGui};
use crate::{
    compositor::{dev::LogicalDevice, CompositeLayer, CompositorPipeline, CompositorTarget},
    gui::layout::ViewerTab,
    silica::{ProcreateFile, SilicaHierarchy},
};
use egui_wgpu::renderer::{RenderPass, ScreenDescriptor};
use parking_lot::{Mutex, RwLock};
use std::path::PathBuf;
use std::{
    collections::HashMap,
    sync::atomic::{
        AtomicBool, AtomicUsize,
        Ordering::{Acquire, Release},
    },
};
use winit::event::{Event, WindowEvent};
use winit::event_loop::ControlFlow;

fn linearize<'a>(
    layers: &'a crate::silica::SilicaGroup,
    composite_layers: &mut Vec<CompositeLayer>,
    mask_layer: &mut Option<(usize, &'a crate::silica::SilicaLayer)>,
) {
    for layer in layers.children.iter().rev() {
        match layer {
            SilicaHierarchy::Group(group) if !group.hidden => {
                linearize(group, composite_layers, mask_layer);
            }
            SilicaHierarchy::Layer(layer) if !layer.hidden => {
                if let Some((_, mask_layer)) = mask_layer {
                    if layer.clipped && mask_layer.hidden {
                        continue;
                    }
                }

                if !layer.clipped {
                    *mask_layer = Some((layer.image, layer));
                }

                composite_layers.push(CompositeLayer {
                    texture: layer.image,
                    clipped: layer.clipped.then(|| mask_layer.unwrap().0),
                    opacity: layer.opacity,
                    blend: layer.blend,
                });
            }
            _ => continue,
        }
    }
}

struct FrameLimiter {
    delta: std::time::Duration,
    next_time: std::time::Instant,
}

impl FrameLimiter {
    pub fn new(target_fps: u32) -> Self {
        Self {
            delta: std::time::Duration::from_secs(1).div_f64(f64::from(target_fps)),
            next_time: std::time::Instant::now(),
        }
    }

    pub async fn wait(&mut self) {
        let now = std::time::Instant::now();
        if let Some(diff) = self.next_time.checked_duration_since(now) {
            // We have woken up before the minimum time that we needed to wait
            // before drawing another frame.
            // now ------------- next_frame
            //        diff
            tokio::time::sleep(diff).await
        } else {
            // We have waken up after the minimum time that we needed to wait to
            // begin drawing another frame.
            // Case 1 //////////////////////////////////////////////////
            //                   delta
            // next_frame ------------------ next_frame + delta
            // next_frame --------- now
            //               diff
            //                      now ---- next_frame + delta
            //                       delta - diff
            // delta - diff > 0
            // Case 2 //////////////////////////////////////////////////
            //              delta
            // next_frame -------- next_frame + delta
            //                     next_frame + delta ------- now
            // next_frame ----------------------------------- now
            //                          diff
            // delta - diff == 0
            self.next_time = now
                + self.delta.saturating_sub(
                    now.checked_duration_since(self.next_time)
                        .unwrap_or_default(),
                );
        }
    }
}

async fn rendering_thread(cs: &CompositorHandle) {
    let mut limiter = FrameLimiter::new(60);
    loop {
        // Ensures that we are not generating frames faster than 60FPS
        // to avoid putting unnecessary computational pressure on the GPU.
        limiter.wait().await;

        for (_, instance) in cs.instances.read().iter() {
            let file = instance.file.read();

            let new_layer_config = instance.file.read().layers.clone();
            // Only force a recompute if we need to.
            let background = (!file.background_hidden).then_some(file.background_color);

            drop(file);

            if instance.change_untick() {
                let mut resolved_layers = Vec::new();
                let mut mask_layer = None;
                linearize(&new_layer_config, &mut resolved_layers, &mut mask_layer);

                let mut lock = instance.target.lock();
                lock.render(
                    &cs.pipeline,
                    background,
                    &resolved_layers,
                    &instance.textures,
                );
                // ENABLE TO DEBUG: hold the lock to make sure the GUI is responsive
                // std::thread::sleep(std::time::Duration::from_secs(1));
                // Debugging notes: if the GPU is highly contended, the main
                // GUI rendering can still be somewhat sluggish.
                drop(lock);
            }
        }
    }
}

pub async fn load_file(
    path: PathBuf,
    dev: &'static LogicalDevice,
    compositor: &CompositorHandle,
) -> bool {
    if let Ok(Ok((file, textures))) =
        tokio::task::spawn_blocking(|| ProcreateFile::open(path, dev)).await
    {
        let mut target = CompositorTarget::new(dev);
        target.flip_vertices((file.flipped.horizontally, file.flipped.vertically));
        target.set_dimensions(file.size.width, file.size.height);

        for _ in 0..file.orientation {
            target.rotate_vertices(true);
            target.set_dimensions(target.dim.height, target.dim.width);
        }

        let id = compositor.curr_id.load(Acquire);
        compositor.curr_id.store(id + 1, Release);
        compositor.instances.write().insert(
            InstanceKey(id),
            Instance {
                file: RwLock::new(file),
                target: Mutex::new(target),
                textures,
                new_texture: AtomicBool::new(true),
                changed: AtomicBool::new(true),
            },
        );

        true
    } else {
        false
    }
}

fn leak<T>(value: T) -> &'static T {
    &*Box::leak(Box::new(value))
}

pub fn start_gui(window: winit::window::Window, event_loop: winit::event_loop::EventLoop<()>) -> ! {
    // LEAK: obtain static reference because this will live for the rest of
    // the lifetime of the program.
    let rt = leak(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap(),
    );
    let (dev, surface) = rt.block_on(LogicalDevice::with_window(&window)).unwrap();
    let dev = leak(dev);
    let compositor = leak(CompositorHandle {
        instances: RwLock::new(HashMap::new()),
        pipeline: CompositorPipeline::new(dev),
        curr_id: AtomicUsize::new(0),
    });
    let toasts = leak(Mutex::new(egui_notify::Toasts::default()));

    let window_size = window.inner_size();
    let surface_format = surface.get_supported_formats(&dev.adapter)[0];
    let mut surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: window_size.width,
        height: window_size.height,
        present_mode: wgpu::PresentMode::Fifo,
    };
    surface.configure(&dev.device, &surface_config);

    let mut state = egui_winit::State::new(&event_loop);
    state.set_pixels_per_point(window.scale_factor() as f32);

    let context = egui::Context::default();
    context.set_pixels_per_point(window.scale_factor() as f32);

    let mut egui_rpass = RenderPass::new(&dev.device, surface_format, 1);

    let mut editor = ViewerGui {
        dev,
        rt,
        canvases: HashMap::new(),
        view_options: ViewOptions {
            smooth: false,
            grid: true,
            extended_crosshair: false,
            rotation: 0.0,
            bottom_bar: false,
        },
        selected_canvas: InstanceKey(0),
        compositor: &compositor,
        canvas_tree: egui_dock::Tree::default(),
        viewer_tree: {
            use egui_dock::{NodeIndex, Tree};
            let mut tree = Tree::new(vec![
                ViewerTab::Information,
                ViewerTab::ViewControls,
                ViewerTab::CanvasControls,
            ]);
            tree.split_below(
                NodeIndex::root(),
                0.4,
                vec![ViewerTab::Files, ViewerTab::Hierarchy],
            );
            tree
        },
        queued_remove: None,
        toasts,
    };

    rt.spawn(rendering_thread(compositor));

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::WindowEvent { event, .. } => {
                match event {
                    WindowEvent::CloseRequested => {
                        *control_flow = ControlFlow::Exit;
                    }
                    WindowEvent::Resized(size) => {
                        // Resize with 0 width and height is used by winit to signal a minimize event on Windows.
                        // See: https://github.com/rust-windowing/winit/issues/208
                        // This solves an issue where the app would panic when minimizing on Windows.
                        if size.width > 0 && size.height > 0 {
                            surface_config.width = size.width;
                            surface_config.height = size.height;
                            surface.configure(&dev.device, &surface_config);
                        }
                    }
                    WindowEvent::DroppedFile(file) => {
                        println!("File dropped: {:?}", file.as_path().display().to_string());
                        rt.spawn(async move {
                            if load_file(file, &dev, compositor).await {
                                toasts.lock().success("Loaded file from drag/drop.");
                            } else {
                                toasts.lock().error("File from drag/drop failed to load.");
                            }
                        });
                    }
                    _ => {
                        state.on_event(&context, &event);
                    }
                }
            }
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(..) => {
                let output_frame = match surface.get_current_texture() {
                    Ok(frame) => frame,
                    Err(wgpu::SurfaceError::Outdated) => {
                        // This error occurs when the app is minimized on Windows.
                        // Silently return here to prevent spamming the console with:
                        // "The underlying surface has changed, and therefore the swap chain must be updated"
                        return;
                    }
                    Err(e) => {
                        eprintln!("Dropped frame with error: {}", e);
                        return;
                    }
                };

                let output_view = output_frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                let input = state.take_egui_input(&window);

                context.begin_frame(input);
                editor.layout_gui(&context);
                editor.toasts.lock().show(&context);
                let output = context.end_frame();

                state.handle_platform_output(&window, &context, output.platform_output);

                let paint_jobs = context.tessellate(output.shapes);

                // Upload all resources for the GPU.
                let screen_descriptor = ScreenDescriptor {
                    size_in_pixels: [surface_config.width, surface_config.height],
                    pixels_per_point: window.scale_factor() as f32,
                };

                for (id, image_delta) in &output.textures_delta.set {
                    egui_rpass.update_texture(&dev.device, &dev.queue, *id, image_delta);
                }
                for id in &output.textures_delta.free {
                    egui_rpass.free_texture(id);
                }
                egui_rpass.update_buffers(&dev.device, &dev.queue, &paint_jobs, &screen_descriptor);

                dev.queue.submit(Some({
                    let mut encoder = dev
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                    egui_rpass.execute(
                        &mut encoder,
                        &output_view,
                        &paint_jobs,
                        &screen_descriptor,
                        Some(wgpu::Color::BLACK),
                    );

                    encoder.finish()
                }));
                output_frame.present();

                if let Some(index) = editor.queued_remove.take() {
                    editor.remove_index(index);
                }

                // Updates textures bound for egui rendering
                // Do not block on any locks/rwlocks since we do not want to block
                // the GUI thread when the renderer is potentially taking a long
                // time to render a frame
                if let Some(instances) = compositor.instances.try_read() {
                    for (idx, instance) in instances.iter() {
                        if instance.new_texture.load(Acquire) {
                            if let Some(target) = instance.target.try_lock() {
                                if let Some((tex, _)) = editor.canvases.insert(
                                    *idx,
                                    (
                                        egui_rpass.register_native_texture(
                                            &dev.device,
                                            &target.output_texture.as_ref().unwrap().make_view(),
                                            if editor.view_options.smooth {
                                                wgpu::FilterMode::Linear
                                            } else {
                                                wgpu::FilterMode::Nearest
                                            },
                                        ),
                                        target.dim,
                                    ),
                                ) {
                                    egui_rpass.free_texture(&tex);
                                }

                                instance.new_texture_untick();
                            }
                        }
                    }
                }
            }
            _ => (),
        }
    });
}
