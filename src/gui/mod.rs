mod canvas;
mod silicate;
mod widgets;

use egui::{Frame, *};
use egui_dock::NodePath;
use egui_dock::tab_viewer::OnCloseResponse;
use std::collections::HashMap;
use std::sync::{Arc, mpsc::Sender};

use crate::app::{
    App, AppEvent,
    instance::{Instance, InstanceKey},
};

use canvas::CanvasView;
use silicate::background::BackgroundControl;
use silicate::hierarchy::LayersHierarchy;
use widgets::pane::{button::PaneButton, menu::PaneMenu};

struct ControlsGui;

impl ControlsGui {
    fn layout_info(ui: &mut Ui, instance: &Instance) {
        Grid::new("File Grid").show(ui, |ui| {
            let file = &instance.file;
            ui.label("Name");
            ui.label(file.name.as_deref().unwrap_or("Not Specified"));
            ui.end_row();
            ui.label("Author");
            ui.label(file.author_name.as_deref().unwrap_or("Not Specified"));
            ui.end_row();
            ui.label("Stroke Count");
            ui.label(file.stroke_count.to_string());
            ui.end_row();
            ui.label("Layer Count");
            ui.label(file.layer_count(false).to_string());
            ui.end_row();
            ui.label("Canvas Size");

            let mut dim1 = file.size.width;
            let mut dim2 = file.size.height;

            if !instance.is_upright() {
                std::mem::swap(&mut dim1, &mut dim2);
            }
            ui.label(format!("{} by {}", dim1, dim2));
        });
    }

    fn layout_canvas_control(ui: &mut Ui, instance: &mut Instance) {
        Grid::new("Canvas Grid").show(ui, |ui| {
            ui.label("Flip");
            ui.horizontal(|ui| {
                let is_upright = instance.is_upright();
                let mut horz_var = instance.file.flipped.horizontally;
                let mut vert_var = instance.file.flipped.vertically;

                if !is_upright {
                    std::mem::swap(&mut horz_var, &mut vert_var);
                }

                if ui.button("Horizontal").clicked() {
                    horz_var = !horz_var;
                }
                if ui.button("Vertical").clicked() {
                    vert_var = !vert_var;
                }

                if !is_upright {
                    std::mem::swap(&mut horz_var, &mut vert_var);
                }

                instance.file.flipped.horizontally = horz_var;
                instance.file.flipped.vertically = vert_var;
            });
            ui.end_row();
        });
    }

    fn layout_export_control(ui: &mut Ui, event_sender: &Sender<AppEvent>, instance: &Instance) {
        Grid::new("Share Grid").num_columns(2).show(ui, |ui| {
            ui.label("Actions");
            ui.vertical(|ui| {
                if ui.button("Export View").clicked() {
                    let texture = instance.output_texture.clone();
                    event_sender.send(AppEvent::SaveDialog(texture)).ok();
                }
            });
        });
    }
}

pub struct ViewOptions {
    pub extended_crosshair: bool,
    pub smooth: bool,
    pub grid: bool,
}

struct CanvasGui<'a> {
    app: &'a Arc<App>,
    event_sender: &'a Sender<AppEvent>,
    instances: &'a mut HashMap<InstanceKey, Instance>,
    view_options: &'a mut ViewOptions,
}

