use crate::{
    app::instance::InstanceKey,
    gui::widgets::layer::{collapsible::LayerCollapsible, mask::LayerMask},
};
use egui::{load::SizedTexture, *};
use silica_gpu::SilicaHierarchy;
use std::collections::HashMap;

use super::layer::LayerControl;

pub struct LayersHierarchy<'a> {
    pub rotation: f32,
    pub previews: &'a HashMap<u32, SizedTexture>,
    pub layers: &'a mut [SilicaHierarchy],
}

impl LayersHierarchy<'_> {
    pub fn ui(self, ui: &mut Ui, idx: InstanceKey) {
        self.layers.iter_mut().for_each(|mut layer| {
            let mut has_mask = false;
            let mut blend_mode = None;

            if let SilicaHierarchy::Layer(layer) = &mut layer
                && let Some(mask_layer) = &mut layer.mask
            {
                let item_spacing_y = ui.spacing().item_spacing.y;
                ui.spacing_mut().item_spacing.y = 1.0;

                let id = mask_layer.id;
                LayerMask::new(
                    mask_layer
                        .name
                        .to_owned()
                        .unwrap_or_else(|| String::from("Unnamed Mask")),
                    &mut mask_layer.hidden,
                )
                .ui(ui, |ui| {
                    // ui.painter().rect(
                    //     ui.max_rect(),
                    //     5,
                    //     Color32::WHITE,
                    //     Stroke::NONE,
                    //     StrokeKind::Middle,
                    // );
                    Self::paint_preview(ui, self.previews, self.rotation, id);
                });
                has_mask = true;
                ui.spacing_mut().item_spacing.y = item_spacing_y;
            }

            let (id, layer_name, hidden, size_change) = match &mut layer {
                SilicaHierarchy::Layer(layer) => {
                    let layer_name = layer
                        .name
                        .to_owned()
                        .unwrap_or_else(|| String::from("Unnamed Layer"));

                    blend_mode = Some(layer.blend);

                    (layer.id, layer_name, &mut layer.hidden, false)
                }
                SilicaHierarchy::Group(layer) => {
                    let layer_name = layer
                        .name
                        .to_owned()
                        .unwrap_or_else(|| String::from("Unnamed Group"));

                    (layer.id, layer_name, &mut layer.hidden, true)
                }
            };

            let collapsible = LayerCollapsible::new(id, layer_name, hidden)
                .size_change(size_change)
                .corner_radius(if has_mask {
                    CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: crate::gui::widgets::layer::CORNER_RADIUS,
                        se: crate::gui::widgets::layer::CORNER_RADIUS,
                    }
                } else {
                    CornerRadius::same(crate::gui::widgets::layer::CORNER_RADIUS)
                })
                .has_mask(has_mask)
                .blend_mode(blend_mode)
                .ui(ui, |ui| {
                    Self::paint_preview(ui, self.previews, self.rotation, id);
                });

            match layer {
                SilicaHierarchy::Layer(layer) => {
                    collapsible
                        .show_body_unindented(ui, |ui| -> _ { LayerControl { layer }.ui(ui) });
                }
                SilicaHierarchy::Group(layer) => {
                    collapsible.show_body_indented(ui, |ui| {
                        LayersHierarchy {
                            rotation: self.rotation,
                            previews: self.previews,
                            layers: &mut layer.children,
                        }
                        .ui(ui, idx);
                    });
                }
            };
        });
    }

    fn paint_preview(ui: &mut Ui, previews: &HashMap<u32, SizedTexture>, rotation: f32, id: u32) {
        if let Some(tex) = previews.get(&id) {
            let image = Image::from_texture(*tex);

            fn round_to_nearest_quarter_turn(theta: f32) -> f32 {
                let theta = theta.rem_euclid(std::f32::consts::TAU);
                (theta / std::f32::consts::FRAC_PI_2).round() * std::f32::consts::FRAC_PI_2
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

            let rotation = round_to_nearest_quarter_turn(rotation);

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
    }
}
