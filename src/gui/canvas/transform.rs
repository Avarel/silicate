use egui::*;

use super::bounds::CanvasViewBounds;

/// Contains the screen rectangle and the plot bounds and provides methods to transform them.
#[derive(Clone, Copy)]
pub struct ScreenTransform {
    /// The screen rectangle.
    pub frame: Rect,

    /// The plot bounds.
    pub bounds: CanvasViewBounds,
}

impl ScreenTransform {
    pub fn new(frame: Rect, mut bounds: CanvasViewBounds) -> Self {
        // Make sure they are not empty.
        if !bounds.is_valid() {
            bounds = CanvasViewBounds::new_symmetrical(1.0);
        }

        Self { frame, bounds }
    }

    pub fn frame(&self) -> &Rect {
        &self.frame
    }

    pub fn bounds(&self) -> &CanvasViewBounds {
        &self.bounds
    }

    pub fn set_bounds(&mut self, bounds: CanvasViewBounds) {
        self.bounds = bounds;
    }

    pub fn translate_bounds(&mut self, delta_pos: Vec2) {
        self.bounds.translate(delta_pos / self.dvalue_dpos());
    }

    /// Zoom by a relative factor with the given screen position as center.
    pub fn zoom(&mut self, zoom_factor: Vec2, center: Pos2) {
        let center = self.value_from_position(center);

        let mut new_bounds = self.bounds;
        new_bounds.min = center + (new_bounds.min - center) / zoom_factor;
        new_bounds.max = center + (new_bounds.max - center) / zoom_factor;

        if new_bounds.is_valid() {
            self.bounds = new_bounds;
        }
    }

    pub fn position_from_point(&self, value: &Vec2) -> Pos2 {
        let x = remap(
            value.x,
            self.bounds.min.x..=self.bounds.max.x,
            (self.frame.left())..=(self.frame.right()),
        );
        let y = remap(
            value.y,
            self.bounds.min.y..=self.bounds.max.y,
            (self.frame.bottom())..=(self.frame.top()), // negated y axis!
        );
        pos2(x, y)
    }

    pub fn value_from_position(&self, pos: Pos2) -> Pos2 {
        let x = remap(
            pos.x,
            (self.frame.left())..=(self.frame.right()),
            self.bounds.min.x..=self.bounds.max.x,
        );
        let y = remap(
            pos.y,
            (self.frame.bottom())..=(self.frame.top()), // negated y axis!
            self.bounds.min.y..=self.bounds.max.y,
        );
        Pos2::new(x, y)
    }

    /// delta position / delta value
    fn dpos_dvalue_x(&self) -> f32 {
        self.frame.width() / self.bounds.width()
    }

    /// delta position / delta value
    fn dpos_dvalue_y(&self) -> f32 {
        -self.frame.height() / self.bounds.height() // negated y axis!
    }

    /// delta position / delta value
    pub fn dvalue_dpos(&self) -> Vec2 {
        Vec2::new(self.dpos_dvalue_x(), self.dpos_dvalue_y())
    }

    pub fn aspect(&self) -> f32 {
        let rw = self.frame.width();
        let rh = self.frame.height();
        (self.bounds.width() / rw) / (self.bounds.height() / rh)
    }

    /// Sets the aspect ratio by expanding the x- or y-axis.
    ///
    /// This never contracts, so we don't miss out on any data.
    pub fn set_aspect_by_expanding(&mut self, aspect: f32) {
        let current_aspect = self.aspect();

        let epsilon = 1e-5;
        if (current_aspect - aspect).abs() < epsilon {
            // Don't make any changes when the aspect is already almost correct.
            return;
        }

        if current_aspect < aspect {
            self.bounds
                .expand_x((aspect / current_aspect - 1.0) * self.bounds.width() * 0.5);
        } else {
            self.bounds
                .expand_y((current_aspect / aspect - 1.0) * self.bounds.height() * 0.5);
        }
    }
}
