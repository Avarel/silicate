use egui::{collapsing_header::CollapsingState, *};

pub struct LayerCollapsible<'a> {
    id: u32,
    name: String,
    hidden: &'a mut bool,
    size_change_on_open: bool,
    corner_radius: CornerRadius,
    has_mask: bool,
    blend_mode: Option<silica_gpu::BlendingMode>,
}

impl<'a> LayerCollapsible<'a> {
    pub fn new(id: u32, name: impl Into<String>, hidden: &'a mut bool) -> Self {
        Self {
            id,
            name: name.into(),
            hidden,
            size_change_on_open: true,
            corner_radius: CornerRadius::same(super::CORNER_RADIUS),
            has_mask: false,
            blend_mode: None,
        }
    }

    pub fn size_change(mut self, size_change: bool) -> Self {
        self.size_change_on_open = size_change;
        self
    }

    pub fn corner_radius(mut self, corner_radius: CornerRadius) -> Self {
        self.corner_radius = corner_radius;
        self
    }

    pub fn has_mask(mut self, has_mask: bool) -> Self {
        self.has_mask = has_mask;
        self
    }

    pub fn blend_mode(mut self, blend_mode: Option<silica_gpu::BlendingMode>) -> Self {
        self.blend_mode = blend_mode;
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

    pub fn ui(self, ui: &mut Ui, preview_body: impl FnOnce(&mut Ui)) -> Prepared {
        let id = ui.make_persistent_id(self.id);
        let mut state =
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);

        let mut overlay_ui = ui.new_child(UiBuilder::new());

        let mut control_width = 0.0;
        let mut frame = egui::Frame::new()
            .corner_radius(self.corner_radius)
            .inner_margin(3)
            .begin(ui);
        frame.content_ui.horizontal(|ui| {
            if self.size_change_on_open {
                ui.set_min_height((1.0 - state.openness(ui.ctx())) * super::HEIGHT);
            } else {
                ui.set_min_height(super::HEIGHT);
            }

            if !state.is_open() || !self.size_change_on_open {
                let (mut preview_rect, _) = ui
                    .allocate_exact_size(vec2(super::PREVIEW_WIDTH, super::HEIGHT), Sense::empty());
                if self.has_mask {
                    preview_rect = preview_rect.translate(vec2(0.0, -3.0));
                    preview_rect.set_height(preview_rect.height() + 3.0);
                    ui.painter().add(Shape::rect_filled(
                        preview_rect,
                        CornerRadius {
                            nw: 0,
                            ne: 0,
                            sw: super::PREVIEW_CORNER_RADIUS,
                            se: super::PREVIEW_CORNER_RADIUS,
                        },
                        super::PREVIEW_BG,
                    ));
                } else {
                    ui.painter()
                        .add(Shape::rect_filled(preview_rect, 5, super::PREVIEW_BG));
                }

                preview_body(&mut ui.new_child(UiBuilder::new().max_rect(preview_rect)));
            } else {
                let (preview_rect, _) = ui.allocate_exact_size(
                    vec2(
                        super::PREVIEW_WIDTH,
                        remap(state.openness(ui.ctx()), 0.0..=1.0, super::HEIGHT..=5.0),
                    ),
                    Sense::empty(),
                );
                ui.painter()
                    .add(Shape::rect_filled(preview_rect, 2, super::PREVIEW_BG));
            }

            Label::new(RichText::new(self.name).strong())
                .selectable(false)
                .ui(ui);

            let response = ui
                .with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(10.0);

                    let mut shown = !*self.hidden;
                    Checkbox::without_text(&mut shown).ui(ui);
                    *self.hidden = !shown;

                    if let Some(blend_mode) = self.blend_mode {
                        ui.add_space(5.0);
                        Label::new(blend_mode.as_short_str())
                            .selectable(false)
                            .ui(ui);
                    } else {
                        state.show_toggle_button(ui, Self::paint_icon);
                    }
                })
                .response;

            control_width = response.rect.width();
        });

        {
            let mut label_rect = frame.frame.outer_rect(frame.content_ui.min_rect());
            label_rect.set_width(label_rect.width() - control_width);
            let response = overlay_ui.allocate_rect(label_rect, Sense::click());
            if response.clicked() || response.secondary_clicked() {
                state.toggle(ui);
            }
        }

        let response = frame.allocate_space(ui);
        if response.hovered() {
            frame.frame.fill = Color32::from_gray(45)
        } else {
            frame.frame.fill = Color32::from_gray(35)
        }
        frame.paint(ui);

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
