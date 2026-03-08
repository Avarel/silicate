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

    pub fn load_files(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            self.event_sender
                .send(AppEvent::LoadFilePath {
                    path,
                    surface_index: None,
                    node_index: None,
                })
                .unwrap();
        }
    }

    pub fn handle_user_event(
        &mut self,
        event: AppEvent,
        rt: &Runtime,
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
            AppEvent::LoadFilePath {
                path,
                surface_index,
                node_index,
            } => match self.app.load_file(&path) {
                Err(err) => {
                    self.toasts
                        .error(format!("File from drag/drop failed to load. Reason: {err}"));
                }
                Ok(key) => {
                    self.toasts.success("Loaded file from drag/drop.");
                    self.event_sender
                        .send(AppEvent::NewView(
                            surface_index.unwrap_or_else(egui_dock::SurfaceIndex::main),
                            node_index.unwrap_or_else(egui_dock::NodeIndex::root),
                            key,
                        ))
                        .unwrap();
                }
            },
            AppEvent::LoadFileBytes {
                bytes,
                surface_index,
                node_index,
            } => match self.app.load_bytes(&bytes) {
                Err(err) => {
                    self.toasts
                        .error(format!("File from drag/drop failed to load. Reason: {err}"));
                }
                Ok(key) => {
                    self.toasts.success("Loaded file from drag/drop.");
                    self.event_sender
                        .send(AppEvent::NewView(
                            surface_index.unwrap_or_else(egui_dock::SurfaceIndex::main),
                            node_index.unwrap_or_else(egui_dock::NodeIndex::root),
                            key,
                        ))
                        .unwrap();
                }
            },
            AppEvent::LoadDialog(surface, node) => {
                let dialog = Dialog::new(self.event_sender.clone()).load_dialog(surface, node);
                #[cfg(not(target_arch = "wasm32"))]
                rt.spawn(dialog);
                #[cfg(target_arch = "wasm32")]
                wasm_bindgen_futures::spawn_local(dialog);
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
                    #[cfg(not(target_arch = "wasm32"))]
                    rt.spawn(dialog);
                    #[cfg(target_arch = "wasm32")]
                    wasm_bindgen_futures::spawn_local(dialog);
                }
            }
            AppEvent::NewView(surface, node, id) => {
                self.viewer
                    .canvas_tree
                    .set_focused_node_and_surface((surface, node));
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
                wasm_bindgen_futures::spawn_local(async move {
                    match fetch_demo_file().await {
                        Ok(bytes) => {
                            event_sender
                                .send(AppEvent::LoadFileBytes {
                                    bytes: Arc::from(bytes),
                                    surface_index: None,
                                    node_index: None,
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
            #[allow(unreachable_patterns)]
            _ => {
                log::error!("Received unhandled AppEvent: {:?}", event);
            }
        }
    }

    /// Render the GUI using the viewer
    pub fn render_gui(&mut self, ctx: &egui::Context) {
        self.viewer.layout_gui(ctx);
        self.toasts.show(ctx);
    }
}
