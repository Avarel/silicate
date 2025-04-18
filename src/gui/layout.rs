use egui::load::SizedTexture;
use egui::*;
use egui_dock::{NodeIndex, SurfaceIndex};
use silica::layers::{SilicaGroup, SilicaHierarchy, SilicaLayer};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;

use crate::app::{App, Instance, InstanceKey, UserEvent};

use super::{
    canvas::CanvasView,
    custom::{blend_radio::BlendModeRadio, opacity_slider::OpacitySlider},
};

struct ControlsGui<'a> {
    app: &'a Arc<App>,
    active_canvas: InstanceKey,
    view_options: &'a mut ViewOptions,
}

impl ControlsGui<'_> {
    fn layout_info(&self, ui: &mut Ui) {
        Grid::new("File Grid").show(ui, |ui| {
            if let Some(instance @ Instance { file, .. }) = self
                .app
                .compositor
                .instances
                .read()
                .get(&self.active_canvas)
            {
                let file = file.read();
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
                ui.label(file.layer_count().to_string());
                ui.end_row();
                ui.label("Canvas Size");

                let mut dim1 = file.size.width;
                let mut dim2 = file.size.height;

                if !instance.is_upright() {
                    std::mem::swap(&mut dim1, &mut dim2);
                }
                ui.label(format!("{} by {}", dim1, dim2));
            } else {
                ui.label("No file loaded...");
            }
        });
    }

    fn layout_view_control(&mut self, ui: &mut Ui) {
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
                self.app.rebind_texture(self.active_canvas);
            }
            ui.end_row();
            ui.label("Rotation");
            {
                if let Some(instance) = self
                    .app
                    .compositor
                    .instances
                    .write()
                    .get_mut(&self.active_canvas)
                {
                    ui.add(
                        Slider::new(&mut instance.rotation, 0.0..=std::f32::consts::TAU)
                            .custom_formatter(|v, _| {
                                let degree = v.to_degrees();
                                format!("{degree:.0}")
                            })
                            .suffix(" deg"),
                    );
                } else {
                    ui.label("No file loaded...");
                }
            }
        });
    }

    fn layout_canvas_control(&mut self, ui: &mut Ui) {
        if let Some(instance) = self
            .app
            .compositor
            .instances
            .write()
            .get_mut(&self.active_canvas)
        {
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
                        instance.target.lock().set_flipped(
                            instance.flipped.horizontally,
                            instance.flipped.vertically,
                        );
                    }
                });
                ui.end_row();
            });

            ui.separator();
            Grid::new("File Grid").num_columns(2).show(ui, |ui| {
                ui.label("Actions");
                ui.vertical(|ui| {
                    if ui.button("Export View").clicked() {
                        let target = instance.target.lock();
                        let texture = target.output();
                        let copied_texture = texture.clone(&self.app.dispatch);
                        self.app.rt.spawn({
                            let app = self.app.clone();
                            async move { app.save_dialog(copied_texture).await }
                        });
                    }
                });
            });
        } else {
            ui.label("No canvas loaded.");
        }
    }

    fn layout_layer_control(ui: &mut Ui, l: &mut SilicaLayer, changed: &mut bool) {
        *changed |= OpacitySlider::new(&mut l.opacity).ui(ui).changed();
        ui.add_space(10.0);
        *changed |= BlendModeRadio::new(&mut l.blend).ui(ui).changed();

        Grid::new(l.id).show(ui, |ui| {
            ui.label("Clipped");
            *changed |= Checkbox::without_text(&mut l.clipped).ui(ui).changed();
        });
        ui.add_space(10.0);
    }

    fn layout_layers_sub(ui: &mut Ui, layers: &mut SilicaGroup, changed: &mut bool) {
        layers.children.iter_mut().for_each(|layer| {
            let (id, layer_name, hidden) = match layer {
                SilicaHierarchy::Layer(layer) => {
                    let layer_name = layer
                        .name
                        .to_owned()
                        .unwrap_or_else(|| format!("Unnamed Layer"));

                    let id = ui.make_persistent_id(layer.id);
                    (id, layer_name, &mut layer.hidden)
                }
                SilicaHierarchy::Group(layer) => {
                    let layer_name = layer
                        .name
                        .to_owned()
                        .unwrap_or_else(|| format!("Unnamed Group"));

                    let id = ui.make_persistent_id(layer.id);
                    (id, layer_name, &mut layer.hidden)
                }
            };

            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                false,
            );

            let header_res = ui.horizontal(|ui| {
                let mut frame = egui::Frame::new()
                    .corner_radius(3)
                    .inner_margin(5)
                    .begin(ui);
                {
                    let ui = &mut frame.content_ui;
                    if ui
                        .add(
                            Label::new(layer_name)
                                .selectable(false)
                                .sense(Sense::click()),
                        )
                        .clicked()
                    {
                        state.toggle(ui);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut shown = !*hidden;
                        *changed |= Checkbox::without_text(&mut shown).ui(ui).changed();
                        *hidden = !shown;
                        state.show_toggle_button(ui, egui::collapsing_header::paint_default_icon);
                    });
                }
                let response = frame.allocate_space(ui);
                if response.hovered() {
                    frame.frame.fill = Color32::from_rgb(50, 50, 50)
                } else {
                    frame.frame.fill = Color32::from_rgb(25, 25, 25)
                }
                frame.end(ui);
            });
            match layer {
                SilicaHierarchy::Layer(layer) => {
                    state.show_body_unindented(ui, |ui| {
                        Self::layout_layer_control(ui, layer, changed);
                    });
                }
                SilicaHierarchy::Group(layer) => {
                    state.show_body_indented(&header_res.response, ui, |ui| {
                        Self::layout_layers_sub(ui, layer, changed);
                    });
                }
            };
        });
    }

    fn layout_layers(&self, ui: &mut Ui) {
        if let Some(instance) = self
            .app
            .compositor
            .instances
            .read()
            .get(&self.active_canvas)
        {
            let mut file = instance.file.write();
            let mut changed = false;

            Self::layout_layers_sub(ui, &mut file.layers, &mut changed);

            ui.separator();

            // Let background controls be first since color controls are bad.
            Grid::new("layers.background").show(ui, |ui| {
                ui.label("Background");
                changed |= ui.checkbox(&mut file.background_hidden, "Hidden").changed();
                ui.end_row();
                ui.label("Background Color");

                // Safety: this is trivially safe, N=3 < 4
                let bg = unsafe {
                    file.background_color
                        .first_chunk_mut::<3>()
                        .unwrap_unchecked()
                };
                changed |= ui.color_edit_button_rgb(bg).changed();
            });

            // let bg = &mut file.background_color;
            // let rgb = Rgba::from_rgb(bg[0], bg[1], bg[2]);
            // let mut color = Color32::from(rgb);
            // let old_value = color;
            // color_picker::color_picker_color32(ui, &mut color, color_picker::Alpha::Opaque);
            // if old_value != color {
            //     *bg = Rgba::from(color).to_rgba_unmultiplied();
            //     changed = true;
            // }

            instance.tick_change(changed);
        } else {
            ui.label("No file hierachy.");
        }
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
    instances: &'a mut HashMap<InstanceKey, Instance>,
    view_options: &'a ViewOptions,
}

