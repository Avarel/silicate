mod canvas;
mod silicate;
mod widgets;

use egui::{Frame, *};
use egui_dock::{tab_viewer::OnCloseResponse, NodeIndex, SurfaceIndex};
use egui_winit::winit::event_loop::EventLoopProxy;
use std::collections::HashMap;
use std::sync::Arc;

use crate::app::{
    instance::{Instance, InstanceKey},
    App, UserEvent,
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

    fn layout_export_control(
        ui: &mut Ui,
        event_loop: &EventLoopProxy<UserEvent>,
        instance: &Instance,
    ) {
        Grid::new("Share Grid").num_columns(2).show(ui, |ui| {
            ui.label("Actions");
            ui.vertical(|ui| {
                if ui.button("Export View").clicked() {
                    let texture = instance.output_texture.clone();
                    event_loop.send_event(UserEvent::SaveDialog(texture)).ok();
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
    event_loop: &'a EventLoopProxy<UserEvent>,
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

                ControlsGui::layout_export_control(ui, self.event_loop, instance);
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
        self.event_loop
            .send_event(UserEvent::RemoveInstance(*tab))
            .unwrap();
        OnCloseResponse::Close
    }

    fn on_add(&mut self, surface: egui_dock::SurfaceIndex, node: egui_dock::NodeIndex) {
        self.event_loop
            .send_event(UserEvent::LoadDialog(surface, node))
            .ok();
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
    pub event_loop: EventLoopProxy<UserEvent>,
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
                Label::new("Drag and drop Procreate documents to view them.")
                    .selectable(false)
                    .ui(ui);

                if ui.button("Load Procreate File").clicked() {
                    self.event_loop
                        .send_event(UserEvent::LoadDialog(
                            SurfaceIndex::main(),
                            NodeIndex::root(),
                        ))
                        .ok();
                }
            });
        } else {
            egui_dock::DockArea::new(&mut self.canvas_tree)
                .id(Id::new("view.dock"))
                .style({
                    let mut style = egui_dock::Style::from_egui(ui.style());
                    style.tab.tab_body.inner_margin = Margin::ZERO;
                    style.tab_bar.height = 50.0;
                    style.tab_bar.hline_color = Color32::TRANSPARENT;
                    style.tab_bar.inner_margin = Margin::same(10);

                    style.tab.spacing = 10.0;

                    style.tab_bar.bg_fill = Color32::TRANSPARENT;

                    style.tab.active.corner_radius = CornerRadius::same(10);
                    style.tab.active.outline_color = Color32::TRANSPARENT;

                    style.tab.inactive.corner_radius = CornerRadius::same(10);
                    style.tab.inactive.outline_color = Color32::TRANSPARENT;

                    style.tab.focused.corner_radius = CornerRadius::same(10);
                    style.tab.focused.outline_color = Color32::TRANSPARENT;
                    style.tab.focused.bg_fill = widgets::ACCENT_COLOR;

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
                        instances: &mut self.instances,
                        event_loop: &self.event_loop,
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
