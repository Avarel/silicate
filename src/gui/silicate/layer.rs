use egui::*;
use silica::layers::SilicaLayer;

use crate::{
    addendum::SilicaLayerAddendum,
    gui::widgets::{blend_radio::BlendModeRadio, opacity_slider::OpacitySlider},
};

pub(super) struct LayerControl<'a> {
    pub layer: &'a mut SilicaLayer,
    pub addendum: &'a SilicaLayerAddendum,
}

impl LayerControl<'_> {
    pub fn ui(self, ui: &mut Ui) {
        ui.push_id(self.addendum.id, |ui| {
            OpacitySlider::new(&mut self.layer.info.opacity).ui(ui);
            ui.add_space(10.0);
            BlendModeRadio::new(&mut self.layer.info.blend).ui(ui);
        });

        Grid::new(self.addendum.id).show(ui, |ui| {
            ui.label("Clipped");
            Checkbox::without_text(&mut self.layer.info.clipped).ui(ui);
        });
        ui.add_space(10.0);
    }
}
