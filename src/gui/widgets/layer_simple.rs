use egui::*;

pub struct LayerSimple<'a> {
    name: String,
    hidden: &'a mut bool,
}

impl<'a> LayerSimple<'a> {
    pub fn new(name: impl Into<String>, hidden: &'a mut bool) -> Self {
        Self {
            name: name.into(),
            hidden,
        }
    }

    pub fn ui(self, ui: &mut Ui, preview_body: impl FnOnce(&mut Ui)) -> Response {
        const PREVIEW_WIDTH: f32 = 60.0;
        const HEIGHT: f32 = 50.0;
        const PREVIEW_BG: Color32 = Color32::from_gray(30);

        let mut control_width = 0.0;
        let mut frame = egui::Frame::new()
            .corner_radius(4)
            .inner_margin(3)
            .begin(ui);
        frame.content_ui.horizontal(|ui| {
            ui.set_min_height(HEIGHT);

            let (preview_rect, _) =
                ui.allocate_exact_size(vec2(PREVIEW_WIDTH, HEIGHT), Sense::empty());
            ui.painter()
                .add(Shape::rect_filled(preview_rect, 5, PREVIEW_BG));

            preview_body(&mut ui.new_child(UiBuilder::new().max_rect(preview_rect)));

            Label::new(RichText::new(self.name).strong())
                .selectable(false)
                .ui(ui);

            let response = ui
                .with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut shown = !*self.hidden;
                    Checkbox::without_text(&mut shown).ui(ui);
                    *self.hidden = !shown;
                })
                .response;

            control_width = response.rect.width();
        });

        {
            let mut label_rect = frame.frame.outer_rect(frame.content_ui.min_rect());
            label_rect.set_width(label_rect.width() - control_width);
        }

        let response = frame.allocate_space(ui);
        if response.hovered() {
            frame.frame.fill = Color32::from_gray(35)
        } else {
            frame.frame.fill = Color32::from_gray(27)
        }
        frame.paint(ui);

        response
    }
}
