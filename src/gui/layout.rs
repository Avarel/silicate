use egui::load::SizedTexture;
use egui::{Frame, *};
use egui_dock::{NodeIndex, SurfaceIndex};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;

use crate::app::{
    instance::{Instance, InstanceKey},
    App, UserEvent,
};

use super::canvas::CanvasView;
use super::silicate::background::BackgroundControl;
use super::silicate::hierarchy::LayersHierarchy;
use super::widgets::pane::{button::PaneButton, menu::PaneMenu};

struct ControlsGui;

impl ControlsGui {
    fn layout_info(ui: &mut Ui, instance: &Instance) {
        Grid::new("File Grid").show(ui, |ui| {
            let file = instance.file.read();
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
                let mut flip_reload = false;

                if ui.button("Horizontal").clicked() {
                    if instance.is_upright() {
                        instance.flipped.horizontally = !instance.flipped.horizontally;
                    } else {
                        instance.flipped.vertically = !instance.flipped.vertically;
                    }
                    instance.tick_change(true);
                    flip_reload = true;
                }
                if ui.button("Vertical").clicked() {
                    if instance.is_upright() {
                        instance.flipped.vertically = !instance.flipped.vertically;
                    } else {
                        instance.flipped.horizontally = !instance.flipped.horizontally;
                    }
                    instance.tick_change(true);
                    flip_reload = true;
                }

                if flip_reload {
                    instance
                        .target
                        .lock()
                        .set_flipped(instance.flipped.horizontally, instance.flipped.vertically);
                }
            });
            ui.end_row();
        });
    }

    fn layout_export_control(ui: &mut Ui, app: &Arc<App>, instance: &Instance) {
        Grid::new("Share Grid").num_columns(2).show(ui, |ui| {
            ui.label("Actions");
            ui.vertical(|ui| {
                if ui.button("Export View").clicked() {
                    let texture = instance.output_texture.clone();
                    app.rt.spawn({
                        let app = app.clone();
                        async move { app.save_dialog(texture).await }
                    });
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
    canvases: &'a mut HashMap<InstanceKey, SizedTexture>,
    previews: &'a HashMap<(InstanceKey, u32), SizedTexture>,
    instances: &'a mut HashMap<InstanceKey, Instance>,
    view_options: &'a mut ViewOptions,
}

impl egui_dock::TabViewer for CanvasGui<'_> {
    type Tab = InstanceKey;

    fn allowed_in_windows(&self, _: &mut Self::Tab) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        let tex = self.canvases.get(tab);

        let Some(instance) = self.instances.get_mut(tab) else {
            return;
        };

        let mut overlay_ui_left = ui.new_child(UiBuilder::new());
        let mut overlay_ui_right = ui.new_child(UiBuilder::new());

        CanvasView::new(
            *tab,
            tex.copied().map(Image::from_texture),
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
                    ui.checkbox(&mut self.view_options.grid, "Enable");
                    ui.end_row();
                    ui.label("Extended Crosshair");
                    ui.checkbox(&mut self.view_options.extended_crosshair, "Enable");
                    ui.end_row();
                    ui.label("Smooth Sampling");
                    if ui
                        .checkbox(&mut self.view_options.smooth, "Enable")
                        .changed()
                    {
                        self.app.rebind_texture(*tab);
                    }
                    ui.end_row();
                    ui.label("Rotation");
                    ui.add(
                        Slider::new(&mut instance.rotation, 0.0..=std::f32::consts::TAU)
                            .custom_formatter(|v, _| {
                                let degree = v.to_degrees();
                                format!("{degree:.0}")
                            })
                            .suffix(" deg"),
                    );
                });

                ControlsGui::layout_canvas_control(ui, instance);

                ui.separator();

                ControlsGui::layout_export_control(ui, self.app, instance);
            },
        );

        PaneMenu::new("Layers", PaneButton::layers(), Align::RIGHT).show(
            &mut overlay_ui_right,
            |ui| {
                let mut file = instance.file.write();
                let mut changed = false;

                LayersHierarchy {
                    instance: &instance,
                    previews: self.previews,
                    layers: &mut file.layers,
                    addendum: &instance.addendum,
                }
                .ui(ui, *tab, &mut changed);

                BackgroundControl { file: &mut file }.ui(ui, &mut changed);

                instance.tick_change(changed);
            },
        );
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> bool {
        self.app
            .event_loop
            .send_event(UserEvent::RemoveInstance(*tab))
            .unwrap();
        true
    }

    fn on_add(&mut self, surface: egui_dock::SurfaceIndex, node: egui_dock::NodeIndex) {
        self.app.rt.spawn({
            let app = self.app.clone();
            async move { app.load_dialog(surface, node).await }
        });
    }

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        self.instances
            .get(tab)
            .and_then(|tab| tab.file.read().name.to_owned())
            .unwrap_or("Untitled Artwork".to_string())
            .into()
    }

    fn id(&mut self, tab: &mut Self::Tab) -> Id {
        Id::new(*tab)
    }
}

