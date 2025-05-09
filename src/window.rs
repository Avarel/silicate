use crate::winit;

use crate::app::{App, UserEvent};
use crate::gui::{ViewOptions, ViewerGui};
use egui::{load::SizedTexture, FullOutput, Vec2, ViewportId};
use egui_notify::{Toast, Toasts};
use egui_wgpu::{wgpu, Renderer, ScreenDescriptor};

use silicate_compositor::dev::{GpuDispatch, GpuHandle};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::UnboundedReceiver;
use wgpu::Surface;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy},
    window::Window,
};

struct WindowBundle {
    dispatch: GpuDispatch,
    surface: wgpu::Surface<'static>,
    window: Arc<egui_winit::winit::window::Window>,
    integration: egui_winit::State,
    screen_descriptor: egui_wgpu::ScreenDescriptor,
    renderer: egui_wgpu::Renderer,
    surface_config: wgpu::SurfaceConfiguration,
}

impl WindowBundle {
    fn new(dev: &GpuHandle, surface: Surface<'static>, window: Arc<Window>) -> Self {
        let surface_caps = surface.get_capabilities(&dev.adapter);
        let surface_format = surface_caps.formats[0];
        let surface_alpha = surface_caps.alpha_modes[0];
        let surface_present: wgpu::PresentMode = surface_caps.present_modes[0];
        let surface_config = {
            let window_size = window.inner_size();
            wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width: window_size.width,
                height: window_size.height,
                present_mode: surface_present,
                view_formats: Vec::new(),
                alpha_mode: surface_alpha,
                desired_maximum_frame_latency: 0,
            }
        };
        dbg!(&surface_caps, &surface_config);

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [surface_config.width, surface_config.height],
            pixels_per_point: window.scale_factor() as f32,
        };
        surface.configure(&dev.dispatch.device(), &surface_config);

        let integration = egui_winit::State::new(
            egui::Context::default(),
            ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        let renderer = Renderer::new(&dev.dispatch.device(), surface_format, None, 1, false);

        Self {
            dispatch: dev.dispatch.clone(),
            surface,
            window,
            integration,
            screen_descriptor,
            surface_config,
            renderer,
        }
    }
}

pub struct AppInstance {
    app: Arc<App>,
    window: WindowBundle,
    viewer: ViewerGui,
    rt: Arc<Runtime>,
    toasts: Toasts,
}

impl AppInstance {
    pub fn new(
        dev: GpuHandle,
        rt: Arc<Runtime>,
        surface: Surface<'static>,
        window: Arc<Window>,
        event_loop_proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        let window = WindowBundle::new(&dev, surface, window);

        let (tx_instances, rx_instances) = tokio::sync::mpsc::channel(2);

        let app = Arc::new(App::new(
            dev.dispatch,
            rt.clone(),
            tx_instances,
            event_loop_proxy,
        ));

        let viewer = ViewerGui {
            app: app.clone(),
            instances: HashMap::new(),
            view_options: ViewOptions {
                smooth: false,
                grid: true,
                extended_crosshair: false,
            },
            new_instances: rx_instances,
            canvas_tree: egui_dock::DockState::new(Vec::new()),
        };

        let app_instance = AppInstance {
            app,
            window,
            rt,
            viewer,
            toasts: Toasts::new().with_anchor(egui_notify::Anchor::BottomLeft),
        };

        app_instance
    }

