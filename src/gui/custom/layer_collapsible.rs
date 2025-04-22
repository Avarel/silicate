use egui::{collapsing_header::CollapsingState, *};

pub struct LayerCollapsible<'a> {
    id: u32,
    name: String,
    hidden: &'a mut bool,
    size_change: bool,
}

impl<'a> LayerCollapsible<'a> {
    pub fn new(id: u32, name: impl Into<String>, hidden: &'a mut bool) -> Self {
        Self {
            id,
            name: name.into(),
            hidden,
            size_change: true,
        }
    }

    pub fn size_change(mut self, size_change: bool) -> Self {
        self.size_change = size_change;
        self
    }

    fn paint_icon(ui: &mut Ui, openness: f32, response: &Response) {
        let visuals = ui.style().interact(response);

        let rect = response.rect;

        // Draw a pointy triangle arrow:
        let rect = Rect::from_center_size(rect.center(), vec2(rect.width(), rect.height() / 2.0));
        let rect = rect.expand(visuals.expansion);
        let mut points = vec![rect.right_top(), rect.center_bottom(), rect.left_top()];
        use std::f32::consts::TAU;
        let rotation = emath::Rot2::from_angle(remap(openness, 0.0..=1.0, -TAU / 4.0..=0.0));
        for p in &mut points {
            *p = rect.center() + rotation * (*p - rect.center());
        }

        ui.painter().add(Shape::line(
            points,
            Stroke::new(1.0, visuals.fg_stroke.color),
        ));
    }

    pub fn ui(self, ui: &mut Ui) -> Prepared {
        let id = ui.make_persistent_id(self.id);
        let mut state =
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);

        let mut changed = false;

        let mut overlay_ui = ui.new_child(UiBuilder::new());

        let mut control_width = 0.0;
        let mut frame = egui::Frame::new()
            .corner_radius(4)
            .inner_margin(5)
            .begin(ui);
        frame.content_ui.horizontal(|ui| {
            if self.size_change {
                ui.set_min_height((1.0 - state.openness(ui.ctx())) * 40.0);
            } else {
                ui.set_min_height(40.0);
            }

            Label::new(RichText::new(self.name).strong()).selectable(false).ui(ui);

            let response = ui
                .with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut shown = !*self.hidden;
                    changed |= Checkbox::without_text(&mut shown).ui(ui).changed();
                    *self.hidden = !shown;
                    state.show_toggle_button(ui, Self::paint_icon);
                })
                .response;

            control_width = response.rect.width();
        });

        {
            let mut label_rect = frame.frame.outer_rect(frame.content_ui.min_rect());
            label_rect.set_width(label_rect.width() - control_width);
            let response = overlay_ui.allocate_rect(label_rect, Sense::click());
            if response.clicked() {
                state.toggle(ui);
            }

        }

        let mut response = frame.allocate_space(ui);
        if response.hovered() {
            frame.frame.fill = Color32::from_gray(35)
        } else {
            frame.frame.fill = Color32::from_gray(27)
        }
        frame.paint(ui);

        if changed {
            response.mark_changed();
        }
        Prepared { state, response }
    }
}

pub struct Prepared {
    pub state: CollapsingState,
    pub response: Response,
}

impl Prepared {
    pub fn show_body_unindented<R>(
        mut self,
        ui: &mut Ui,
        add_body: impl FnOnce(&mut Ui) -> R,
    ) -> Option<InnerResponse<R>> {
        self.state.show_body_unindented(ui, add_body)
    }

    pub fn show_body_indented<R>(
        mut self,
        ui: &mut Ui,
        add_body: impl FnOnce(&mut Ui) -> R,
    ) -> Option<InnerResponse<R>> {
        self.state.show_body_indented(&self.response, ui, add_body)
    }
}
