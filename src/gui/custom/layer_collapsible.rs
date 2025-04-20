use egui::{collapsing_header::CollapsingState, *};

pub struct LayerCollapsible<'a> {
    id: u32,
    name: String,
    hidden: &'a mut bool,
    size_change: bool,
}

impl<'a> LayerCollapsible<'a> {
    pub fn new(id: u32, name: String, hidden: &'a mut bool) -> Self {
        Self { id, name, hidden, size_change: true }
    }

    pub fn size_change(mut self, size_change: bool) -> Self {
        self.size_change = size_change;
        self
    }

    pub fn ui(self, ui: &mut Ui) -> Prepared {
        let id = ui.make_persistent_id(self.id);
        let mut state =
            egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);

        let mut changed = false;

        let mut frame = egui::Frame::new()
            .corner_radius(3)
            .inner_margin(5)
            .begin(ui);
        frame.content_ui.horizontal(|ui| {
            if self.size_change {
                ui.set_min_height((1.0 - state.openness(ui.ctx())) * 40.0);
            } else {
                ui.set_min_height(40.0);
            }

            if Label::new(self.name)
                .selectable(false)
                .sense(Sense::click())
                .ui(ui)
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
        });
        let mut response = frame.allocate_space(ui);
        if response.hovered() {
            frame.frame.fill = Color32::from_rgb(50, 50, 50)
        } else {
            frame.frame.fill = Color32::from_rgb(25, 25, 25)
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
