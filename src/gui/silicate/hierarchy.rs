use crate::{
    addendum::SilicaHierarchyAddendum, app::instance::InstanceKey,
    gui::widgets::layer_collapsible::LayerCollapsible,
};
use egui::{load::SizedTexture, *};
use silica::layers::SilicaHierarchy;
use std::collections::HashMap;

use super::layer::LayerControl;

pub struct LayersHierarchy<'a> {
    pub rotation: f32,
    pub previews: &'a HashMap<(InstanceKey, u32), SizedTexture>,
    pub layers: &'a mut [SilicaHierarchy],
    pub addendum: &'a [SilicaHierarchyAddendum],
}

impl LayersHierarchy<'_> {
    pub fn ui(self, ui: &mut Ui, idx: InstanceKey) {
        self.layers
            .iter_mut()
            .zip(self.addendum.iter())
            .for_each(|(mut layer, addendum)| {
                let (id, layer_name, hidden, size_change) = match (&mut layer, addendum) {
                    (SilicaHierarchy::Layer(layer), SilicaHierarchyAddendum::Layer(addendum)) => {
                        let layer_name = layer
                            .name
                            .to_owned()
                            .unwrap_or_else(|| String::from("Unnamed Layer"));

                        (addendum.id, layer_name, &mut layer.hidden, false)
                    }
                    (SilicaHierarchy::Group(layer), SilicaHierarchyAddendum::Group(addendum)) => {
                        let layer_name = layer
                            .name
                            .to_owned()
                            .unwrap_or_else(|| String::from("Unnamed Group"));

                        (addendum.id, layer_name, &mut layer.hidden, true)
                    }
                    _ => unreachable!(),
                };

                let collapsible = LayerCollapsible::new(id, layer_name, hidden)
                    .size_change(size_change)
                    .ui(ui, |ui| {
                        if let Some(tex) = self.previews.get(&(idx, id)) {
                            let image = Image::from_texture(*tex);

                            fn round_to_nearest_quarter_turn(theta: f32) -> f32 {
                                let theta = theta.rem_euclid(std::f32::consts::TAU);
                                (theta / std::f32::consts::FRAC_PI_2).round()
                                    * std::f32::consts::FRAC_PI_2
                            }

                            fn is_upright(theta: f32) -> bool {
                                let deg = theta.rem_euclid(std::f32::consts::TAU).to_degrees();
                                !(45.0..135.0).contains(&deg) && !(225.0..315.0).contains(&deg)
                            }

                            fn make_max_fit_rect(max_rect: Rect, size: Vec2) -> Rect {
                                let scale_x = max_rect.width() / size.x;
                                let scale_y = max_rect.height() / size.y;
                                let size = scale_x.min(scale_y) * size;
                                Rect::from_center_size(max_rect.center(), size)
                            }

                            let rotation = round_to_nearest_quarter_turn(self.rotation);

                            let original_image_size = image.size().expect("wgpu texture have size");
                            let mut image_size = original_image_size;
                            if is_upright(rotation) {
                                std::mem::swap(&mut image_size.x, &mut image_size.y);
                            }

                            let max_rect_fit = make_max_fit_rect(ui.max_rect(), image_size);
                            image_size.x = max_rect_fit.width();
                            image_size.y = max_rect_fit.height();

                            if !is_upright(rotation) {
                                std::mem::swap(&mut image_size.x, &mut image_size.y);
                            }

                            image.rotate(rotation, Vec2::splat(0.5)).paint_at(
                                ui,
                                Rect::from_center_size(ui.max_rect().center(), image_size),
                            );
                        }
                    });

                match (layer, addendum) {
                    (SilicaHierarchy::Layer(layer), SilicaHierarchyAddendum::Layer(addendum)) => {
                        collapsible.show_body_unindented(ui, |ui| -> _ {
                            LayerControl { layer, addendum }.ui(ui)
                        });
                    }
                    (SilicaHierarchy::Group(layer), SilicaHierarchyAddendum::Group(addendum)) => {
                        collapsible.show_body_indented(ui, |ui| {
                            LayersHierarchy {
                                rotation: self.rotation,
                                previews: self.previews,
                                layers: &mut layer.children,
                                addendum: &addendum.children,
                            }
                            .ui(ui, idx);
                        });
                    }
                    _ => unreachable!(),
                };
            });
    }
}
