use crate::compositor::BufferDimensions;
use crate::silica::{BlendingMode, SilicaHierarchy};
use crate::silica::{SilicaGroup, SilicaLayer};
use egui::load::SizedTexture;
use egui::*;
use egui_dock::{NodeIndex, SurfaceIndex};
use std::collections::HashMap;
use std::sync::Arc;

use super::app::{App, Instance, InstanceKey};
use super::canvas;

struct ControlsGui<'a> {
    app: &'a Arc<App>,
    rt: &'a tokio::runtime::Runtime,

    selected_canvas: &'a InstanceKey,
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
                .get(self.selected_canvas)
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
                self.app
                    .eloop
                    .send_event(super::UserEvent::RebindTexture(*self.selected_canvas))
                    .unwrap();
            }
            ui.end_row();
            ui.label("Rotation");
            {
                let mut degree = self.view_options.rotation.to_degrees();
                ui.add(Slider::new(&mut degree, 0.0..=360.0).suffix(" deg"));
                self.view_options.rotation = degree.to_radians();
            }
        });
    }

    fn layout_canvas_control(&mut self, ui: &mut Ui) {
        if let Some(instance) = self
            .app
            .compositor
            .instances
            .read()
            .get(self.selected_canvas)
        {
            Grid::new("Canvas Grid").show(ui, |ui| {
                ui.label("Flip");
                ui.horizontal(|ui| {
                    if ui.button("Horizontal").clicked() {
                        instance.target.lock().data.flip_vertices(false, true);
                        instance.store_change_or(true);
                        self.app
                            .eloop
                            .send_event(super::UserEvent::RebindTexture(*self.selected_canvas))
                            .unwrap();
                    }
                    if ui.button("Vertical").clicked() {
                        instance.target.lock().data.flip_vertices(true, false);
                        instance.store_change_or(true);
                        self.app
                            .eloop
                            .send_event(super::UserEvent::RebindTexture(*self.selected_canvas))
                            .unwrap();
                    }
                });
                ui.end_row();
                ui.label("Rotate");
                ui.horizontal(|ui| {
                    if ui.button("CCW").clicked() {
                        let mut target = instance.target.lock();
                        target.data.rotate_vertices(true);
                        if target.transpose_dimensions() {
                            self.app
                                .eloop
                                .send_event(super::UserEvent::RebindTexture(*self.selected_canvas))
                                .unwrap();
                        }
                        instance.store_change_or(true);
                    }
                    if ui.button("CW").clicked() {
                        let mut target = instance.target.lock();
                        target.data.rotate_vertices(false);
                        if target.transpose_dimensions() {
                            self.app
                                .eloop
                                .send_event(super::UserEvent::RebindTexture(*self.selected_canvas))
                                .unwrap();
                        }
                        instance.store_change_or(true);
                    }
                });
            });
            let instances = self.app.compositor.instances.read();
            if let Some(instance) = instances.get(self.selected_canvas) {
                ui.separator();
                Grid::new("File Grid").num_columns(2).show(ui, |ui| {
                    ui.label("Actions");
                    ui.vertical(|ui| {
                        if ui.button("Export View").clicked() {
                            if let Some(texture) = instance.target.lock().output.as_ref() {
                                let copied_texture = texture.texture.clone(&self.app.dev);
                                self.rt.spawn(self.app.clone().save_dialog(copied_texture));
                            }
                        }
                    });
                });
            }
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
            *changed |= ComboBox::from_id_source(0)
                .selected_text(l.blend.as_str())
                .show_ui(ui, |ui| {
                    for b in BlendingMode::all() {
                        ui.selectable_value(&mut l.blend, *b, b.as_str());
                    }
                })
                .response
                .changed();
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
            .get(self.selected_canvas)
        {
            let mut file = instance.file.write();
            let mut changed = false;

            let mut i = 0;
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

            instance.store_change_or(changed);
        } else {
            ui.label("No file hierachy.");
        }
    }
}

pub struct ViewOptions {
    pub extended_crosshair: bool,
    pub smooth: bool,
    pub grid: bool,
    pub rotation: f32,
    pub bottom_bar: bool,
}

struct CanvasGui<'a> {
    app: &'a Arc<App>,
    rt: &'a tokio::runtime::Runtime,

    canvases: &'a mut HashMap<InstanceKey, (TextureId, BufferDimensions)>,
    instances: &'a HashMap<InstanceKey, Instance>,
    view_options: &'a ViewOptions,
}

impl egui_dock::TabViewer for CanvasGui<'_> {
    type Tab = InstanceKey;

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        let tex = self.canvases.get(tab);
        canvas::CanvasView::new(
            *tab,
            tex.map(|(tex, size)| {
                Image::from_texture(SizedTexture::new(
                    *tex,
                    (size.width as f32, size.height as f32),
                ))
            }),
        )
        .with_rotation(self.view_options.rotation)
        .show_extended_crosshair(self.view_options.extended_crosshair)
        .show_grid(self.view_options.grid)
        .show_bottom_bar(self.view_options.bottom_bar)
        .show(ui);
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> bool {
        self.app
            .eloop
            .send_event(super::UserEvent::RemoveInstance(*tab))
            .unwrap();
        true
    }

    fn on_add(&mut self, surface: egui_dock::SurfaceIndex, node: egui_dock::NodeIndex) {
        self.rt.spawn(self.app.clone().load_dialog(surface, node));
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
    pub rt: Arc<tokio::runtime::Runtime>,

    pub canvases: HashMap<InstanceKey, (TextureId, BufferDimensions)>,
    pub selected_canvas: InstanceKey,
    pub view_options: ViewOptions,
    pub canvas_tree: egui_dock::DockState<InstanceKey>,
    pub viewer_tree: egui_dock::DockState<ViewerTab>,
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
                    self.rt.spawn(
                        self.app
                            .clone()
                            .load_dialog(SurfaceIndex::main(), NodeIndex::root()),
                    );
                }
            });
        } else {
            if let Some(mut added_instances) = self.app.added_instances.try_lock() {
                for (surface, node, id) in added_instances.drain(..) {
                    self.canvas_tree
                        .set_focused_node_and_surface((surface, node));
                    self.canvas_tree.push_to_focused_leaf(id);
                }
            }

            if let Some((_, id)) = self.canvas_tree.find_active_focused() {
                self.selected_canvas = *id;
            }
            egui_dock::DockArea::new(&mut self.canvas_tree)
                .id(Id::new("view.dock"))
                .style(egui_dock::Style::from_egui(ui.style()))
                .show_add_buttons(true)
                .show_inside(
                    ui,
                    &mut CanvasGui {
                        app: &self.app,
                        rt: &self.rt,

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
            .frame(Frame::none())
            .show(context, |ui| {
                egui_dock::DockArea::new(&mut self.viewer_tree)
                    .style(egui_dock::Style::from_egui(ui.style()))
                    .show_close_buttons(false)
                    .show_inside(
                        ui,
                        &mut ControlsGui {
                            app: &self.app,
                            rt: &self.rt,
                            selected_canvas: &self.selected_canvas,
                            view_options: &mut self.view_options,
                        },
                    );
            });

        CentralPanel::default()
            .frame(Frame::none())
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
        Frame::none()
            .inner_margin(style::Margin::same(10.0))
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
