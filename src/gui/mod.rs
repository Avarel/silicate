pub mod app;
mod canvas;
mod layout;

use self::{
    app::InstanceKey,
    layout::{ViewOptions, ViewerGui},
};
use crate::{
    compositor::{dev::GpuHandle, CompositorPipeline},
    gui::layout::ViewerTab,
};
use app::{App, CompositorApp, UserEvent};
use egui::{load::SizedTexture, FullOutput, ViewportId};
use egui_wgpu::{Renderer, ScreenDescriptor};
use egui_winit::winit::{
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::Window,
};
use parking_lot::{Mutex, RwLock};
use tokio::runtime::Runtime;
use wgpu::Surface;

use crate::winit;
use std::{collections::HashMap, sync::Arc, time::Instant};
use std::{sync::atomic::AtomicUsize, time::Duration};
use winit::{event::WindowEvent, event_loop::ControlFlow};

pub struct AppWin {
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) window: Arc<egui_winit::winit::window::Window>,
    pub(crate) integration: egui_winit::State,
    pub(crate) screen_descriptor: egui_wgpu::ScreenDescriptor,
    pub(crate) renderer: egui_wgpu::Renderer,
    pub(crate) surface_config: wgpu::SurfaceConfiguration,
}

pub struct AppInstance {
    pub app: Arc<App>,
    pub rendering: AppWin,
    pub(crate) editor: layout::ViewerGui,
}

impl AppInstance {
    pub fn new(
        dev: GpuHandle,
        rt: Arc<Runtime>,
        surface: Surface<'static>,
        window: Arc<Window>,
        event_loop_proxy: EventLoopProxy<UserEvent>,
    ) -> Self {
        let surface_caps = surface.get_capabilities(&dev.adapter);
        let surface_format = surface_caps.formats[0];
        let surface_alpha = surface_caps.alpha_modes[0];
        let surface_config = {
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
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [surface_config.width, surface_config.height],
            pixels_per_point: window.scale_factor() as f32,
        };
        surface.configure(&dev.device, &surface_config);

        let integration = egui_winit::State::new(
            egui::Context::default(),
            ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        let renderer = Renderer::new(&dev.device, surface_format, None, 1, false);

        let app = Arc::new(App {
            compositor: Arc::new(CompositorApp {
                instances: RwLock::new(HashMap::new()),
                pipeline: CompositorPipeline::new(&dev),
                curr_id: AtomicUsize::new(0),
            }),
            rt,
            dev: Arc::new(dev),
            toasts: Mutex::new(egui_notify::Toasts::default()),
            added_instances: Mutex::new(Vec::with_capacity(1)),
            event_loop: event_loop_proxy,
        });

        let editor = ViewerGui {
            app: app.clone(),
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

        let app_instance = AppInstance {
            app,
            rendering: AppWin {
                surface,
                window,
                integration,
                screen_descriptor,
                surface_config,
                renderer,
            },
            editor,
        };

        app_instance
            .app
            .rt
            .spawn(app_instance.app.compositor.clone().rendering_thread());

        app_instance
    }

    pub fn handle_event(
        &mut self,
        event: egui_winit::winit::event::WindowEvent,
        eltarget: &ActiveEventLoop,
    ) {
        match event {
            WindowEvent::RedrawRequested => {
                let output_frame = match self.rendering.surface.get_current_texture() {
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

                let input = self
                    .rendering
                    .integration
                    .take_egui_input(&self.rendering.window);

                self.rendering.integration.egui_ctx().begin_pass(input);
                self.editor
                    .layout_gui(&self.rendering.integration.egui_ctx());
                self.app
                    .toasts
                    .lock()
                    .show(&self.rendering.integration.egui_ctx());
                let FullOutput {
                    platform_output,
                    textures_delta,
                    shapes,
                    pixels_per_point,
                    viewport_output,
                } = self.rendering.integration.egui_ctx().end_pass();

                let repaint_after = viewport_output[&ViewportId::ROOT].repaint_delay;

                if repaint_after.is_zero() {
                    self.rendering.window.request_redraw();
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

                self.rendering
                    .integration
                    .handle_platform_output(&self.rendering.window, platform_output);

                // Draw the GUI onto the output texture.
                let paint_jobs = self
                    .rendering
                    .integration
                    .egui_ctx()
                    .tessellate(shapes, pixels_per_point);

                // Upload all resources for the GPU.
                for (id, image_delta) in textures_delta.set {
                    self.rendering.renderer.update_texture(
                        &self.app.dev.device,
                        &self.app.dev.queue,
                        id,
                        &image_delta,
                    );
                }
                for id in textures_delta.free {
                    self.rendering.renderer.free_texture(&id);
                }

                self.app.dev.queue.submit(Some({
                    let mut encoder = self
                        .app
                        .dev
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

                    self.rendering.renderer.update_buffers(
                        &self.app.dev.device,
                        &self.app.dev.queue,
                        &mut encoder,
                        &paint_jobs,
                        &self.rendering.screen_descriptor,
                    );

                    self.rendering.renderer.render(
                        &mut encoder
                            .begin_render_pass(&wgpu::RenderPassDescriptor {
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
                            })
                            .forget_lifetime(),
                        &paint_jobs,
                        &self.rendering.screen_descriptor,
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
                    self.rendering.surface_config.width = size.width;
                    self.rendering.surface_config.height = size.height;
                    self.rendering.screen_descriptor.size_in_pixels = [size.width, size.height];
                    self.rendering
                        .surface
                        .configure(&self.app.dev.device, &self.rendering.surface_config);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.rendering.screen_descriptor.pixels_per_point = scale_factor as f32;
                self.rendering
                    .surface
                    .configure(&self.app.dev.device, &self.rendering.surface_config);
            }
            WindowEvent::DroppedFile(file) => {
                println!("File dropped: {:?}", file.as_path().display().to_string());
                self.app.rt.spawn({
                    let app = self.app.clone();
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
                let response = self
                    .rendering
                    .integration
                    .on_window_event(&self.rendering.window, &event);
                if response.repaint {
                    self.rendering.window.request_redraw();
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
            app::UserEvent::RemoveInstance(idx) => {
                self.editor.remove_index(idx);
            }
            e @ app::UserEvent::RebindTexture(idx) => {
                // Updates textures bound for EGUI rendering
                // Do not block on any locks/rwlocks since we do not want to block
                // the GUI thread when the renderer is potentially taking a long
                // time to render a frame.
                let texture_filter = if self.editor.view_options.smooth {
                    wgpu::FilterMode::Linear
                } else {
                    wgpu::FilterMode::Nearest
                };

                let instances = self.app.compositor.instances.read();
                if let Some(instance) = instances.get(&idx) {
                    if let Some(target) = instance.target.try_lock() {
                        if let Some(output) = target.output.as_ref() {
                            let texture_view = output.texture.create_srgb_view();

                            if let Some(tex) = self.editor.canvases.get_mut(&idx) {
                                self.rendering
                                    .renderer
                                    .update_egui_texture_from_wgpu_texture(
                                        &self.app.dev.device,
                                        &texture_view,
                                        texture_filter,
                                        tex.id,
                                    );
                                tex.size = target.dim.to_vec2();
                            } else {
                                let tex = self.rendering.renderer.register_native_texture(
                                    &self.app.dev.device,
                                    &texture_view,
                                    texture_filter,
                                );
                                self.editor.canvases.insert(
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
                self.app.event_loop.send_event(e).unwrap();
            }
        }
    }
}
