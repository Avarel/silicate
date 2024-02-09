pub mod app;
mod canvas;
mod layout;

use self::{
    app::{App, InstanceKey},
    layout::{ViewOptions, ViewerGui},
};
use crate::gui::layout::ViewerTab;
use egui::{load::SizedTexture, FullOutput, ViewportId};
use egui_wgpu::{Renderer, ScreenDescriptor};

use crate::winit;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc, time::Instant};
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

impl App {
    pub fn run(
        self: Arc<Self>,
        window: &winit::window::Window,
        surface: wgpu::Surface,
        event_loop: egui_winit::winit::event_loop::EventLoop<app::UserEvent>,
    ) -> Result<(), winit::error::EventLoopError> {
        let surface_caps = surface.get_capabilities(&self.dev.adapter);
        let surface_format = surface_caps.formats[0];
        let surface_alpha = surface_caps.alpha_modes[0];
        let mut surface_config = {
            let window_size = window.inner_size();
            wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width: window_size.width,
                height: window_size.height,
                present_mode: wgpu::PresentMode::Fifo,
                view_formats: Vec::new(),
                alpha_mode: surface_alpha,
                desired_maximum_frame_latency: 0,
            }
        };
        let mut screen_descriptor = ScreenDescriptor {
            size_in_pixels: [surface_config.width, surface_config.height],
            pixels_per_point: window.scale_factor() as f32,
        };
        surface.configure(&self.dev.device, &surface_config);

        let mut integration = egui_winit::State::new(
            egui::Context::default(),
            ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
        );

        let mut renderer = Renderer::new(&self.dev.device, surface_format, None, 1);