    pub fn load_files(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            match self.app.load_file(path) {
                Err(err) => {
                    self.toasts
                        .error(format!("File from drag/drop failed to load. Reason: {err}"));
                }
                Ok(key) => {
                    self.toasts.success("Loaded file from command line.");
                    self.app
                        .new_instances
                        .blocking_send((
                            egui_dock::SurfaceIndex::main(),
                            egui_dock::NodeIndex::root(),
                            key,
                        ))
                        .unwrap();
                }
            }
        }
    }

    pub fn handle_event(
        &mut self,
        event: egui_winit::winit::event::WindowEvent,
        eltarget: &ActiveEventLoop,
    ) {
        match event {
            WindowEvent::RedrawRequested => {
                let output_frame = match self.window.surface.get_current_texture() {
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

                let input = self.window.integration.take_egui_input(&self.window.window);

                self.window.integration.egui_ctx().begin_pass(input);
                self.viewer.layout_gui(&self.window.integration.egui_ctx());

                self.toasts.show(&self.window.integration.egui_ctx());

                let FullOutput {
                    platform_output,
                    textures_delta,
                    shapes,
                    pixels_per_point,
                    viewport_output,
                } = self.window.integration.egui_ctx().end_pass();

                let repaint_after = viewport_output[&ViewportId::ROOT].repaint_delay;

                if repaint_after.is_zero() {
                    self.window.window.request_redraw();
                    eltarget.set_control_flow(ControlFlow::Poll);
                } else if let Some(repaint_after_instant) =
                    Instant::now().checked_add(repaint_after)
                {
                    eltarget.set_control_flow(ControlFlow::WaitUntil(repaint_after_instant));
                } else {
                    eltarget.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_secs(1),
                    ));
                }

                self.window
                    .integration
                    .handle_platform_output(&self.window.window, platform_output);

                let dispatch = &self.window.dispatch;

                // Draw the GUI onto the output texture.
                let paint_jobs = self
                    .window
                    .integration
                    .egui_ctx()
                    .tessellate(shapes, pixels_per_point);

                // Upload all resources for the GPU.
                for (id, image_delta) in textures_delta.set {
                    self.window.renderer.update_texture(
                        dispatch.device(),
                        dispatch.queue(),
                        id,
                        &image_delta,
                    );
                }
                for id in textures_delta.free {
                    self.window.renderer.free_texture(&id);
                }

                self.window.dispatch.submit_queue(|encoder| {
                    self.window.renderer.update_buffers(
                        dispatch.device(),
                        dispatch.queue(),
                        encoder,
                        &paint_jobs,
                        &self.window.screen_descriptor,
                    );

                    self.window.renderer.render(
                        &mut encoder
                            .begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: None,
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &output_view,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                        store: wgpu::StoreOp::Store,
                                    },
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                            })
                            .forget_lifetime(),
                        &paint_jobs,
                        &self.window.screen_descriptor,
                    );
                });
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
                    self.window.surface_config.width = size.width;
                    self.window.surface_config.height = size.height;
                    self.window.screen_descriptor.size_in_pixels = [size.width, size.height];
                    self.window
                        .surface
                        .configure(&self.window.dispatch.device(), &self.window.surface_config);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.window.screen_descriptor.pixels_per_point = scale_factor as f32;
                self.window
                    .surface
                    .configure(&self.window.dispatch.device(), &self.window.surface_config);
            }
            WindowEvent::DroppedFile(file) => {
                println!("File dropped: {:?}", file.as_path().display().to_string());
                self.rt.spawn({
                    let app = self.app.clone();
                    async move {
                        match app.load_file(file) {
                            Err(_) => {
                                app.send_toast(Toast::error("File from drag/drop failed to load."));
                            }
                            Ok(key) => {
                                app.send_toast(Toast::success("Loaded file from drag/drop."));
                                app.new_instances
                                    .send((
                                        egui_dock::SurfaceIndex::main(),
                                        egui_dock::NodeIndex::root(),
                                        key,
                                    ))
                                    .await
                                    .unwrap();
                            }
                        }
                    }
                });
            }
            _ => {
                let response = self
                    .window
                    .integration
                    .on_window_event(&self.window.window, &event);
                if response.repaint {
                    self.window.window.request_redraw();
                    eltarget.set_control_flow(ControlFlow::Poll);
                } else {
                    eltarget.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_secs(1),
                    ))
                }
            }
        }
    }

    pub fn handle_user_event(&mut self, event: UserEvent) {
        match event {
            UserEvent::RemoveInstance(idx) => {
                self.viewer.instances.remove(&idx);
            }
            UserEvent::RebindTexture(idx) => {
                // Updates textures bound for EGUI rendering
                // Do not block on any locks/rwlocks since we do not want to block
                // the GUI thread when the renderer is potentially taking a long
                // time to render a frame.
                let texture_filter = if self.viewer.view_options.smooth {
                    wgpu::FilterMode::Linear
                } else {
                    wgpu::FilterMode::Nearest
                };

                let Some(instance) = self.viewer.instances.get_mut(&idx) else {
                    return;
                };

                let output = &instance.output_texture;
                let texture_view = output.create_srgb_view();
                let size = Vec2::new(output.width() as f32, output.height() as f32);

                if let Some(tex) = &mut instance.canvas {
                    self.window.renderer.update_egui_texture_from_wgpu_texture(
                        &self.window.dispatch.device(),
                        &texture_view,
                        texture_filter,
                        tex.id,
                    );
                    tex.size = size;
                } else {
                    let id = self.window.renderer.register_native_texture(
                        &self.window.dispatch.device(),
                        &texture_view,
                        texture_filter,
                    );
                    instance.canvas = Some(SizedTexture { id, size });
                }
            }
            UserEvent::RebindPreviews(idx) => {
                let Some(instance) = self.viewer.instances.get_mut(&idx) else {
                    return;
                };

                let Some(preview_texture) = &instance.preview_textures else {
                    return;
                };

                let texture_filter = wgpu::FilterMode::Linear;
                let size = Vec2::new(
                    preview_texture.width() as f32,
                    preview_texture.height() as f32,
                );

                for i in 0..preview_texture.layers() {
                    let texture_view = preview_texture.create_srgb_view_layer(i);
                    if let Some(tex) = instance.previews.get_mut(&i) {
                        self.window.renderer.update_egui_texture_from_wgpu_texture(
                            &self.window.dispatch.device(),
                            &texture_view,
                            texture_filter,
                            tex.id,
                        );
                        tex.size = size;
                    } else {
                        let id = self.window.renderer.register_native_texture(
                            &self.window.dispatch.device(),
                            &texture_view,
                            texture_filter,
                        );
                        instance.previews.insert(i, SizedTexture { id, size });
                    }
                }
            }
            UserEvent::NewInstance(instance_key, instance) => {
                self.viewer.instances.insert(instance_key, instance);
                self.app
                    .event_loop
                    .send_event(UserEvent::RebindPreviews(instance_key))
                    .unwrap();
                self.app
                    .event_loop
                    .send_event(UserEvent::RebindTexture(instance_key))
                    .unwrap();
            }
            UserEvent::Toast(toast) => {
                self.toasts.add(toast);
            }
        }
    }
}