pub struct ViewerGui {
    pub app: Arc<App>,

    pub previews: HashMap<(InstanceKey, u32), SizedTexture>,
    pub canvases: HashMap<InstanceKey, SizedTexture>,
    pub active_canvas: InstanceKey,
    pub view_options: ViewOptions,
    pub canvas_tree: egui_dock::DockState<InstanceKey>,
    pub(crate) new_instances: Receiver<(SurfaceIndex, NodeIndex, InstanceKey)>,
}

impl ViewerGui {
    pub fn remove_index(&mut self, index: InstanceKey) {
        self.canvases.remove(&index);
        self.app.compositor.instances.write().remove(&index);
    }

    fn layout_view(&mut self, ui: &mut Ui) {
        ui.set_min_size(ui.available_size());

        let mut instances = self.app.compositor.instances.write();

        if instances.is_empty() {
            ui.allocate_space(vec2(
                0.0,
                ui.available_height() / 2.0 - ui.text_style_height(&style::TextStyle::Button),
            ));
            ui.vertical_centered(|ui| {
                ui.label("Drag and drop Procreate file to view it.");
                if ui.button("Load Procreate File").clicked() {
                    self.app.rt.spawn({
                        let app = self.app.clone();
                        async move {
                            app.load_dialog(SurfaceIndex::main(), NodeIndex::root())
                                .await
                        }
                    });
                }
            });
        } else {
            while let Ok((surface, node, id)) = self.new_instances.try_recv() {
                self.canvas_tree
                    .set_focused_node_and_surface((surface, node));
                self.canvas_tree.push_to_focused_leaf(id);
            }

            if let Some((_, &mut id)) = self.canvas_tree.find_active_focused() {
                self.active_canvas = id;
            }
            egui_dock::DockArea::new(&mut self.canvas_tree)
                .id(Id::new("view.dock"))
                .style({
                    let mut style = egui_dock::Style::from_egui(ui.style());
                    style.tab.tab_body.inner_margin = Margin::ZERO;
                    style.tab_bar.height = 50.0;
                    style.tab_bar.hline_color = Color32::TRANSPARENT;
                    style.tab_bar.inner_margin = Margin::same(10);

                    style.tab.spacing = 10.0;

                    style.tab.active.corner_radius = CornerRadius::same(10);
                    style.tab.active.outline_color = Color32::TRANSPARENT;

                    style.tab.inactive.corner_radius = CornerRadius::same(10);
                    style.tab.inactive.outline_color = Color32::TRANSPARENT;

                    style.tab.focused.corner_radius = CornerRadius::same(10);
                    style.tab.focused.outline_color = Color32::TRANSPARENT;
                    style.tab.focused.bg_fill = super::widgets::ACCENT_COLOR;

                    style.tab.hovered.corner_radius = CornerRadius::same(10);
                    style.tab.hovered.outline_color = Color32::TRANSPARENT;

                    style.buttons.close_tab_color = Color32::WHITE;
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
                        canvases: &mut self.canvases,
                        previews: &mut self.previews,
                        instances: &mut instances,
                    },
                );
        }
    }

    pub fn layout_gui(&mut self, context: &Context) {
        CentralPanel::default()
            .frame(Frame::NONE)
            .show(context, |ui| {
                self.layout_view(ui);
            });
    }
}
