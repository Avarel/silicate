use egui::{collapsing_header::CollapsingState, *};

pub struct LayerCollapsible<'a> {
    id: u32,
    name: String,
    hidden: &'a mut bool,
}

impl<'a> LayerCollapsible<'a> {
    pub fn new(id: u32, name: String, hidden: &'a mut bool) -> Self {
        Self { id, name, hidden }
    }

    pub fn ui(self, ui: &mut Ui) -> Prepared {
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            ui.make_persistent_id(self.id),
            false,
        );

        let mut changed = false;

        let mut response = ui
            .horizontal(|ui| {
                let mut frame = egui::Frame::new()
                    .corner_radius(3)
                    .inner_margin(5)
                    .begin(ui);
                {
                    let ui = &mut frame.content_ui;
                    if ui
                        .add(
                            Label::new(self.name)
                                .selectable(false)
                                .sense(Sense::click()),
                        )
                        .clicked()
                    {
                        state.toggle(ui);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut shown = !*self.hidden;
                        changed |= Checkbox::without_text(&mut shown).ui(ui).changed();
                        *self.hidden = !shown;
                        state.show_toggle_button(ui, egui::collapsing_header::paint_default_icon);
                    });
                }
                let response = frame.allocate_space(ui);
                if response.hovered() {
                    frame.frame.fill = Color32::from_rgb(50, 50, 50)
                } else {
                    frame.frame.fill = Color32::from_rgb(25, 25, 25)
                }
                frame.end(ui)
            })
            .inner;

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