impl egui_dock::TabViewer for CanvasGui<'_> {
    type Tab = InstanceKey;

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        let tex = self.canvases.get(tab);

        let mut rotation = self.instances.get(tab).map(|v| v.rotation).unwrap_or(0.0);

        CanvasView::new(*tab, tex.copied().map(Image::from_texture), &mut rotation)
            .show_extended_crosshair(self.view_options.extended_crosshair)
            .show_grid(self.view_options.grid)
            .show(ui);

        self.instances.get_mut(tab).map(|v| {
            v.rotation = rotation.rem_euclid(std::f32::consts::TAU);
        });
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
}

pub struct ViewerGui {
    pub app: Arc<App>,

    pub canvases: HashMap<InstanceKey, SizedTexture>,
    pub active_canvas: InstanceKey,
    pub view_options: ViewOptions,
    pub canvas_tree: egui_dock::DockState<InstanceKey>,
    pub viewer_tree: egui_dock::DockState<ViewerTab>,
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
                .style(egui_dock::Style::from_egui(ui.style()))
                .show_add_buttons(true)
                .show_leaf_close_all_buttons(false)
                .show_leaf_collapse_buttons(false)
                .show_inside(
                    ui,
                    &mut CanvasGui {
                        app: &self.app,
                        view_options: &self.view_options,
                        canvases: &mut self.canvases,
                        instances: &mut instances,
                    },
                );
        }
    }

    pub fn layout_gui(&mut self, context: &Context) {
        SidePanel::new(panel::Side::Right, "Side Panel")
            .default_width(300.0)
            .frame(Frame::NONE)
            .show(context, |ui| {
                egui_dock::DockArea::new(&mut self.viewer_tree)
                    .style(egui_dock::Style::from_egui(ui.style()))
                    .show_close_buttons(false)
                    .show_leaf_close_all_buttons(false)
                    .show_inside(
                        ui,
                        &mut ControlsGui {
                            app: &self.app,
                            active_canvas: self.active_canvas,
                            view_options: &mut self.view_options,
                        },
                    );
            });

        CentralPanel::default()
            .frame(Frame::NONE)
            .show(context, |ui| {
                self.layout_view(ui);
            });
    }
}

#[derive(Clone, Copy)]
pub enum ViewerTab {
    Information,
    ViewControls,
    CanvasControls,
    Hierarchy,
}

impl egui_dock::TabViewer for ControlsGui<'_> {
    type Tab = ViewerTab;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        Frame::NONE
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| match *tab {
                ViewerTab::Information => self.layout_info(ui),
                ViewerTab::ViewControls => self.layout_view_control(ui),
                ViewerTab::CanvasControls => self.layout_canvas_control(ui),
                ViewerTab::Hierarchy => self.layout_layers(ui),
            });
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match *tab {
            ViewerTab::Information => "Info",
            ViewerTab::ViewControls => "View",
            ViewerTab::CanvasControls => "Canvas",
            ViewerTab::Hierarchy => "Hierarchy",
        }
        .into()
    }
}