impl egui_dock::TabViewer for CanvasGui<'_> {
    type Tab = InstanceKey;

    fn allowed_in_windows(&self, _: &mut Self::Tab) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        let Some(instance) = self.instances.get_mut(tab) else {
            return;
        };

        let mut overlay_ui_left = ui.new_child(UiBuilder::new());
        let mut overlay_ui_right = ui.new_child(UiBuilder::new());

        CanvasView::new(
            *tab,
            instance.canvas.map(Image::from_texture),
            &mut instance.rotation,
        )
        .show_extended_crosshair(self.view_options.extended_crosshair)
        .show_grid(self.view_options.grid)
        .show(ui);

        PaneMenu::new("Actions", PaneButton::menu(), Align::LEFT).show(
            &mut overlay_ui_left,
            |ui| {
                ControlsGui::layout_info(ui, instance);

                ui.separator();

                Grid::new("View Grid").show(ui, |ui| {
                    ui.label("Grid View");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.view_options.grid, false, "Disabled")
                            .changed();
                        ui.selectable_value(&mut self.view_options.grid, true, "Enabled")
                            .changed();
                    });
                    ui.end_row();
                    ui.label("Crosshair");
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.view_options.extended_crosshair,
                            false,
                            "Disabled",
                        )
                        .changed();
                        ui.selectable_value(
                            &mut self.view_options.extended_crosshair,
                            true,
                            "Enabled",
                        )
                        .changed();
                    });
                    ui.end_row();
                    ui.label("Sampling");
                    ui.horizontal(|ui| {
                        let mut changed = false;
                        changed |= ui
                            .selectable_value(&mut self.view_options.smooth, false, "Nearest")
                            .changed();
                        changed |= ui
                            .selectable_value(&mut self.view_options.smooth, true, "Linear")
                            .changed();
                        if changed {
                            self.app.rebind_texture(*tab);
                        }
                    });
                    ui.end_row();
                    ui.label("Rotation");
                    ui.add(
                        Slider::new(&mut instance.rotation, 0.0..=std::f32::consts::TAU)
                            .custom_formatter(|v, _| {
                                let degree = v.to_degrees();
                                format!("{degree:.0}")
                            })
                            .custom_parser(|s| s.parse::<f64>().map(|d| d.to_radians()).ok())
                            .suffix(" deg"),
                    );

                    ui.end_row();
                    ui.label("Theme");
                    ui.horizontal(|ui| {
                        let mut theme = ui.ctx().options(|opt| opt.theme_preference);
                        let mut changed = false;
                        changed |= ui
                            .selectable_value(&mut theme, egui::ThemePreference::System, "System")
                            .changed();
                        changed |= ui
                            .selectable_value(&mut theme, egui::ThemePreference::Light, "Light")
                            .changed();
                        changed |= ui
                            .selectable_value(&mut theme, egui::ThemePreference::Dark, "Dark")
                            .changed();

                        if changed {
                            self.event_sender.send(AppEvent::SetTheme(theme)).unwrap();
                        }
                    });
                });

                ControlsGui::layout_canvas_control(ui, instance);

                ui.separator();

                ControlsGui::layout_export_control(ui, self.event_sender, instance);
            },
        );

        PaneMenu::new("Layers", PaneButton::layers(), Align::RIGHT).show(
            &mut overlay_ui_right,
            |ui| {
                LayersHierarchy {
                    rotation: instance.rotation,
                    flipped: instance.file.flipped,
                    previews: &instance.previews,
                    layers: &mut instance.file.layers,
                }
                .ui(ui, *tab);

                BackgroundControl {
                    file: &mut instance.file,
                }
                .ui(ui);
            },
        );

        instance.submit_to_compositor();
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> OnCloseResponse {
        self.event_sender
            .send(AppEvent::RemoveInstance(*tab))
            .unwrap();
        OnCloseResponse::Close
    }

    fn on_add(&mut self, node_path: egui_dock::NodePath) {
        self.event_sender.send(AppEvent::LoadDialog(node_path)).ok();
    }

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        self.instances
            .get(tab)
            .and_then(|tab| tab.file.name.to_owned())
            .unwrap_or("Untitled Artwork".to_string())
            .into()
    }

    fn id(&mut self, tab: &mut Self::Tab) -> Id {
        Id::new(*tab)
    }
}

pub struct ViewerGui {
    pub app: Arc<App>,
    pub event_sender: Sender<AppEvent>,
    pub instances: HashMap<InstanceKey, Instance>,

    pub view_options: ViewOptions,
    pub canvas_tree: egui_dock::DockState<InstanceKey>,
}

