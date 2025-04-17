use egui::load::SizedTexture;
use egui::*;
use egui_dock::{NodeIndex, SurfaceIndex};
use silica::layers::{SilicaGroup, SilicaHierarchy, SilicaLayer};
use silicate_compositor::blend::BlendingMode;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;

use super::app::{App, Instance, InstanceKey, UserEvent};
use super::canvas;

struct ControlsGui<'a> {
    app: &'a Arc<App>,
    active_canvas: InstanceKey,
    view_options: &'a mut ViewOptions,
}

impl ControlsGui<'_> {
    fn layout_info(&self, ui: &mut Ui) {
        Grid::new("File Grid").show(ui, |ui| {
            if let Some(Instance { file, .. }) = self
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
                ui.label("Canvas Size");
                ui.label(format!("{} by {}", file.size.width, file.size.height));
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
            ui.label("Bottom Bar");
            ui.checkbox(&mut self.view_options.bottom_bar, "Enable");
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
                    let mut degree = instance.rotation.to_degrees();
                    ui.add(Slider::new(&mut degree, 0.0..=360.0).suffix(" deg"));
                    instance.rotation = degree.to_radians();
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

    fn layout_layer_control(ui: &mut Ui, i: usize, l: &mut SilicaLayer, changed: &mut bool) {
        ui.horizontal_wrapped(|ui| {
            *changed |= ui.checkbox(&mut l.hidden, "Hidden").changed();
            *changed |= ui.checkbox(&mut l.clipped, "Clipped").changed();
        });
        Grid::new(i).show(ui, |ui| {
            ui.label("Blend");
            ComboBox::from_id_salt(0)
                .selected_text(l.blend.as_str())
                .show_ui(ui, |ui| {
                    for b in BlendingMode::all() {
                        *changed |= ui.selectable_value(&mut l.blend, *b, b.as_str()).changed();
                    }
                });
            ui.end_row();

            let mut percent = l.opacity * 100.0;
            ui.label("Opacity");
            *changed |= ui
                .add(
                    Slider::new(&mut percent, 0.0..=100.0)
                        .fixed_decimals(0)
                        .suffix("%"),
                )
                .changed();
            l.opacity = percent / 100.0;
        });
    }

    fn layout_layers_sub(ui: &mut Ui, layers: &mut SilicaGroup, i: &mut usize, changed: &mut bool) {
        for layer in &mut layers.children {
            *i += 1;
            match layer {
                SilicaHierarchy::Layer(l) => {
                    ui.push_id(*i, |ui| {
                        *i += 1;

                        ui.collapsing(
                            l.name
                                .to_owned()
                                .unwrap_or_else(|| format!("Unnamed Layer [{i}]")),
                            |ui| {
                                Self::layout_layer_control(ui, *i, l, changed);
                            },
                        );
                    });
                }
                SilicaHierarchy::Group(h) => {
                    ui.push_id(*i, |ui| {
                        *i += 1;
                        ui.collapsing(
                            h.name
                                .to_owned()
                                .unwrap_or_else(|| format!("Unnamed Group [{i}]")),
                            |ui| {
                                *changed |= ui.checkbox(&mut h.hidden, "Hidden").changed();
                                Self::layout_layers_sub(ui, h, i, changed);
                            },
                        )
                    });
                }
            }
        }
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

            let mut i = 1000;
            Self::layout_layers_sub(ui, &mut file.layers, &mut i, &mut changed);

            ui.separator();

            // Let background controls be first since color controls are bad.
            Grid::new("layers.background").show(ui, |ui| {
                ui.label("Background");
                changed |= ui.checkbox(&mut file.background_hidden, "Hidden").changed();
                ui.end_row();
                ui.label("Background Color");

                // Safety: This is trivially safe. The underlying container is of 4 elements.
                // This does the same thing as split_array_mut except that is not stabilized yet.
                let bg = unsafe { &mut *(file.background_color.as_mut_ptr() as *mut [f32; 3]) };
                changed |= ui.color_edit_button_rgb(bg).changed();
            });

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
    pub bottom_bar: bool,
}

struct CanvasGui<'a> {
    app: &'a Arc<App>,
    canvases: &'a mut HashMap<InstanceKey, SizedTexture>,
    instances: &'a HashMap<InstanceKey, Instance>,
    view_options: &'a ViewOptions,
}

impl egui_dock::TabViewer for CanvasGui<'_> {
    type Tab = InstanceKey;

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        let tex = self.canvases.get(tab);

        let rotation = self.instances.get(tab).map(|v| v.rotation).unwrap_or(0.0);

        canvas::CanvasView::new(*tab, tex.copied().map(Image::from_texture))
            .with_rotation(rotation)
            .show_extended_crosshair(self.view_options.extended_crosshair)
            .show_grid(self.view_options.grid)
            .show_bottom_bar(self.view_options.bottom_bar)
            .show(ui);
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

        let mut instances = self.app.compositor.instances.read();

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
