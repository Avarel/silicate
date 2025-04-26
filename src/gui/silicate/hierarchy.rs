use crate::{
    addendum::SilicaHierarchyAddendum, app::InstanceKey,
    gui::custom::layer_collapsible::LayerCollapsible,
};
use egui::{load::SizedTexture, *};
use silica::layers::SilicaHierarchy;
use std::collections::HashMap;

use super::layer::LayerControl;

pub struct LayersHierarchy<'a> {
    pub previews: &'a HashMap<(InstanceKey, u32), SizedTexture>,
    pub layers: &'a mut [SilicaHierarchy],
    pub addendum: &'a [SilicaHierarchyAddendum],
}

impl LayersHierarchy<'_> {
    pub fn ui(self, ui: &mut Ui, idx: InstanceKey, changed: &mut bool) {
        self.layers
            .iter_mut()
            .zip(self.addendum.iter())
            .for_each(|(mut layer, addendum)| {
                let (id, layer_name, hidden, size_change) = match (&mut layer, addendum) {
                    (SilicaHierarchy::Layer(layer), SilicaHierarchyAddendum::Layer(addendum)) => {
                        let layer_name = layer
                            .name
                            .to_owned()
                            .unwrap_or_else(|| format!("Unnamed Layer"));

                        (addendum.id, layer_name, &mut layer.hidden, false)
                    }
                    (SilicaHierarchy::Group(layer), SilicaHierarchyAddendum::Group(addendum)) => {
                        let layer_name = layer
                            .name
                            .to_owned()
                            .unwrap_or_else(|| format!("Unnamed Group"));

                        (addendum.id, layer_name, &mut layer.hidden, true)
                    }
                    _ => unreachable!(),
                };

                let collapsible = LayerCollapsible::new(id, layer_name, hidden)
                    .size_change(size_change)
                    .ui(ui, |ui| {
                        let image = self
                            .previews
                            .get(&(idx, id))
                            .map(|tex| Image::from_texture(*tex));
                        if let Some(image) = image {
                            image.paint_at(ui, ui.max_rect());
                        }
                    });

                *changed |= collapsible.response.changed();

                match (layer, addendum) {
                    (SilicaHierarchy::Layer(layer), SilicaHierarchyAddendum::Layer(addendum)) => {
                        collapsible.show_body_unindented(ui, |ui| -> _ {
                            LayerControl { layer, addendum }.ui(ui, changed)
                        });
                    }
                    (SilicaHierarchy::Group(layer), SilicaHierarchyAddendum::Group(addendum)) => {
                        collapsible.show_body_indented(ui, |ui| {
                            LayersHierarchy {
                                previews: self.previews,
                                layers: &mut layer.children,
                                addendum: &addendum.children,
                            }
                            .ui(ui, idx, changed);
                        });
                    }
                    _ => unreachable!(),
                };
            });
    }
}
