mod app;
mod canvas;
mod layout;

use self::{
    app::{App, CompositorHandle, InstanceKey},
    layout::{ViewOptions, ViewerGui},
};
use crate::{
    compositor::{dev::GpuHandle, CompositorPipeline},
    gui::layout::ViewerTab,
};
use egui::{FullOutput, ViewportId};
use egui_wgpu::renderer::{Renderer, ScreenDescriptor};
use egui_winit::winit::event::{Event, WindowEvent};
use egui_winit::winit::event_loop::ControlFlow;
use parking_lot::{Mutex, RwLock};
use std::time::Duration;
use std::{
    collections::HashMap,
    sync::{atomic::AtomicUsize, Arc},
    time::Instant,
};

#[derive(Debug, Clone, Copy)]
pub enum UserEvent {
    RebindTexture(InstanceKey),
    RemoveInstance(InstanceKey),
}

pub fn start_gui(
    window: egui_winit::winit::window::Window,
    event_loop: egui_winit::winit::event_loop::EventLoop<UserEvent>,
) -> ! {
    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap(),
    );

    let (app, surface) = {
        let (dev, surface) = rt.block_on(GpuHandle::with_window(&window)).unwrap();
        (
            Arc::new(App {
                compositor: CompositorHandle {
                    instances: RwLock::new(HashMap::new()),
                    pipeline: CompositorPipeline::new(&dev),
                    curr_id: AtomicUsize::new(0),
                },
                dev: Arc::new(dev),
                toasts: Mutex::new(egui_notify::Toasts::default()),
                added_instances: Mutex::new(Vec::with_capacity(1)),
                eloop: event_loop.create_proxy(),
            }),
            surface,
        )
    };

    let window_size = window.inner_size();
    let surface_caps = surface.get_capabilities(&app.dev.adapter);
    let surface_format = surface_caps.formats[0];
    let surface_alpha = surface_caps.alpha_modes[0];
    let mut surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: window_size.width,
        height: window_size.height,
        present_mode: wgpu::PresentMode::Fifo,
        view_formats: Vec::new(),
        alpha_mode: surface_alpha,
    };
    let mut screen_descriptor = ScreenDescriptor {
        size_in_pixels: [surface_config.width, surface_config.height],
        pixels_per_point: window.scale_factor() as f32,
    };
    surface.configure(&app.dev.device, &surface_config);

    let mut integration = egui_winit::State::new(
        ViewportId::ROOT,
        &window,
        Some(window.scale_factor() as f32),
        None,
    );

    let context = egui::Context::default();

    let mut egui_rpass = Renderer::new(&app.dev.device, surface_format, None, 1);

    let mut editor = ViewerGui {
        app: app.clone(),
        rt: rt.clone(),
        canvases: HashMap::new(),
        view_options: ViewOptions {
            smooth: false,
            grid: true,
            extended_crosshair: false,
            rotation: 0.0,
            bottom_bar: false,
        },
        selected_canvas: InstanceKey(0),
        canvas_tree: egui_dock::DockState::new(Vec::new()),
        viewer_tree: {
            let tabs = vec![
                ViewerTab::Information,
                ViewerTab::ViewControls,
                ViewerTab::CanvasControls,
            ];
            let mut state = egui_dock::DockState::new(tabs);
            state.main_surface_mut().split_below(
                egui_dock::NodeIndex::root(),
                0.4,
                vec![ViewerTab::Hierarchy],
            );
            state
        },
    };

    rt.spawn(app.clone().rendering_thread());

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
                            screen_descriptor.size_in_pixels = [size.width, size.height];
                            surface.configure(&app.dev.device, &surface_config);
                        }
                    }
                    WindowEvent::ScaleFactorChanged {
                        scale_factor,
                        new_inner_size: &mut size,
                    } => {
                        if size.width > 0 && size.height > 0 {
                            surface_config.width = size.width;
                            surface_config.height = size.height;
                            screen_descriptor.pixels_per_point = scale_factor as f32;
                            surface.configure(&app.dev.device, &surface_config);
                        }
                    }
                    WindowEvent::DroppedFile(file) => {
                        println!("File dropped: {:?}", file.as_path().display().to_string());
                        rt.spawn({
                            let app = app.clone();
                            async move {
                                match app.clone().load_file(file).await {
                                    Err(err) => {
                                        app.toasts.lock().error(format!(
                                            "File from drag/drop failed to load. Reason: {err}"
                                        ));
                                    }
                                    Ok(key) => {
                                        app.toasts.lock().success("Loaded file from drag/drop.");
                                        app.added_instances.lock().push((
                                            egui_dock::SurfaceIndex::main(),
                                            egui_dock::NodeIndex::root(),
                                            key,
                                        ));
                                    }
                                }
                            }
                        });
                    }
                    _ => {
                        let response = integration.on_window_event(&context, &event);
                        *control_flow = if response.repaint {
                            window.request_redraw();
                            ControlFlow::Poll
                        } else {
                            ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(1))
                        }
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

                let input = integration.take_egui_input(&window);

                context.begin_frame(input);
                editor.layout_gui(&context);
                editor.app.toasts.lock().show(&context);
                let FullOutput {
                    platform_output,
                    textures_delta,
                    shapes,
                    pixels_per_point,
                    viewport_output,
                } = context.end_frame();

                let repaint_after = viewport_output[&ViewportId::ROOT].repaint_delay;

                *control_flow = if repaint_after.is_zero() {
                    window.request_redraw();
                    ControlFlow::Poll
                } else if let Some(repaint_after_instant) =
                    Instant::now().checked_add(repaint_after)
                {
                    ControlFlow::WaitUntil(repaint_after_instant)
                } else {
                    ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(1))
                };

                integration.handle_platform_output(&window, &context, platform_output);

                // Draw the GUI onto the output texture.
                let paint_jobs = context.tessellate(shapes, pixels_per_point);

                // Upload all resources for the GPU.
                for (id, image_delta) in textures_delta.set {
                    egui_rpass.update_texture(&app.dev.device, &app.dev.queue, id, &image_delta);
                }
                for id in textures_delta.free {
                    egui_rpass.free_texture(&id);
                }

                app.dev.queue.submit(Some({
                    let mut encoder = app
                        .dev
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

                    egui_rpass.update_buffers(
                        &app.dev.device,
                        &app.dev.queue,
                        &mut encoder,
                        &paint_jobs,
                        &screen_descriptor,
                    );

                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &output_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });

                    egui_rpass.render(&mut rpass, &paint_jobs, &screen_descriptor);

                    drop(rpass);

                    encoder.finish()
                }));
                output_frame.present();
            }
            Event::UserEvent(UserEvent::RemoveInstance(idx)) => {
                editor.remove_index(idx);
            }
            Event::UserEvent(e @ UserEvent::RebindTexture(idx)) => {
                // Updates textures bound for EGUI rendering
                // Do not block on any locks/rwlocks since we do not want to block
                // the GUI thread when the renderer is potentially taking a long
                // time to render a frame.
                let texture_filter = if editor.view_options.smooth {
                    wgpu::FilterMode::Linear
                } else {
                    wgpu::FilterMode::Nearest
                };

                let instances = app.compositor.instances.read();
                if let Some(instance) = instances.get(&idx) {
                    if let Some(target) = instance.target.try_lock() {
                        if let Some(output) = target.output.as_ref() {
                            let texture_view = output.texture.create_srgb_view();

                            if let Some((tex, dim)) = editor.canvases.get_mut(&idx) {
                                egui_rpass.update_egui_texture_from_wgpu_texture(
                                    &app.dev.device,
                                    &texture_view,
                                    texture_filter,
                                    *tex,
                                );
                                *dim = target.dim;
                            } else {
                                let tex = egui_rpass.register_native_texture(
                                    &app.dev.device,
                                    &texture_view,
                                    texture_filter,
                                );
                                editor.canvases.insert(idx, (tex, target.dim));
                            }
                            return;
                        }
                    }
                }
                // bounce the event
                app.eloop.send_event(e).unwrap();
            }
            _ => (),
        }
    });
}
