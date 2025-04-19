use egui::*;

const FILL_COLOR: Color32 = Color32::from_rgb(48, 116, 243);
const HANDLE_RADIUS: f32 = 5.0;

#[must_use = "You should put this widget in a ui with `ui.add(widget);`"]
pub struct OpacitySlider<'a> {
    value: &'a mut f32,
}

impl<'a> OpacitySlider<'a> {
    /// Creates a new horizontal slider.
    ///
    /// The `value` given will be clamped to the `range`,
    /// unless you change this behavior with [`Self::clamping`].
    pub fn new(value: &'a mut f32) -> Self {
        Self { value }
    }

    fn get_value(&mut self) -> f32 {
        let value = *self.value;
        value.clamp(0.0, 1.0)
    }

    fn set_value(&mut self, mut value: f32) {
        value = value.clamp(0.0, 1.0);
        *self.value = value;
    }

    /// For instance, `position` is the mouse position and `position_range` is the physical location of the slider on the screen.
    fn value_from_position(&self, position: f32, position_range: Rangef) -> f32 {
        let normalized = remap_clamp(position, position_range, 0.0..=1.0);
        normalized.clamp(0.0, 1.0)
    }

    fn position_from_value(&self, value: f32, position_range: Rangef) -> f32 {
        let normalized = value.clamp(0.0, 1.0);
        lerp(position_range, normalized)
    }
}

impl OpacitySlider<'_> {
    /// Just the slider, no text
    fn allocate_slider_space(&self, ui: &mut Ui, thickness: f32) -> Response {
        let desired_size = vec2(ui.available_width(), thickness);
        ui.allocate_response(desired_size, Sense::drag())
    }

    /// Just the slider, no text
    fn slider_ui(&mut self, ui: &Ui, response: &Response) {
        let rect = &response.rect;
        let position_range = self.position_range(rect);

        if let Some(pointer_position_2d) = response.interact_pointer_pos() {
            let position = self.pointer_position(pointer_position_2d);
            let new_value = self.value_from_position(position, position_range);
            self.set_value(new_value);
        }

        let mut decrement = 0usize;
        let mut increment = 0usize;

        if response.has_focus() {
            ui.ctx().memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    EventFilter {
                        // pressing arrows in the orientation of the
                        // slider should not move focus to next widget
                        horizontal_arrows: true,
                        vertical_arrows: false,
                        ..Default::default()
                    },
                );
            });

            let (dec_key, inc_key) = (Key::ArrowLeft, Key::ArrowRight);

            ui.input(|input| {
                decrement += input.num_presses(dec_key);
                increment += input.num_presses(inc_key);
            });
        }

        let kb_step = increment as f32 - decrement as f32;

        if kb_step != 0.0 {
            let ui_point_per_step = 1.0; // move this many ui points for each kb_step
            let prev_value = self.get_value();
            let prev_position = self.position_from_value(prev_value, position_range);
            let new_position = prev_position + ui_point_per_step * kb_step;
            let new_value = self.value_from_position(new_position, position_range);
            self.set_value(new_value);
        }

        // Paint it:
        if ui.is_rect_visible(response.rect) {
            let value = self.get_value();

            let visuals = ui.style().interact(response);
            let widget_visuals = &ui.visuals().widgets;

            let rail_rect = self.rail_rect(rect);
            let corner_radius = widget_visuals.inactive.corner_radius;

            ui.painter()
                .rect_filled(rail_rect, corner_radius, widget_visuals.inactive.bg_fill);

            let position_1d = self.position_from_value(value, position_range);
            let center = self.marker_center(position_1d, &rail_rect);

            // Paint trailing fill.
            let mut trailing_rail_rect = rail_rect;

            // The trailing rect has to be drawn differently depending on the orientation.
            trailing_rail_rect.max.x = center.x + corner_radius.nw as f32;

            ui.painter()
                .rect_filled(trailing_rail_rect, corner_radius, FILL_COLOR);

            ui.painter().add(epaint::CircleShape {
                center,
                radius: HANDLE_RADIUS + visuals.expansion,
                fill: FILL_COLOR,
                stroke: Stroke::NONE,
            });
        }
    }

    fn marker_center(&self, position_1d: f32, rail_rect: &Rect) -> Pos2 {
        pos2(position_1d, rail_rect.center().y)
    }

    fn pointer_position(&self, pointer_position_2d: Pos2) -> f32 {
        pointer_position_2d.x
    }

    fn position_range(&self, rect: &Rect) -> Rangef {
        rect.x_range().shrink(HANDLE_RADIUS)
    }

    fn rail_rect(&self, rect: &Rect) -> Rect {
        const RADIUS: f32 = 1.0;
        Rect::from_min_max(
            pos2(rect.left(), rect.center().y - RADIUS),
            pos2(rect.right(), rect.center().y + RADIUS),
        )
    }

    pub fn ui(mut self, ui: &mut Ui) -> Response {
        let old_value = self.get_value();

        ui.horizontal(|ui| {
            ui.label("Opacity");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(format!("{:.0}%", old_value * 100.0));
            });
        });

        let thickness = ui
            .text_style_height(&TextStyle::Body)
            .at_least(ui.spacing().interact_size.y);
        let mut response = self.allocate_slider_space(ui, thickness);
        self.slider_ui(ui, &response);

        let value = self.get_value();
        if value != old_value {
            response.mark_changed();
        }

        response
    }
}
