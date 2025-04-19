use egui::*;
use silicate_compositor::blend::BlendingMode;

pub struct BlendModeRadio<'a> {
    value: &'a mut BlendingMode,
}

#[derive(Clone, Copy, Debug)]
pub struct BlendModeRadioLoaded;

impl BlendModeRadioLoaded {
    pub fn load(ctx: &Context, id: Id) -> Option<Self> {
        ctx.data_mut(|d| d.get_persisted(id))
    }

    pub fn store(self, ctx: &Context, id: Id) {
        ctx.data_mut(|d| d.insert_persisted(id, self));
    }
}

impl<'a> BlendModeRadio<'a> {
    pub fn new(value: &'a mut BlendingMode) -> Self {
        Self { value }
    }

    fn layout_scroll_area(&mut self, ui: &mut Ui) {
        let min_y = ui.min_rect().min.y;
        let mut scroll_to_value = 0.0;
        let mut scroll = ScrollArea::vertical().max_height(70.0).show(ui, |ui| {
            ui.set_width(ui.available_width());

            for b in BlendingMode::all() {
                let mut frame = egui::Frame::NONE
                    .inner_margin(Margin::symmetric(10, 3))
                    .begin(ui);
                {
                    let ui = &mut frame.content_ui;
                    ui.set_width(ui.available_width());
                    Label::new(RichText::new(b.as_str()).color(Color32::WHITE))
                        .selectable(false)
                        .ui(ui);
                }
                let response = ui.allocate_rect(frame.content_ui.min_rect(), Sense::click());

                if b == self.value {
                    scroll_to_value = response.rect.min.y - min_y;
                    frame.frame.fill = super::ACCENT_COLOR;
                } else if response.hovered() {
                    frame.frame.fill = Color32::from_rgb(50, 50, 50)
                }

                if response.clicked() {
                    *self.value = *b;
                }
                frame.end(ui);
            }
        });

        let loaded = BlendModeRadioLoaded::load(ui.ctx(), ui.id()).is_some();
        if !loaded {
            scroll.state.offset = vec2(0.0, scroll_to_value);
            scroll.state.store(ui.ctx(), scroll.id);
        }
        BlendModeRadioLoaded.store(ui.ctx(), ui.id());
    }

    pub fn ui(mut self, ui: &mut Ui) -> Response {
        let old_value = *self.value;

        let mut response = egui::Frame::default()
            .inner_margin(Margin::symmetric(0, 5))
            .corner_radius(4)
            .fill(Color32::from_rgb(20, 20, 20))
            .show(ui, |ui| self.layout_scroll_area(ui)).response;

        if old_value != *self.value {
            response.mark_changed();
        }
        response
    }
}
