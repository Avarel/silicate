use egui::*;
use silica_gpu::layer::SilicaLayerGpu;

use crate::gui::widgets::{blend_radio::BlendModeRadio, opacity_slider::OpacitySlider};

pub(super) struct LayerControl<'a> {
    pub layer: &'a mut SilicaLayerGpu,
}

impl LayerControl<'_> {
    pub fn ui(self, ui: &mut Ui) {
        ui.push_id(self.layer.addendum.id, |ui| {
            OpacitySlider::new(&mut self.layer.info.opacity).ui(ui);
            ui.add_space(10.0);
            BlendModeRadio::new(&mut self.layer.info.blend).ui(ui);
        });

        Grid::new(self.layer.addendum.id).show(ui, |ui| {
            ui.label("Clipped");
            Checkbox::without_text(&mut self.layer.info.clipped).ui(ui);
        });
        ui.add_space(10.0);
    }
}
