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
        ui.painter().add(Shape::rect_filled(rect, 5.0, color));

        let text_color = Color32::DARK_GRAY;

        let galley = ui.painter().layout_no_wrap(
            "?".into(),
            TextStyle::Monospace.resolve(ui.style()),
            text_color,
        );
        let text_pos = rect.center() - galley.size() / 2.0;
        ui.painter().add(Shape::galley(text_pos, galley, text_color));
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
            visuals.fg_stroke.color
        };

        (self.drawer)(ui, rect, color);

        response
    }
}
