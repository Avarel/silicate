use egui::{pos2, Color32, Rect};

pub mod opacity_slider;
pub mod blend_radio;
pub mod color_picker;
pub mod layer_collapsible;
pub mod pane;

const ACCENT_COLOR: Color32 = Color32::from_rgb(48, 116, 243);

fn rail_rect(rect: &Rect) -> Rect {
    const RADIUS: f32 = 1.0;
    Rect::from_min_max(
        pos2(rect.left(), rect.center().y - RADIUS),
        pos2(rect.right(), rect.center().y + RADIUS),
    )
}
