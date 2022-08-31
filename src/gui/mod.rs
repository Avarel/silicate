mod layout;

use self::layout::{CompositorState, EditorState};
use crate::compositor::{dev::LogicalDevice, tex::GpuTexture, CompositeLayer};
use crate::silica::{ProcreateFile, SilicaGroup};
use crate::{compositor::Compositor, silica::SilicaHierarchy};
use egui_wgpu::renderer::{RenderPass, ScreenDescriptor};
use parking_lot::RwLock;
use std::sync::{atomic::AtomicBool, Arc};
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

    pub fn wait(&mut self) {
        let now = std::time::Instant::now();
        if let Some(diff) = self.next_time.checked_duration_since(now) {
            // We have woken up before the minimum time that we needed to wait
            // before drawing another frame.
            // now ------------- next_frame
            //        diff
            std::thread::sleep(diff);
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

fn rendering_thread(cs: Arc<CompositorState>) {
    let mut limiter = FrameLimiter::new(60);
    let mut old_layer_config = SilicaGroup::empty();
    while cs.is_active() {
        // Ensures that we are not generating frames faster than 60FPS
        // to avoid putting unnecessary computational pressure on the GPU.
        limiter.wait();

        if cs.compositor.read().dim.is_empty() || cs.file.read().is_none() {
            continue;
        }

        // Only force a recompute if we need to.
        let new_layer_config = cs.file.read().as_ref().unwrap().layers.clone();
        if cs.get_recomposit() || old_layer_config != new_layer_config {
            let mut resolved_layers = Vec::new();
            let gpu_textures = cs.gpu_textures.read();
            let mut mask_layer = None;
            linearize(
                &new_layer_config,
                &mut resolved_layers,
                &mut mask_layer,
            );

            let background = {
                let file = cs.file.read();
                let file = file.as_ref().unwrap();
                (!file.background_hidden).then_some(file.background_color)
            };

            *cs.tex.write() =
                cs.compositor
                    .write()
                    .render(background, &resolved_layers, &gpu_textures.as_ref().unwrap());
            old_layer_config = new_layer_config;
            cs.set_recomposit(false);
        }
    }
}

pub fn start_gui(
    dev: LogicalDevice,
    surface: wgpu::Surface,
    window: winit::window::Window,
    event_loop: winit::event_loop::EventLoop<()>,
) {
    let dev = &*Box::leak(Box::new(dev));

    let window_size = window.inner_size();

    let surface_format = surface.get_supported_formats(&dev.adapter)[0];

    let swap_chain_format = wgpu::TextureFormat::Bgra8UnormSrgb;

    let mut surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: swap_chain_format,
        width: window_size.width,
        height: window_size.height,
        present_mode: wgpu::PresentMode::Fifo,
    };

    surface.configure(&dev.device, &surface_config);

    let mut state = egui_winit::State::new(&event_loop);
    state.set_pixels_per_point(window.scale_factor() as f32);
    let context = egui::Context::default();
    context.set_pixels_per_point(window.scale_factor() as f32);

    let cs = Arc::new(CompositorState {
        file: RwLock::new(None),
        gpu_textures: RwLock::new(None),
        compositor: RwLock::new(Compositor::new(dev)),
        tex: RwLock::new(GpuTexture::empty_with_extent(
            &dev,
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            None,
            GpuTexture::OUTPUT_USAGE,
        )),
        active: AtomicBool::new(true),
        force_recomposit: AtomicBool::new(false),
    });

    let mut egui_rpass = RenderPass::new(&dev.device, surface_format, 1);

    let egui_tex = egui_rpass.register_native_texture(
        &dev.device,
        &cs.tex.read().make_view(),
        wgpu::FilterMode::Linear,
    );

    let mut es = EditorState {
        dev,
        egui_tex,
        smooth: false,
        show_grid: true,
        cs: Arc::clone(&cs),
    };

    std::thread::spawn({
        let cs = Arc::clone(&cs);
        move || rendering_thread(cs)
    });

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::WindowEvent { event, .. } => {
                match event {
                    WindowEvent::CloseRequested => {
                        es.cs.deactivate();
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

                        let cs = Arc::clone(&cs);
                        std::thread::spawn(move || {
                            let (pf, pt) = ProcreateFile::open(file, &dev).unwrap();
                            let mut file = cs.file.write();
                            let mut textures = cs.gpu_textures.write();
                            let mut compositor = cs.compositor.write();
                            compositor
                                .flip_vertices((pf.flipped.horizontally, pf.flipped.vertically));
                            compositor.set_dimensions(pf.size.width, pf.size.height);
                            *file = Some(pf);
                            *textures = Some(pt);
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
                es.layout_gui(&context);
                let output = context.end_frame();

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

                {
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
                    dev.queue.submit(Some(encoder.finish()));
                }
                output_frame.present();

                if let Some(z) = es.cs.tex.try_read() {
                    egui_rpass.free_texture(&es.egui_tex);
                    es.egui_tex = egui_rpass.register_native_texture(
                        &dev.device,
                        &z.make_view(),
                        if es.smooth {
                            wgpu::FilterMode::Linear
                        } else {
                            wgpu::FilterMode::Nearest
                        },
                    );
                }
            }
            _ => (),
        }
    });
}
