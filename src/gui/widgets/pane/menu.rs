use egui::*;

use super::{button::PaneButton, Pane};

pub struct PaneMenu {
    name: String,
    pane_button: PaneButton,
    align: Align,
}

impl PaneMenu {
    pub fn new(name: impl Into<String>, pane_button: PaneButton, align: Align) -> Self {
        Self {
            name: name.into(),
            pane_button,
            align,
        }
    }

    pub fn show<R>(self, ui: &mut Ui, add_body: impl FnOnce(&mut Ui) -> R) {
        Frame::NONE.inner_margin(10).show(ui, |ui| {
            ui.with_layout(Layout::top_down(self.align), |ui| {
                let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
                    ui.ctx(),
                    ui.next_auto_id(),
                    false,
                );

                if self.pane_button.ui(ui, state.is_open()).clicked() {
                    state.toggle(ui);
                }

                ui.allocate_space(vec2(0.0, 10.0));

                if state.is_open() {
                    Pane::new(self.name, 250.0).show(ui, add_body);
                }

                state.store(ui.ctx());
            });
        });
    }
}
