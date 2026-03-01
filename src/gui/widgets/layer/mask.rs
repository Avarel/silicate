use egui::*;

pub struct LayerMask<'a> {
    name: String,
    hidden: &'a mut bool,
}

impl<'a> LayerMask<'a> {
    pub fn new(name: impl Into<String>, hidden: &'a mut bool) -> Self {
        Self {
            name: name.into(),
            hidden,
        }
    }

    pub fn ui(self, ui: &mut Ui, preview_body: impl FnOnce(&mut Ui)) -> Response {
        let mut control_width = 0.0;
        let mut frame = egui::Frame::new()
            .corner_radius(CornerRadius {
                nw: super::CORNER_RADIUS,
                ne: super::CORNER_RADIUS,
                sw: 0,
                se: 0,
            })
            .inner_margin(super::PADDING)
            .begin(ui);
        frame.content_ui.horizontal(|ui| {
            ui.set_min_height(super::HEIGHT);

            let (mut preview_rect, _) =
                ui.allocate_exact_size(vec2(super::PREVIEW_WIDTH, super::HEIGHT), Sense::empty());
            preview_rect.set_height(preview_rect.height() + super::PADDING);

            ui.painter().add(Shape::rect_filled(
                preview_rect,
                CornerRadius {
                    nw: super::PREVIEW_CORNER_RADIUS,
                    ne: super::PREVIEW_CORNER_RADIUS,
                    sw: 0,
                    se: 0,
                },
                super::PREVIEW_BG,
            ));

            preview_body(&mut ui.new_child(UiBuilder::new().max_rect(preview_rect)));

            Label::new(RichText::new(self.name).strong())
                .selectable(false)
                .ui(ui);

            let response = ui
                .with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(10.0);

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
            frame.frame.fill = Color32::from_gray(45)
        } else {
            frame.frame.fill = Color32::from_gray(35)
        }
        frame.paint(ui);

        response
    }
}
