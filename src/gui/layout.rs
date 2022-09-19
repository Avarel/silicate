use crate::compositor::{dev::LogicalDevice, tex::GpuTexture};
use crate::compositor::{BufferDimensions, CompositorTarget};
use crate::silica::{ProcreateFile, SilicaGroup, SilicaLayer};
use crate::{
    compositor::CompositorPipeline,
    silica::{BlendingMode, SilicaHierarchy},
};
use egui::*;
use parking_lot::{Mutex, RwLock};
use std::sync::atomic::AtomicBool;

use super::canvas;

pub struct Instance {
    pub file: ProcreateFile,
    pub textures: Vec<GpuTexture>,
    pub target: CompositorTarget<'static>,
    pub new_texture: AtomicBool,
    pub force_recomposit: AtomicBool,
}

impl Drop for Instance {
    fn drop(&mut self) {
        println!("Closing {:?}", self.file.name);
    }
}

pub struct CompositorHandle {
    pub instances: RwLock<Vec<Instance>>,
    pub pipeline: CompositorPipeline,
}

async fn load_dialog(
    dev: &'static LogicalDevice,
    compositor: &CompositorHandle,
    toasts: &'static Mutex<egui_notify::Toasts>,
) {
    if let Some(handle) = rfd::AsyncFileDialog::new()
        .add_filter("procreate", &["procreate"])
        .pick_file()
        .await
    {
        if super::load_file(handle.path().to_path_buf(), dev, compositor).await {
            toasts.lock().success("Load succeeded.");
        } else {
            toasts.lock().error("Load failed.");
        }
    } else {
        toasts.lock().info("Load cancelled.");
    }
}

async fn save_dialog(
    dev: &'static LogicalDevice,
    copied_texture: GpuTexture,
    toasts: &'static Mutex<egui_notify::Toasts>,
) {
    if let Some(handle) = rfd::AsyncFileDialog::new()
        .add_filter("png", image::ImageFormat::Png.extensions_str())
        .add_filter("jpeg", image::ImageFormat::Jpeg.extensions_str())
        .add_filter("tga", image::ImageFormat::Tga.extensions_str())
        .add_filter("tiff", image::ImageFormat::Tiff.extensions_str())
        .add_filter("webp", image::ImageFormat::WebP.extensions_str())
        .add_filter("bmp", image::ImageFormat::Bmp.extensions_str())
        .save_file()
        .await
    {
        let dim = BufferDimensions::from_extent(copied_texture.size);
        tokio::task::spawn_blocking(move || copied_texture.export(dev, dim, handle.path()))
            .await
            .unwrap();
        toasts.lock().success("Export succeeded.");
    } else {
        toasts.lock().info("Load cancelled.");
    }
}

struct ViewerDock<'a> {
    dev: &'static LogicalDevice,
    rt: &'static tokio::runtime::Runtime,
    selected_canvas: &'a mut usize,
    compositor: &'static CompositorHandle,
    view_options: &'a mut ViewOptions,
    queued_remove: &'a mut Option<usize>,
    toasts: &'static Mutex<egui_notify::Toasts>,
}

