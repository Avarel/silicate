use egui::*;

type PaneButtonDrawer = fn(&mut Ui, Rect, Color32);

pub struct PaneButton {
    drawer: PaneButtonDrawer,
}

impl PaneButton {
    fn new(drawer: PaneButtonDrawer) -> Self {
        Self { drawer }
    }

    pub fn menu() -> Self {
        Self::new(Self::menu_button_drawer)
    }

    pub fn layers() -> Self {
        Self::new(Self::layer_button_drawer)
    }

    fn menu_button_drawer(ui: &mut Ui, rect: Rect, color: Color32) {
        let small_rect_size = rect.size() * 0.75;
        let bar_rect_size = small_rect_size * vec2(1.0, 0.15);
        let top_rect =
            Rect::from_center_size(rect.min + rect.size() * vec2(0.5, 0.2), bar_rect_size);
        let mid_rect = Rect::from_center_size(rect.center(), bar_rect_size);
        let bot_rect =
            Rect::from_center_size(rect.min + rect.size() * vec2(0.5, 0.8), bar_rect_size);

        ui.painter().add(Shape::rect_filled(top_rect, 1.0, color));
        ui.painter().add(Shape::rect_filled(mid_rect, 1.0, color));
        ui.painter().add(Shape::rect_filled(bot_rect, 1.0, color));
    }

    fn layer_button_drawer(ui: &mut Ui, rect: Rect, color: Color32) {
        let small_rect_size = rect.size() * 0.75;
        let back_rect =
            Rect::from_center_size(rect.min + rect.size() * vec2(0.625, 0.375), small_rect_size);
        let front_rect =
            Rect::from_center_size(rect.min + rect.size() * vec2(0.375, 0.625), small_rect_size);

        ui.painter().add(Shape::rect_filled(
            back_rect,
            3.0,
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 50),
        ));
        ui.painter().add(Shape::rect_filled(front_rect, 3.0, color));
    }

    pub fn ui(self, ui: &mut Ui, active: bool) -> Response {
        let size = vec2(25.0, 25.0);
        let (id, rect) = ui.allocate_space(size);

        let mut response = ui.interact(rect, id, Sense::click());

        let (mut icon_rect, _) = ui.spacing().icon_rectangles(response.rect);
        icon_rect.set_center(pos2(
            response.rect.left() + ui.spacing().indent / 2.0,
            response.rect.center().y,
        ));

        let visuals = ui.style().interact(&response);

        let rect = response.rect.expand(visuals.expansion);

        if response.hovered() {
            response = response.on_hover_cursor(CursorIcon::PointingHand);
        }

        let color = if active {
            super::super::ACCENT_COLOR
        } else {
            Color32::WHITE
        };

        (self.drawer)(ui, rect, color);

        response
    }
}