impl ViewerGui {
    fn layout_view(&mut self, ui: &mut Ui) {
        ui.set_min_size(ui.available_size());

        if self.instances.is_empty() {
            ui.allocate_space(vec2(
                0.0,
                ui.available_height() / 2.0 - ui.text_style_height(&style::TextStyle::Button),
            ));
            ui.vertical_centered(|ui| {
                let width = (ui.available_width() - 50.0).max(0.0);
                let height = ui.available_height().max(0.0);

                let max_width = 300.0;
                let max_height = 200.0;

                Area::new(ui.next_auto_id())
                    .movable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ui.ctx(), |ui| {
                        ui.set_width(width.min(max_width));
                        ui.set_height(height.min(max_height));

                        ui.horizontal(|ui| {
                            Label::new(
                                RichText::new("Silicate")
                                    .heading()
                                    .strong()
                                    .color(ui.visuals().strong_text_color()),
                            )
                            .selectable(false)
                            .ui(ui);

                            let git_hash =
                                crate::built_info::GIT_COMMIT_HASH_SHORT.unwrap_or("unknown hash");
                            let pkg_version = crate::built_info::PKG_VERSION;
                            let version_string = format!("v{pkg_version} ({git_hash})");
                            Label::new(
                                RichText::new(version_string)
                                    .small()
                                    .color(ui.visuals().strong_text_color()),
                            )
                            .selectable(false)
                            .ui(ui);
                        });

                        Label::new("GPU-accelerated viewer for the Procreate file format.")
                            .selectable(false)
                            .ui(ui);
                        ui.add_space(10.0);
                        Label::new("Drag and drop Procreate documents to view them.")
                            .selectable(false)
                            .ui(ui);

                        egui::warn_if_debug_build(ui);

                        ui.separator();
                        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                            if ui.button("Open File").clicked() {
                                self.event_sender
                                    .send(AppEvent::LoadDialog(NodePath::MAIN_ROOT))
                                    .ok();
                            }

                            #[cfg(target_arch = "wasm32")]
                            if ui.button("Load Demo File").clicked() {
                                self.event_sender.send(AppEvent::LoadDemoFile).ok();
                            }
                        });
                    });
            });
        } else {
            egui_dock::DockArea::new(&mut self.canvas_tree)
                .id(Id::new("view.dock"))
                .style({
                    let corner_radius = CornerRadius::same(5);

                    let mut style = egui_dock::Style::from_egui(ui.style());
                    style.tab.tab_body.inner_margin = Margin::ZERO;
                    style.tab_bar.height = 50.0;
                    style.tab_bar.hline_color = Color32::TRANSPARENT;
                    style.tab_bar.inner_margin = Margin::same(10);

                    style.tab.spacing = 10.0;

                    style.tab_bar.bg_fill = Color32::TRANSPARENT;

                    style.tab.active.corner_radius = corner_radius;
                    style.tab.active.bg_fill = Color32::TRANSPARENT;
                    style.tab.active.outline_color = Color32::TRANSPARENT;

                    style.tab.inactive.corner_radius = corner_radius;
                    style.tab.inactive.bg_fill = Color32::TRANSPARENT;
                    style.tab.inactive.outline_color = Color32::TRANSPARENT;

                    style.tab.focused.corner_radius = corner_radius;
                    style.tab.focused.outline_color = Color32::TRANSPARENT;
                    style.tab.focused.bg_fill = widgets::ACCENT_COLOR;
                    style.tab.focused.text_color = Color32::WHITE;

                    style.tab.hovered.corner_radius = corner_radius;
                    style.tab.hovered.bg_fill = ui.visuals().widgets.hovered.bg_fill;
                    style.tab.hovered.outline_color = Color32::TRANSPARENT;

                    style.buttons.close_tab_bg_fill = Color32::TRANSPARENT;

                    style
                })
                .show_add_buttons(true)
                .show_leaf_close_all_buttons(false)
                .show_leaf_collapse_buttons(false)
                .show_inside(
                    ui,
                    &mut CanvasGui {
                        app: &self.app,
                        view_options: &mut self.view_options,
                        instances: &mut self.instances,
                        event_sender: &self.event_sender,
                    },
                );
        }
    }

    pub fn layout_gui(&mut self, ui: &mut Ui) {
        CentralPanel::default()
            .frame(Frame::NONE.fill(ui.style().visuals.panel_fill))
            .show_inside(ui, |ui| {
                self.layout_view(ui);

                ui.input(|i| {
                    i.raw.dropped_files.iter().for_each(|file| {
                        if let Some(path) = &file.path {
                            #[cfg(not(target_arch = "wasm32"))]
                            self.event_sender
                                .send(AppEvent::LoadFile {
                                    path: path.to_path_buf(),
                                    node_path: None,
                                })
                                .ok();
                            #[cfg(target_arch = "wasm32")]
                            {
                                self.event_sender
                                    .send(AppEvent::Toast(egui_notify::Toast::error(
                                        "File drag/drop is not supported on this platform.",
                                    )))
                                    .ok();
                                let _ = path;
                            }
                        } else if let Some(bytes) = &file.bytes {
                            #[cfg(target_arch = "wasm32")]
                            self.event_sender
                                .send(AppEvent::LoadFile {
                                    bytes: bytes.clone(),
                                    node_path: None,
                                })
                                .ok();
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                self.event_sender
                                    .send(AppEvent::Toast(egui_notify::Toast::error(
                                        "File drag/drop with in-memory data is not supported on this platform.",
                                    )))
                                    .ok();
                                let _ = bytes;
                            }
                        }
                    });
                })
            });
    }
}