        let mut editor = ViewerGui {
            app: self.clone(),
            canvases: HashMap::new(),
            view_options: ViewOptions {
                smooth: false,
                grid: true,
                extended_crosshair: false,
                rotation: 0.0,
                bottom_bar: false,
            },
            active_canvas: InstanceKey(0),
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

        self.rt.spawn(self.clone().rendering_thread());

        event_loop.run(move |event, eltarget| {
            match event {
                // Event::MainEventsCleared => window.request_redraw(),
                Event::WindowEvent { event, .. } => {
                    match event {
                        WindowEvent::RedrawRequested => {
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

                            integration.egui_ctx().begin_frame(input);
                            editor.layout_gui(&integration.egui_ctx());
                            editor.app.toasts.lock().show(&integration.egui_ctx());
                            let FullOutput {
                                platform_output,
                                textures_delta,
                                shapes,
                                pixels_per_point,
                                viewport_output,
                            } = integration.egui_ctx().end_frame();

                            let repaint_after = viewport_output[&ViewportId::ROOT].repaint_delay;

                            if repaint_after.is_zero() {
                                window.request_redraw();
                                eltarget.set_control_flow(ControlFlow::Poll);
                            } else if let Some(repaint_after_instant) =
                                Instant::now().checked_add(repaint_after)
                            {
                                eltarget.set_control_flow(ControlFlow::WaitUntil(
                                    repaint_after_instant,
                                ));
                            } else {
                                eltarget.set_control_flow(ControlFlow::WaitUntil(
                                    Instant::now() + Duration::from_secs(1),
                                ));
                            }

                            integration.handle_platform_output(&window, platform_output);

                            // Draw the GUI onto the output texture.
                            let paint_jobs =
                                integration.egui_ctx().tessellate(shapes, pixels_per_point);

                            // Upload all resources for the GPU.
                            for (id, image_delta) in textures_delta.set {
                                renderer.update_texture(
                                    &self.dev.device,
                                    &self.dev.queue,
                                    id,
                                    &image_delta,
                                );
                            }
                            for id in textures_delta.free {
                                renderer.free_texture(&id);
                            }

                            self.dev.queue.submit(Some({
                                let mut encoder = self.dev.device.create_command_encoder(
                                    &wgpu::CommandEncoderDescriptor::default(),
                                );

                                renderer.update_buffers(
                                    &self.dev.device,
                                    &self.dev.queue,
                                    &mut encoder,
                                    &paint_jobs,
                                    &screen_descriptor,
                                );

                                renderer.render(
                                    &mut encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                        label: None,
                                        color_attachments: &[Some(
                                            wgpu::RenderPassColorAttachment {
                                                view: &output_view,
                                                resolve_target: None,
                                                ops: wgpu::Operations {
                                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                                    store: wgpu::StoreOp::Store,
                                                },
                                            },
                                        )],
                                        depth_stencil_attachment: None,
                                        timestamp_writes: None,
                                        occlusion_query_set: None,
                                    }),
                                    &paint_jobs,
                                    &screen_descriptor,
                                );

                                encoder.finish()
                            }));
                            output_frame.present();
                        }
                        WindowEvent::CloseRequested => {
                            eltarget.exit();
                            return;
                        }
                        WindowEvent::Resized(size) => {
                            // Resize with 0 width and height is used by winit to signal a minimize event on Windows.
                            // See: https://github.com/rust-windowing/winit/issues/208
                            // This solves an issue where the app would panic when minimizing on Windows.
                            if size.width > 0 && size.height > 0 {
                                surface_config.width = size.width;
                                surface_config.height = size.height;
                                screen_descriptor.size_in_pixels = [size.width, size.height];
                                surface.configure(&self.dev.device, &surface_config);
                            }
                        }
                        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                            screen_descriptor.pixels_per_point = scale_factor as f32;
                            surface.configure(&self.dev.device, &surface_config);
                        }
                        WindowEvent::DroppedFile(file) => {
                            println!("File dropped: {:?}", file.as_path().display().to_string());
                            self.rt.spawn({
                                let app = self.clone();
                                async move {
                                    match app.clone().load_file(file).await {
                                        Err(err) => {
                                            app.toasts.lock().error(format!(
                                                "File from drag/drop failed to load. Reason: {err}"
                                            ));
                                        }
                                        Ok(key) => {
                                            app.toasts
                                                .lock()
                                                .success("Loaded file from drag/drop.");
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
                            let response = integration.on_window_event(&window, &event);
                            if response.repaint {
                                window.request_redraw();
                                eltarget.set_control_flow(ControlFlow::Poll);
                            } else {
                                eltarget.set_control_flow(ControlFlow::WaitUntil(
                                    Instant::now() + Duration::from_secs(1),
                                ))
                            }
                        }
                    }
                }
                Event::UserEvent(app::UserEvent::RemoveInstance(idx)) => {
                    editor.remove_index(idx);
                }
                Event::UserEvent(e @ app::UserEvent::RebindTexture(idx)) => {
                    // Updates textures bound for EGUI rendering
                    // Do not block on any locks/rwlocks since we do not want to block
                    // the GUI thread when the renderer is potentially taking a long
                    // time to render a frame.
                    let texture_filter = if editor.view_options.smooth {
                        wgpu::FilterMode::Linear
                    } else {
                        wgpu::FilterMode::Nearest
                    };

                    let instances = self.compositor.instances.read();
                    if let Some(instance) = instances.get(&idx) {
                        if let Some(target) = instance.target.try_lock() {
                            if let Some(output) = target.output.as_ref() {
                                let texture_view = output.texture.create_srgb_view();

                                if let Some(tex) = editor.canvases.get_mut(&idx) {
                                    renderer.update_egui_texture_from_wgpu_texture(
                                        &self.dev.device,
                                        &texture_view,
                                        texture_filter,
                                        tex.id,
                                    );
                                    tex.size = target.dim.to_vec2();
                                } else {
                                    let tex = renderer.register_native_texture(
                                        &self.dev.device,
                                        &texture_view,
                                        texture_filter,
                                    );
                                    editor.canvases.insert(
                                        idx,
                                        SizedTexture {
                                            id: tex,
                                            size: target.dim.to_vec2(),
                                        },
                                    );
                                }
                                return;
                            }
                        }
                    }
                    // bounce the event
                    self.event_loop.send_event(e).unwrap();
                }
                _ => (),
            }
        })
    }
}
