use egui::*;

/// 2D bounding box of f64 precision.
/// The range of data values we show.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CanvasViewBounds {
    pub min: Pos2,
    pub max: Pos2,
}

impl CanvasViewBounds {
    pub const NOTHING: Self = Self {
        min: Pos2::new(f32::INFINITY, f32::INFINITY),
        max: Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
    };

    pub fn new_symmetrical(half_extent: f32) -> Self {
        Self {
            min: Pos2::from([-half_extent; 2]),
            max: Pos2::from([half_extent; 2]),
        }
    }

    pub fn is_finite(&self) -> bool {
        self.min.is_finite() && self.max.is_finite()
    }

    pub fn is_valid(&self) -> bool {
        self.is_finite() && self.width() > 0.0 && self.height() > 0.0
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    /// Expand to include the given (x,y) value
    pub fn extend_with(&mut self, value: &Vec2) {
        self.extend_with_x(value.x);
        self.extend_with_y(value.y);
    }

    /// Expand to include the given x coordinate
    pub fn extend_with_x(&mut self, x: f32) {
        self.min.x = self.min.x.min(x);
        self.max.x = self.max.x.max(x);
    }

    /// Expand to include the given y coordinate
    pub fn extend_with_y(&mut self, y: f32) {
        self.min.y = self.min.y.min(y);
        self.max.y = self.max.y.max(y);
    }

    pub fn expand_x(&mut self, pad: f32) {
        self.min.x -= pad;
        self.max.x += pad;
    }

    pub fn expand_y(&mut self, pad: f32) {
        self.min.y -= pad;
        self.max.y += pad;
    }

    pub fn merge_x(&mut self, other: &CanvasViewBounds) {
        self.min.x = self.min.x.min(other.min.x);
        self.max.x = self.max.x.max(other.max.x);
    }

    pub fn merge_y(&mut self, other: &CanvasViewBounds) {
        self.min.y = self.min.y.min(other.min.y);
        self.max.y = self.max.y.max(other.max.y);
    }

    pub fn set_x(&mut self, other: &CanvasViewBounds) {
        self.min.x = other.min.x;
        self.max.x = other.max.x;
    }

    pub fn set_y(&mut self, other: &CanvasViewBounds) {
        self.min.y = other.min.y;
        self.max.y = other.max.y;
    }

    pub fn translate_x(&mut self, delta: f32) {
        self.min.x += delta;
        self.max.x += delta;
    }

    pub fn translate_y(&mut self, delta: f32) {
        self.min.y += delta;
        self.max.y += delta;
    }

    pub fn translate(&mut self, delta: Vec2) {
        self.translate_x(delta.x);
        self.translate_y(delta.y);
    }

    pub fn add_relative_margin_x(&mut self, margin_fraction: Vec2) {
        let width = self.width().max(0.0);
        self.expand_x(margin_fraction.x * width);
    }

    pub fn add_relative_margin_y(&mut self, margin_fraction: Vec2) {
        let height = self.height().max(0.0);
        self.expand_y(margin_fraction.y * height);
    }
}

#[derive(Clone, Copy)]
pub struct AutoBounds {
    pub x: bool,
    pub y: bool,
}

impl AutoBounds {
    pub fn from_bool(val: bool) -> Self {
        AutoBounds { x: val, y: val }
    }

    pub fn any(&self) -> bool {
        self.x || self.y
    }
}

impl From<bool> for AutoBounds {
    fn from(val: bool) -> Self {
        AutoBounds::from_bool(val)
    }
}
