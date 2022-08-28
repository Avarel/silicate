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
    pub file: RwLock<ProcreateFile>,
    pub compositor: RwLock<Compositor<'dev>>,
    pub tex: RwLock<GpuTexture>,
    pub active: AtomicBool,
    pub force_recomposit: AtomicBool,
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
}

pub struct EditorState<'dev> {
    pub dev: &'dev LogicalDevice,
    pub egui_tex: TextureId,
    pub smooth: bool,
    pub show_grid: bool,
    pub cs: Arc<CompositorState<'dev>>,
}

impl<'dev> EditorState<'dev> {
    fn layout_file(&self, ui: &mut Ui) {
        let cs = &self.cs;
        Frame::group(&Style::default()).show(ui, |ui| {
            ui.collapsing("File", |ui| {
                ui.separator();

                Grid::new("File Grid")
                    .num_columns(2)
                    .spacing([8.0, 10.0])
                    .striped(true)
                    .show(ui, |ui| {
                        let file = cs.file.read();
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
                        ui.allocate_space(vec2(ui.available_width(), 0.0))
                    });

                if ui.button("Export View").clicked() {
                    cs.tex.read().export(self.dev, cs.compositor.read().dim);
                }
                ui.allocate_space(vec2(ui.available_width(), 0.0))
            });
            ui.allocate_space(vec2(ui.available_width(), 0.0))
        });
    }

    fn layout_view_control(&mut self, ui: &mut Ui) {
        let cs = &self.cs;
        Frame::group(&Style::default()).show(ui, |ui| {
            ui.collapsing("View Control", |ui| {
                ui.separator();
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
                                compositor.flip_vertices((true, false));
                                cs.set_recomposit(true);
                            }
                            if ui.button("Vertical").clicked() {
                                compositor.flip_vertices((false, true));
                                cs.set_recomposit(true);
                            }
                        });
                        ui.end_row();
                        ui.label("Rotate");
                        ui.horizontal(|ui| {
                            if ui.button("CCW").clicked() {
                                compositor.rotate_vertices(true);
                                compositor
                                    .set_dimensions(compositor.dim.height, compositor.dim.width);
                                cs.set_recomposit(true);
                            }
                            if ui.button("CW").clicked() {
                                compositor.rotate_vertices(false);
                                compositor
                                    .set_dimensions(compositor.dim.height, compositor.dim.width);
                                cs.set_recomposit(true);
                            }
                        });
                        ui.allocate_space(vec2(ui.available_width(), 0.0))
                    });
                // ui.allocate_space(vec2(ui.available_width(), 0.0))
            });
            ui.allocate_space(vec2(ui.available_width(), 0.0))
        });
    }

    fn layout_layers_sub(ui: &mut Ui, layers: &mut SilicaGroup, i: &mut usize) {
        let mut first = true;
        for layer in &mut layers.children {
            *i += 1;
            match layer {
                SilicaHierarchy::Layer(l) => {
                    ui.push_id(*i, |ui| {
                        *i += 1;
                        ui.collapsing(l.name.as_deref().unwrap_or(""), |ui| {
                            ui.checkbox(&mut l.hidden, "Hidden");
                            if !first {
                                ui.checkbox(&mut l.clipped, "Clipped");
                            }
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
            first = false;
        }
    }

    fn layout_layers(&self, ui: &mut Ui) {
        let cs = &self.cs;

        let mut i = 0;
        Frame::group(&Style::default()).show(ui, |ui| {
            ui.label("Layers");
            ui.separator();
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    Self::layout_layers_sub(ui, &mut cs.file.write().layers, &mut i);
                });
            ui.allocate_space(vec2(ui.available_width(), 0.0))
        });
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

    pub fn layout_gui(&mut self, context: &Context) {
        SidePanel::new(panel::Side::Right, "Side Panel")
            .default_width(300.0)
            .show(&context, |ui| {
                self.layout_file(ui);
                self.layout_view_control(ui);
                self.layout_layers(ui);
            });

        CentralPanel::default()
            .frame(Frame::none())
            .show(&context, |ui| {
                self.layout_view(ui);
            });
    }
}