impl ViewerDock<'_> {
    fn layout_info(&self, ui: &mut Ui) {
        Grid::new("File Grid").show(ui, |ui| {
            if let Some(Instance { file, .. }) =
                self.compositor.instances.read().get(*self.selected_canvas)
            {
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

    fn layout_file_control(&mut self, ui: &mut Ui) {
        let mut instances = self.compositor.instances.write();
        Grid::new("File Grid").show(ui, |ui| {
            ui.label("File");
            ComboBox::from_id_source("File View")
                .selected_text(if instances.is_empty() {
                    "No file loaded..."
                } else {
                    instances[*self.selected_canvas]
                        .file
                        .name
                        .as_deref()
                        .unwrap_or("Untitled Artwork")
                })
                .show_ui(ui, |ui| {
                    for (idx, instance) in instances.iter().enumerate() {
                        ui.selectable_value(
                            self.selected_canvas,
                            idx,
                            instance.file.name.as_deref().unwrap_or("Untitled Artwork"),
                        );
                    }
                });

            ui.end_row();
            ui.label("Actions");
            ui.vertical(|ui| {
                if !instances.is_empty() && ui.button("Close Current File").clicked() {
                    *self.queued_remove = Some(*self.selected_canvas);
                }
                if ui.button("Open Other File").clicked() {
                    self.rt
                        .spawn(load_dialog(self.dev, self.compositor, self.toasts));
                }
                if let Some(instance) = instances.get_mut(*self.selected_canvas) {
                    if ui.button("Export View").clicked() {
                        let copied_texture = instance
                            .target
                            .output_texture
                            .as_ref()
                            .unwrap()
                            .clone(self.dev);

                        self.rt
                            .spawn(save_dialog(self.dev, copied_texture, self.toasts));
                    }
                }
            });
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
                let mut instances = self.compositor.instances.write();
                if let Some(instance) = instances.get_mut(*self.selected_canvas) {
                    *instance.new_texture.get_mut() = true;
                }
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
            .compositor
            .instances
            .write()
            .get_mut(*self.selected_canvas)
        {
            Grid::new("Canvas Grid").show(ui, |ui| {
                ui.label("Flip");
                ui.horizontal(|ui| {
                    if ui.button("Horizontal").clicked() {
                        instance.target.flip_vertices((false, true));
                        *instance.force_recomposit.get_mut() |= true;
                        *instance.new_texture.get_mut() = true;
                    }
                    if ui.button("Vertical").clicked() {
                        instance.target.flip_vertices((true, false));
                        *instance.force_recomposit.get_mut() |= true;
                        *instance.new_texture.get_mut() = true;
                    }
                });
                ui.end_row();
                ui.label("Rotate");
                ui.horizontal(|ui| {
                    if ui.button("CCW").clicked() {
                        instance.target.rotate_vertices(true);
                        *instance.new_texture.get_mut() = instance
                            .target
                            .set_dimensions(instance.target.dim.height, instance.target.dim.width);
                        *instance.force_recomposit.get_mut() |= true;
                    }
                    if ui.button("CW").clicked() {
                        instance.target.rotate_vertices(false);
                        *instance.new_texture.get_mut() = instance
                            .target
                            .set_dimensions(instance.target.dim.height, instance.target.dim.width);
                        *instance.force_recomposit.get_mut() |= true;
                    }
                });
                ui.end_row();
                ui.label("Background");
                *instance.force_recomposit.get_mut() |= ui
                    .checkbox(&mut instance.file.background_hidden, "Hidden")
                    .changed();
                ui.end_row();
                ui.label("Background Color");

                let bg = (&mut instance.file.background_color[0..=2])
                    .try_into()
                    .unwrap();
                *instance.force_recomposit.get_mut() |= ui.color_edit_button_rgb(bg).changed();
            });
        }
    }

    fn layout_layer_control(ui: &mut Ui, i: usize, l: &mut SilicaLayer) {
        Grid::new(i).show(ui, |ui| {
            ui.label("Hidden");
            ui.checkbox(&mut l.hidden, "");
            ui.end_row();
            ui.label("Clipped");
            ui.checkbox(&mut l.clipped, "");
            ui.end_row();

            ui.label("Blend");
            ComboBox::from_id_source(0)
                .selected_text(l.blend.to_str())
                .show_ui(ui, |ui| {
                    for b in BlendingMode::all() {
                        ui.selectable_value(&mut l.blend, *b, b.to_str());
                    }
                });
            ui.end_row();

            let mut percent = l.opacity * 100.0;
            ui.label("Opacity");
            ui.add(
                Slider::new(&mut percent, 0.0..=100.0)
                    .fixed_decimals(0)
                    .suffix("%"),
            );
            l.opacity = percent / 100.0;
        });
    }

    fn layout_layers_sub(ui: &mut Ui, layers: &mut SilicaGroup, i: &mut usize) {
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
                                Self::layout_layer_control(ui, *i, l);
                            },
                        );
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
        let mut i = 0;

        if let Some(Instance { file, .. }) = self
            .compositor
            .instances
            .write()
            .get_mut(*self.selected_canvas)
        {
            Self::layout_layers_sub(ui, &mut file.layers, &mut i);
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("No file hierachy.");
            });
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

pub struct EditorState {
    pub dev: &'static LogicalDevice,
    pub rt: &'static tokio::runtime::Runtime,

    pub compositor: &'static CompositorHandle,
    pub canvases: Vec<Option<TextureId>>,

    pub selected_canvas: usize,

    pub view_options: ViewOptions,

    pub queued_remove: Option<usize>,

    pub tree: egui_dock::Tree<ViewerTab>,
    pub toasts: &'static Mutex<egui_notify::Toasts>,
}

impl EditorState {
    pub fn remove_index(&mut self, index: usize) {
        self.canvases.remove(index);
        self.compositor.instances.write().remove(index);
    }

    fn layout_view(&mut self, ui: &mut Ui) {
        Frame::none()
            .fill(ui.visuals().window_fill())
            .stroke(ui.visuals().window_stroke())
            .inner_margin(style::Margin::same(1.0))
            .outer_margin(style::Margin::same(0.0))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());

                if let Some(instance) = self.compositor.instances.read().get(self.selected_canvas) {
                    if let Some(&Some(tex)) = self.canvases.get(self.selected_canvas) {
                        let size = instance.target.dim;
                        canvas::CanvasView::new(
                            &instance.file.name,
                            Some(Image::new(tex, (size.width as f32, size.height as f32))),
                        )
                        .with_rotation(self.view_options.rotation)
                        .show_extended_crosshair(self.view_options.extended_crosshair)
                        .show_grid(self.view_options.grid)
                        .show_bottom_bar(self.view_options.bottom_bar)
                        .show(ui);
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        if ui.button("Load a Procreate file to begin viewing it.").clicked() {
                            self.rt
                                .spawn(load_dialog(self.dev, self.compositor, self.toasts));
                        }
                    });
                }
            });
    }

    pub fn layout_gui(&mut self, context: &Context) {
        SidePanel::new(panel::Side::Right, "Side Panel")
            .default_width(300.0)
            .frame(Frame::none())
            .show(context, |ui| {
                egui_dock::DockArea::new(&mut self.tree)
                    .style(egui_dock::Style {
                        show_close_buttons: false,
                        ..egui_dock::Style::from_egui(ui.style().as_ref())
                    })
                    .show_inside(
                        ui,
                        &mut ViewerDock {
                            dev: &mut self.dev,
                            selected_canvas: &mut self.selected_canvas,
                            view_options: &mut &mut self.view_options,
                            compositor: &self.compositor,
                            rt: self.rt,
                            queued_remove: &mut self.queued_remove,
                            toasts: self.toasts,
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
    Files,
    Hierarchy,
}

impl egui_dock::TabViewer for ViewerDock<'_> {
    type Tab = ViewerTab;

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        Frame::none()
            .inner_margin(style::Margin::same(10.0))
            .show(ui, |ui| match *tab {
                ViewerTab::Information => self.layout_info(ui),
                ViewerTab::ViewControls => self.layout_view_control(ui),
                ViewerTab::CanvasControls => self.layout_canvas_control(ui),
                ViewerTab::Hierarchy => self.layout_layers(ui),
                ViewerTab::Files => self.layout_file_control(ui),
            });
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match *tab {
            ViewerTab::Information => "Info",
            ViewerTab::ViewControls => "View",
            ViewerTab::CanvasControls => "Canvas",
            ViewerTab::Hierarchy => "Hierarchy",
            ViewerTab::Files => "Files",
        }
        .into()
    }
}
