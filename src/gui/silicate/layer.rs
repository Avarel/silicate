use egui::*;
use silica::layers::SilicaLayer;

use crate::{
    addendum::SilicaLayerAddendum,
    gui::custom::{blend_radio::BlendModeRadio, opacity_slider::OpacitySlider},
};

pub(super) struct LayerControl<'a> {
    pub layer: &'a mut SilicaLayer,
    pub addendum: &'a SilicaLayerAddendum,
}

impl LayerControl<'_> {
    pub fn ui(self, ui: &mut Ui, changed: &mut bool) {
        ui.push_id(self.addendum.id, |ui| {
            *changed |= OpacitySlider::new(&mut self.layer.opacity).ui(ui).changed();
            ui.add_space(10.0);
            *changed |= BlendModeRadio::new(&mut self.layer.blend).ui(ui).changed();
        });

        Grid::new(self.addendum.id).show(ui, |ui| {
            ui.label("Clipped");
            *changed |= Checkbox::without_text(&mut self.layer.clipped)
                .ui(ui)
                .changed();
        });
        ui.add_space(10.0);
    }
}
