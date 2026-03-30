mod dialog;
pub mod prefs;

use crate::app::compositor::CompositorApp;
use crate::app::instance::InstanceKey;
use crate::app::{App, AppEvent};
use crate::gui::{ViewOptions, ViewerGui};
use crate::window::prefs::PersistedPreferences;
use dialog::Dialog;
use eframe::wgpu;
use egui::{Vec2, load::SizedTexture};
use egui_notify::Toasts;
use silicate_compositor::tex::TextureExt;

use std::{
    collections::HashMap,
    sync::{Arc, mpsc::Sender},
};

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

pub struct AppInstance {
    app: Arc<App>,
    viewer: ViewerGui,
    toasts: Toasts,
    event_sender: Sender<AppEvent>,
    compositors: HashMap<InstanceKey, (CompositorApp, wgpu::Texture)>,
}

impl AppInstance {
    /// Create a new AppInstance for use with eframe
    pub fn new_for_eframe(
        device: wgpu::Device,
        queue: wgpu::Queue,
        ctx: &egui::Context,
        event_sender: &Sender<AppEvent>,
    ) -> Self {
        let app = Arc::new(App::new(
            device.clone(),
            queue.clone(),
            event_sender.clone(),
        ));

        let preferences = PersistedPreferences::load(ctx).unwrap_or_default();

        ctx.set_theme(preferences.theme);

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
            event_sender: event_sender.clone(),
            compositors: HashMap::new(),
        }
    }

    pub fn compositors_mut(&mut self) -> &mut HashMap<InstanceKey, (CompositorApp, wgpu::Texture)> {
        &mut self.compositors
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_files(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            self.event_sender
                .send(AppEvent::LoadFile {
                    path,
                    node_path: None,
                })
                .unwrap();
        }
    }

    pub fn handle_user_event(
        &mut self,
        event: AppEvent,
        rt: &super::UnifiedRuntime,
        ctx: &egui::Context,
        frame: &mut eframe::Frame,
    ) {
        #[cfg(target_arch = "wasm32")]
        // rt is unused on wasm, so we bind it to _ to avoid a warning.
        // On native, we use rt to spawn blocking tasks for file loading and saving.
        let _ = rt;

        match event {
            AppEvent::RemoveInstance(idx) => {
                self.viewer.instances.remove(&idx);
                self.compositors.remove(&idx);
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
                    device, renderer, ..
                }) = frame.wgpu_render_state()
                {
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
                        let id =
                            renderer.register_native_texture(device, &texture_view, texture_filter);
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
                    device, renderer, ..
                }) = frame.wgpu_render_state()
                {
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
                #[cfg(not(target_arch = "wasm32"))]
                rt.spawn(compositor.rendering_thread(instance.output_texture.clone()));
                #[cfg(target_arch = "wasm32")]
                self.compositors
                    .insert(instance_key, (compositor, instance.output_texture.clone()));

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
            #[cfg(not(target_arch = "wasm32"))]
            AppEvent::LoadFile {
                path,
                node_path
            } => match self.app.load_file(&path) {
                Err(err) => {
                    self.toasts
                        .error(format!("File from drag/drop failed to load. Reason: {err}"));
                }
                Ok(key) => {
                    self.toasts.success("Loaded file from drag/drop.");
                    self.event_sender
                        .send(AppEvent::NewView(
                            node_path.unwrap_or(egui_dock::NodePath::MAIN_ROOT),
                            key,
                        ))
                        .unwrap();
                }
            },
            #[cfg(target_arch = "wasm32")]
            AppEvent::LoadFile { bytes, node_path } => match self.app.load_bytes(&bytes) {
                Err(err) => {
                    self.toasts
                        .error(format!("File from drag/drop failed to load. Reason: {err}"));
                }
                Ok(key) => {
                    self.toasts.success("Loaded file from drag/drop.");
                    self.event_sender
                        .send(AppEvent::NewView(
                            node_path.unwrap_or(egui_dock::NodePath::MAIN_ROOT),
                            key,
                        ))
                        .unwrap();
                }
            },
            AppEvent::LoadDialog(node_path) => {
                let dialog = Dialog::new(self.event_sender.clone()).load_dialog(node_path);
                rt.spawn(dialog);
            }
            AppEvent::SaveDialog(texture) => {
                if let Some(eframe::egui_wgpu::RenderState { device, queue, .. }) =
                    frame.wgpu_render_state()
                {
                    let dialog = Dialog::new(self.event_sender.clone()).save_dialog(
                        device.clone(),
                        queue.clone(),
                        texture,
                    );
                    rt.spawn(dialog);
                }
            }
            AppEvent::NewView(node_path, id) => {
                self.viewer
                    .canvas_tree
                    .set_focused_node_and_surface(node_path);
                self.viewer.canvas_tree.push_to_focused_leaf(id);
            }
            AppEvent::SetTheme(theme) => {
                ctx.set_theme(theme);
                PersistedPreferences { theme }.store(ctx);
            }
            #[cfg(target_arch = "wasm32")]
            AppEvent::LoadDemoFile => {
                use crate::web::fetch_demo_file;
                let event_sender = self.event_sender.clone();
                rt.spawn(async move {
                    match fetch_demo_file().await {
                        Ok(bytes) => {
                            event_sender
                                .send(AppEvent::LoadFile {
                                    bytes: Arc::from(bytes),
                                    node_path: None,
                                })
                                .unwrap();
                        }
                        Err(_) => {
                            event_sender
                                .send(AppEvent::Toast(egui_notify::Toast::error(
                                    "Failed to fetch demo file.",
                                )))
                                .unwrap();
                        }
                    }
                });
            }
        }
    }

    /// Render the GUI using the viewer
    pub fn render_gui(&mut self, ui: &mut egui::Ui) {
        self.viewer.layout_gui(ui);
        self.toasts.show(ui.ctx());
    }
}
