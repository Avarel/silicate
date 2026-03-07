pub mod button;
pub mod menu;

use egui::*;

pub struct Pane {
    name: String,
    width: f32,
}

impl Pane {
    pub fn new(name: String, width: f32) -> Self {
        Self { name, width }
    }

    pub fn show<R>(self, ui: &mut Ui, add_body: impl FnOnce(&mut Ui) -> R) {
        Frame::new()
            .fill(ui.visuals().window_fill)
            .inner_margin(10)
            .corner_radius(10)
            .shadow(Shadow {
                offset: [0, 0],
                blur: 20,
                spread: 10,
                color: ui.visuals().window_shadow.color,
            })
            .show(ui, |ui| {
                ui.set_width(self.width);
                ui.with_layout(Layout::default(), |ui| {
                    Frame::new().inner_margin(10).show(ui, |ui| {
                        ui.add(
                            Label::new(
                                RichText::new(&self.name)
                                    .heading()
                                    .strong()
                                    .color(ui.visuals().strong_text_color()),
                            )
                            .selectable(false),
                        );
                    });

                    ScrollArea::vertical()
                        .id_salt(ui.next_auto_id())
                        .show(ui, add_body);
                })
            });
    }
}
