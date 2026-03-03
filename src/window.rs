mod dialog;

use crate::app::{App, AppEvent};
use crate::gui::{ViewOptions, ViewerGui};
use dialog::Dialog;
use egui::{Vec2, load::SizedTexture};
use egui_notify::Toasts;
use eframe::wgpu;
use silicate_compositor::tex::TextureExt;
use tokio::runtime::Runtime;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, mpsc::Sender},
};

pub struct AppInstance {
    app: Arc<App>,
    viewer: ViewerGui,
    toasts: Toasts,
    event_sender: Sender<AppEvent>,
    #[allow(dead_code)]
    renderer: Option<eframe::egui_wgpu::Renderer>,
}

impl AppInstance {
    /// Create a new AppInstance for use with eframe
    pub fn new_for_eframe(
        device: wgpu::Device,
        queue: wgpu::Queue,
        event_sender: Sender<AppEvent>,
    ) -> Self {
        let app = Arc::new(App::new(
            device.clone(),
            queue.clone(),
            event_sender.clone(),
        ));

        let viewer = ViewerGui {
            app: app.clone(),
            instances: HashMap::new(),
            view_options: ViewOptions {
                smooth: false,
                grid: true,
                extended_crosshair: false,
            },
            canvas_tree: egui_dock::DockState::new(Vec::new()),
            event_sender: event_sender.clone(),
        };

        AppInstance {
            app,
            viewer,
            toasts: Toasts::new().with_anchor(egui_notify::Anchor::BottomLeft),
            event_sender,
            renderer: None,
        }
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
                    self.event_sender
                        .send(AppEvent::NewView(
                            egui_dock::SurfaceIndex::main(),
                            egui_dock::NodeIndex::root(),
                            key,
                        ))
                        .unwrap();
                }
            }
        }
    }

    pub fn handle_user_event(&mut self, event: AppEvent, rt: &Runtime, frame: &mut eframe::Frame) {
        match event {
            AppEvent::RemoveInstance(idx) => {
                self.viewer.instances.remove(&idx);
            }
            AppEvent::RebindTexture(idx) => {
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
                let texture_view = output.create_default_view();
                let size = Vec2::new(output.size().width as f32, output.size().height as f32);

                if let Some(eframe::egui_wgpu::RenderState {
                    device,
                    renderer,
                    ..
                }) = frame.wgpu_render_state() {
                    let mut renderer = renderer.write();
                    if let Some(tex) = &mut instance.canvas {
                        renderer.update_egui_texture_from_wgpu_texture(
                            device,
                            &texture_view,
                            texture_filter,
                            tex.id,
                        );
                        tex.size = size;
                    } else {
                        let id = renderer.register_native_texture(
                            device,
                            &texture_view,
                            texture_filter,
                        );
                        instance.canvas = Some(SizedTexture { id, size });
                    }
                }
            }
            AppEvent::RebindPreviews(idx) => {
                let Some(instance) = self.viewer.instances.get_mut(&idx) else {
                    return;
                };

                let Some(preview_texture) = &instance.preview_textures else {
                    return;
                };

                let texture_filter = wgpu::FilterMode::Linear;
                let size = Vec2::new(
                    preview_texture.size().width as f32,
                    preview_texture.size().height as f32,
                );

                if let Some(eframe::egui_wgpu::RenderState {
                    device,
                    renderer,
                    ..
                }) = frame.wgpu_render_state() {
                    let mut renderer = renderer.write();
                    for i in 0..preview_texture.size().depth_or_array_layers {
                        let texture_view = preview_texture.create_view_layer(i);
                        if let Some(tex) = instance.previews.get_mut(&i) {
                            renderer.update_egui_texture_from_wgpu_texture(
                                &device,
                                &texture_view,
                                texture_filter,
                                tex.id,
                            );
                            tex.size = size;
                        } else {
                            let id = renderer.register_native_texture(
                                &device,
                                &texture_view,
                                texture_filter,
                            );
                            instance.previews.insert(i, SizedTexture { id, size });
                        }
                    }
                }
            }
            AppEvent::NewInstance(instance_key, instance, compositor) => {
                rt.spawn(compositor.rendering_thread(instance.output_texture.clone()));
                self.viewer.instances.insert(instance_key, instance);
                self.event_sender
                    .send(AppEvent::RebindPreviews(instance_key))
                    .unwrap();
                self.event_sender
                    .send(AppEvent::RebindTexture(instance_key))
                    .unwrap();
            }
            AppEvent::Toast(toast) => {
                self.toasts.add(toast);
            }
            AppEvent::LoadFile(path) => {
                match self.app.load_file(path) {
                    Err(err) => {
                        self.toasts
                            .error(format!("File from drag/drop failed to load. Reason: {err}"));
                    }
                    Ok(key) => {
                        self.toasts.success("Loaded file from drag/drop.");
                        self.event_sender
                            .send(AppEvent::NewView(
                                egui_dock::SurfaceIndex::main(),
                                egui_dock::NodeIndex::root(),
                                key,
                            ))
                            .unwrap();
                    }
                }
            }
            AppEvent::LoadDialog(surface, node) => {
                rt.spawn(Dialog::new(self.event_sender.clone()).load_dialog(
                    self.app.clone(),
                    surface,
                    node,
                ));
            }
            AppEvent::SaveDialog(texture) => {
                if let Some(wgpu_render_state) = frame.wgpu_render_state() {
                    rt.spawn(Dialog::new(self.event_sender.clone()).save_dialog(
                        wgpu_render_state.device.clone(),
                        wgpu_render_state.queue.clone(),
                        texture,
                    ));
                }
            }
            AppEvent::NewView(surface, node, id) => {
                self.viewer
                    .canvas_tree
                    .set_focused_node_and_surface((surface, node));
                self.viewer.canvas_tree.push_to_focused_leaf(id);
            }
        }
    }

    /// Render the GUI using the viewer
    pub fn render_gui(&mut self, ctx: &egui::Context) {
        self.viewer.layout_gui(ctx);
        self.toasts.show(ctx);
    }
}
