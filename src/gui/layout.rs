use crate::compositor::{dev::LogicalDevice, tex::GpuTexture};
use crate::silica::{ProcreateFile, SilicaGroup};
use crate::{
    compositor::Compositor,
    silica::{BlendingMode, SilicaHierarchy},
};
use egui::*;
use parking_lot::RwLock;
use std::sync::{atomic::AtomicBool, Arc};

pub struct CompositorState<'dev> {
    pub file: RwLock<Option<ProcreateFile>>,
    pub gpu_textures: RwLock<Option<Vec<GpuTexture>>>,
    pub compositor: RwLock<Compositor<'dev>>,
    pub tex: RwLock<GpuTexture>,
    pub active: AtomicBool,
    pub force_recomposit: AtomicBool,
    pub changed: AtomicBool,
}

impl<'dev> CompositorState<'dev> {
    const ORDERING: std::sync::atomic::Ordering = std::sync::atomic::Ordering::SeqCst;

    pub fn is_active(&self) -> bool {
        self.active.load(Self::ORDERING)
    }

    pub fn deactivate(&self) {
        self.force_recomposit.store(false, Self::ORDERING);
    }

    pub fn get_recomposit(&self) -> bool {
        self.force_recomposit.load(Self::ORDERING)
    }

    pub fn set_recomposit(&self, b: bool) {
        self.force_recomposit.store(b, Self::ORDERING);
    }

    pub fn get_changed(&self) -> bool {
        self.changed.load(Self::ORDERING)
    }

    pub fn set_changed(&self, b: bool) {
        self.changed.store(b, Self::ORDERING);
    }
}

pub struct ViewerState<'dev> {
    pub dev: &'dev LogicalDevice,
    pub egui_tex: TextureId,
    pub smooth: bool,
    pub show_grid: bool,
    pub cs: Arc<CompositorState<'dev>>,
}

impl<'dev> ViewerState<'dev> {
    fn layout_file(&self, ui: &mut Ui) {
        let cs = &self.cs;
        Grid::new("File Grid")
            .num_columns(2)
            .spacing([8.0, 10.0])
            .striped(true)
            .show(ui, |ui| {
                if let Some(file) = cs.file.read().as_ref() {
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
                ui.allocate_space(vec2(ui.available_width(), 0.0));
            });

        if ui.button("Export View").clicked() {
            cs.tex.read().export(self.dev, cs.compositor.read().dim);
        }
        ui.allocate_space(vec2(ui.available_width(), 0.0));
    }

    fn layout_view_control(&mut self, ui: &mut Ui) {
        let cs = &self.cs;
        if ui.button("Toggle Grid").clicked() {
            self.show_grid = !self.show_grid;
        }
        if ui.checkbox(&mut self.smooth, "Smooth").changed() {
            cs.set_recomposit(true);
        }
        ui.separator();

        Grid::new("Control Grid")
            .num_columns(2)
            .spacing([8.0, 10.0])
            .striped(true)
            .show(ui, |ui| {
                let compositor = &mut *cs.compositor.write();

                ui.label("Flip");
                ui.horizontal(|ui| {
                    if ui.button("Horizontal").clicked() {
                        compositor.flip_vertices((false, true));
                        cs.set_recomposit(true);
                        cs.set_changed(true);
                    }
                    if ui.button("Vertical").clicked() {
                        compositor.flip_vertices((true, false));
                        cs.set_recomposit(true);
                        cs.set_changed(true);
                    }
                });
                ui.end_row();
                ui.label("Rotate");
                ui.horizontal(|ui| {
                    if ui.button("CCW").clicked() {
                        compositor.rotate_vertices(true);
                        compositor.set_dimensions(compositor.dim.height, compositor.dim.width);
                        cs.set_recomposit(true);
                        cs.set_changed(true);
                    }
                    if ui.button("CW").clicked() {
                        compositor.rotate_vertices(false);
                        compositor.set_dimensions(compositor.dim.height, compositor.dim.width);
                        cs.set_recomposit(true);
                        cs.set_changed(true);
                    }
                });
                ui.allocate_space(vec2(ui.available_width(), 0.0))
            });
    }

    fn layout_layers_sub(ui: &mut Ui, layers: &mut SilicaGroup, i: &mut usize) {
        for layer in &mut layers.children {
            *i += 1;
            match layer {
                SilicaHierarchy::Layer(l) => {
                    ui.push_id(*i, |ui| {
                        *i += 1;
                        ui.collapsing(l.name.as_deref().unwrap_or(""), |ui| {
                            ui.checkbox(&mut l.hidden, "Hidden");
                            // TODO: last child layer cannot have clipped
                            ui.checkbox(&mut l.clipped, "Clipped");

                            ComboBox::from_label("Blending Mode")
                                .selected_text(l.blend.to_str())
                                .show_ui(ui, |ui| {
                                    for b in BlendingMode::all() {
                                        ui.selectable_value(&mut l.blend, *b, b.to_str());
                                    }
                                });
                            ui.add(Slider::new(&mut l.opacity, 0.0..=1.0).text("Opacity"));
                        });
                    });
                }
                SilicaHierarchy::Group(h) => {
                    ui.push_id(*i, |ui| {
                        *i += 1;
                        ui.collapsing(h.name.to_string().as_str(), |ui| {
                            ui.checkbox(&mut h.hidden, "Hidden");
                            Self::layout_layers_sub(ui, h, i);
                        })
                    });
                }
            }
        }
    }

    fn layout_layers(&self, ui: &mut Ui) {
        let cs = &self.cs;

        let mut i = 0;
        if let Some(file) = cs.file.write().as_mut() {
            Self::layout_layers_sub(ui, &mut file.layers, &mut i);
        } else {
            ui.label("No file loaded...");
        }
    }

    fn layout_view(&mut self, ui: &mut Ui) {
        let cs = &self.cs;
        let mut plot = plot::Plot::new("Image View").data_aspect(1.0);

        if self.show_grid {
            plot = plot.show_x(false).show_y(false).show_axes([false, false]);
        }

        plot.show(ui, |plot_ui| {
            let size = cs.compositor.read().dim;
            plot_ui.image(plot::PlotImage::new(
                self.egui_tex,
                plot::PlotPoint { x: 0.0, y: 0.0 },
                (size.width as f32, size.height as f32),
            ))
        });
    }
}

pub struct EditorState<'dev> {
    pub viewer: ViewerState<'dev>,
    pub tree: egui_dock::Tree<&'static str>,
}

impl<'dev> EditorState<'dev> {
    pub fn layout_gui(&mut self, context: &Context) {
        egui_dock::DockArea::new(&mut self.tree)
            .style(egui_dock::Style {
                show_close_buttons: false,
                ..egui_dock::Style::from_egui(context.style().as_ref())
            })
            .show(context, &mut self.viewer);
    }
}

impl<'dev> egui_dock::TabViewer for ViewerState<'dev> {
    type Tab = &'static str;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        // ui.label(format!("Content of {tab}"));
        match *tab {
            "Information" => self.layout_file(ui),
            "View Controls" => self.layout_view_control(ui),
            "Hierarchy" => self.layout_layers(ui),
            "Viewer" => self.layout_view(ui),
            _ => {}
        }
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        (*tab).into()
    }
}
