use egui::*;

/// 2D bounding box of f64 precision.
/// The range of data values we show.
#[derive(Clone, Copy, PartialEq, Debug)]
struct CanvasViewBounds {
    min: Pos2,
    max: Pos2,
}

impl CanvasViewBounds {
    pub const NOTHING: Self = Self {
        min: Pos2::new(f32::INFINITY, f32::INFINITY),
        max: Pos2::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
    };

    pub(crate) fn new_symmetrical(half_extent: f32) -> Self {
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
    pub(crate) fn extend_with(&mut self, value: &Vec2) {
        self.extend_with_x(value.x);
        self.extend_with_y(value.y);
    }

    /// Expand to include the given x coordinate
    pub(crate) fn extend_with_x(&mut self, x: f32) {
        self.min.x = self.min.x.min(x);
        self.max.x = self.max.x.max(x);
    }

    /// Expand to include the given y coordinate
    pub(crate) fn extend_with_y(&mut self, y: f32) {
        self.min.y = self.min.y.min(y);
        self.max.y = self.max.y.max(y);
    }

    pub(crate) fn expand_x(&mut self, pad: f32) {
        self.min.x -= pad;
        self.max.x += pad;
    }

    pub(crate) fn expand_y(&mut self, pad: f32) {
        self.min.y -= pad;
        self.max.y += pad;
    }

    pub(crate) fn merge_x(&mut self, other: &CanvasViewBounds) {
        self.min.x = self.min.x.min(other.min[0]);
        self.max.x = self.max.x.max(other.max[0]);
    }

    pub(crate) fn merge_y(&mut self, other: &CanvasViewBounds) {
        self.min.y = self.min.y.min(other.min[1]);
        self.max.y = self.max.y.max(other.max[1]);
    }

    pub(crate) fn set_x(&mut self, other: &CanvasViewBounds) {
        self.min.x = other.min.x;
        self.max.x = other.max.x;
    }

    pub(crate) fn set_y(&mut self, other: &CanvasViewBounds) {
        self.min.y = other.min.y;
        self.max.y = other.max.y;
    }

    pub(crate) fn translate_x(&mut self, delta: f32) {
        self.min.x += delta;
        self.max.x += delta;
    }

    pub(crate) fn translate_y(&mut self, delta: f32) {
        self.min.y += delta;
        self.max.y += delta;
    }

    pub(crate) fn translate(&mut self, delta: Vec2) {
        self.translate_x(delta.x);
        self.translate_y(delta.y);
    }

    pub(crate) fn add_relative_margin_x(&mut self, margin_fraction: Vec2) {
        let width = self.width().max(0.0);
        self.expand_x(margin_fraction.x * width);
    }

    pub(crate) fn add_relative_margin_y(&mut self, margin_fraction: Vec2) {
        let height = self.height().max(0.0);
        self.expand_y(margin_fraction.y * height);
    }
}

/// Contains the screen rectangle and the plot bounds and provides methods to transform them.
#[derive(Clone, Copy)]
struct ScreenTransform {
    /// The screen rectangle.
    frame: Rect,

    /// The plot bounds.
    bounds: CanvasViewBounds,
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

    pub fn translate_bounds(&mut self, mut delta_pos: Vec2) {
        delta_pos.x *= self.dvalue_dpos()[0];
        delta_pos.y *= self.dvalue_dpos()[1];
        self.bounds.translate(delta_pos);
    }

    /// Zoom by a relative factor with the given screen position as center.
    pub fn zoom(&mut self, zoom_factor: Vec2, center: Pos2) {
        let center = self.value_from_position(center);

        let mut new_bounds = self.bounds;
        new_bounds.min[0] = center.x + (new_bounds.min[0] - center.x) / (zoom_factor.x);
        new_bounds.max[0] = center.x + (new_bounds.max[0] - center.x) / (zoom_factor.x);
        new_bounds.min[1] = center.y + (new_bounds.min[1] - center.y) / (zoom_factor.y);
        new_bounds.max[1] = center.y + (new_bounds.max[1] - center.y) / (zoom_factor.y);

        if new_bounds.is_valid() {
            self.bounds = new_bounds;
        }
    }

    pub fn position_from_point(&self, value: &Vec2) -> Pos2 {
        let x = remap(
            value.x,
            self.bounds.min[0]..=self.bounds.max[0],
            (self.frame.left())..=(self.frame.right()),
        );
        let y = remap(
            value.y,
            self.bounds.min[1]..=self.bounds.max[1],
            (self.frame.bottom())..=(self.frame.top()), // negated y axis!
        );
        pos2(x, y)
    }

    pub fn value_from_position(&self, pos: Pos2) -> Pos2 {
        let x = remap(
            pos.x,
            (self.frame.left())..=(self.frame.right()),
            self.bounds.min[0]..=self.bounds.max[0],
        );
        let y = remap(
            pos.y,
            (self.frame.bottom())..=(self.frame.top()), // negated y axis!
            self.bounds.min[1]..=self.bounds.max[1],
        );
        Pos2::new(x, y)
    }

    /// delta position / delta value
    pub fn dpos_dvalue_x(&self) -> f32 {
        self.frame.width() / self.bounds.width()
    }

    /// delta position / delta value
    pub fn dpos_dvalue_y(&self) -> f32 {
        -self.frame.height() / self.bounds.height() // negated y axis!
    }

    /// delta value / delta position
    pub fn dvalue_dpos(&self) -> [f32; 2] {
        [1.0 / self.dpos_dvalue_x(), 1.0 / self.dpos_dvalue_y()]
    }

    fn aspect(&self) -> f32 {
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

pub struct CanvasView<'a> {
    id_source: Id,

    allow_zoom: bool,
    allow_drag: bool,
    allow_scroll: bool,
    allow_rotate: bool,

    min_auto_bounds: CanvasViewBounds,
    margin_fraction: Vec2,
    allow_boxed_zoom: bool,
    boxed_zoom_pointer_button: PointerButton,

    data_aspect: Option<f32>,
    show_background: bool,

    image: Option<Image<'static>>,
    image_rotation: &'a mut f32,

    show_grid: bool,
    show_extended_crosshair: bool,
}

#[derive(Clone, Copy)]
struct AutoBounds {
    x: bool,
    y: bool,
}

impl AutoBounds {
    fn from_bool(val: bool) -> Self {
        AutoBounds { x: val, y: val }
    }

    fn any(&self) -> bool {
        self.x || self.y
    }
}

impl From<bool> for AutoBounds {
    fn from(val: bool) -> Self {
        AutoBounds::from_bool(val)
    }
}

/// Information about the plot that has to persist between frames.
#[derive(Clone, Copy)]
struct ViewMemory {
    auto_bounds: AutoBounds,
    min_auto_bounds: CanvasViewBounds,
    last_screen_transform: ScreenTransform,
    /// Allows to remember the first click position when performing a boxed zoom
    last_click_pos_for_zoom: Option<Pos2>,
}

impl ViewMemory {
    pub fn load(ctx: &Context, id: Id) -> Option<Self> {
        ctx.data_mut(|data| data.get_persisted(id))
    }

    pub fn store(self, ctx: &Context, id: Id) {
        ctx.data_mut(|data| data.insert_persisted(id, self))
    }
}

impl<'a> CanvasView<'a> {
    /// Give a unique id for each plot within the same [`Ui`].
    pub fn new(
        id_source: impl std::hash::Hash,
        image: Option<Image<'static>>,
        image_rotation: &'a mut f32,
    ) -> Self {
        Self {
            id_source: Id::new(id_source),
            allow_zoom: true,
            allow_drag: true,
            allow_scroll: true,
            allow_rotate: true,
            margin_fraction: Vec2::splat(0.05),
            allow_boxed_zoom: true,
            boxed_zoom_pointer_button: PointerButton::Secondary,
            min_auto_bounds: CanvasViewBounds::NOTHING,

            show_grid: false,
            show_extended_crosshair: false,

            data_aspect: None,
            show_background: true,
            image,
            image_rotation,
        }
    }

    pub fn show_grid(mut self, enable: bool) -> Self {
        self.show_grid = enable;
        self
    }

    pub fn show_extended_crosshair(mut self, enable: bool) -> Self {
        self.show_extended_crosshair = enable;
        self
    }

    /// Interact with and add items to the plot and finally draw it.
    pub fn show(self, ui: &mut Ui) -> InnerResponse<()> {
        let Self {
            id_source,
            allow_zoom,
            allow_scroll,
            allow_drag,
            allow_rotate,
            allow_boxed_zoom,
            boxed_zoom_pointer_button: boxed_zoom_pointer,
            min_auto_bounds,
            margin_fraction,
            data_aspect,
            show_background,
            image,
            image_rotation,
            show_extended_crosshair,
            show_grid,
            ..
        } = self;

        let size = ui.available_size();

        // Allocate the space.
        let (rect, mut response) = ui.allocate_exact_size(size, Sense::click_and_drag());

        // Load or initialize the memory.
        let plot_id = ui.make_persistent_id(id_source);
        ui.ctx().check_for_id_clash(plot_id, rect, "Plot");

        let mut memory = ViewMemory::load(ui.ctx(), plot_id).unwrap_or_else(|| ViewMemory {
            auto_bounds: (!min_auto_bounds.is_valid()).into(),
            min_auto_bounds,
            last_screen_transform: ScreenTransform::new(rect, min_auto_bounds),
            last_click_pos_for_zoom: None,
        });

        // If the min bounds changed, recalculate everything.
        if min_auto_bounds != memory.min_auto_bounds {
            memory = ViewMemory {
                auto_bounds: (!min_auto_bounds.is_valid()).into(),
                min_auto_bounds,
                ..memory
            };
            memory.clone().store(ui.ctx(), plot_id);
        }

        let ViewMemory {
            mut auto_bounds,
            last_screen_transform,
            mut last_click_pos_for_zoom,
            ..
        } = memory;

        // Background
        if show_background {
            ui.painter().with_clip_rect(rect).add(epaint::RectShape {
                rect,
                corner_radius: CornerRadius::same(2),
                fill: ui.visuals().extreme_bg_color,
                stroke: ui.visuals().widgets.noninteractive.bg_stroke,
                stroke_kind: StrokeKind::Middle,
                round_to_pixels: None,
                blur_width: 0.0,
                brush: None,
            });
        }

        // --- Bound computation ---
        let mut bounds = *last_screen_transform.bounds();

        // Allow double clicking to reset to automatic bounds.
        if response.double_clicked_by(PointerButton::Primary) {
            auto_bounds = true.into();
        }

        if !bounds.is_valid() {
            auto_bounds = true.into();
        }

        // Set bounds automatically based on content.
        if auto_bounds.any() {
            if auto_bounds.x {
                bounds.set_x(&min_auto_bounds);
            }

            if auto_bounds.y {
                bounds.set_y(&min_auto_bounds);
            }

            if let Some(image) = image.as_ref() {
                let image_size = image.size().unwrap();
                let image_bounds = {
                    let mut bounds = CanvasViewBounds::NOTHING;
                    let left_top = Vec2::new(-image_size.x / 2.0, -image_size.y / 2.0);
                    let right_bottom = Vec2::new(image_size.x / 2.0, image_size.y / 2.0);
                    bounds.extend_with(&left_top);
                    bounds.extend_with(&right_bottom);
                    bounds
                };

                if auto_bounds.x {
                    bounds.merge_x(&image_bounds);
                }
                if auto_bounds.y {
                    bounds.merge_y(&image_bounds);
                }
            }

            if auto_bounds.x {
                bounds.add_relative_margin_x(margin_fraction);
            }

            if auto_bounds.y {
                bounds.add_relative_margin_y(margin_fraction);
            }
        }

        let mut transform = ScreenTransform::new(rect, bounds);

        // Enforce aspect ratio
        transform.set_aspect_by_expanding(1.0);

        // Dragging
        if allow_drag && response.dragged_by(PointerButton::Primary) {
            response = response.on_hover_cursor(CursorIcon::Grabbing);
            transform.translate_bounds(-response.drag_delta());
            auto_bounds = false.into();
        }

        let image_size = image.as_ref().and_then(|image| image.size());

        let prepared = PreparedView {
            image,
            image_rotation,
            show_extended_crosshair,
            show_grid,
            transform,
        };
        prepared.ui(ui, &response);

        if response.double_clicked_by(PointerButton::Middle) {
            *image_rotation = (*image_rotation / std::f32::consts::FRAC_PI_2).round() * std::f32::consts::FRAC_PI_2;
        }

        // Rotation
        if response.dragged_by(PointerButton::Middle) {
            response = response.on_hover_cursor(CursorIcon::Move);
            let delta = response.drag_delta();
            if let Some(hover_pos) = response.hover_pos() {
                let frame = vec2(transform.frame.width(), transform.frame.height());

                if let Some(image_size) = image_size {
                    let rect = {
                        let left_top = Vec2::new(-image_size.x / 2.0, -image_size.y / 2.0);
                        let right_bottom = Vec2::new(image_size.x / 2.0, image_size.y / 2.0);
                        let left_top_tf = transform.position_from_point(&left_top);
                        let right_bottom_tf = transform.position_from_point(&right_bottom);
                        Rect::from_two_pos(left_top_tf, right_bottom_tf)
                    };
                    let image_screen_center = ((rect.max - rect.min) / 2.0) / image_size;

                    let image_pos_center = rect.min + image_screen_center * image_size;

                    let hover_norm_pos = hover_pos - image_pos_center.to_vec2();
                    let p1 = (hover_norm_pos - delta).to_vec2() / frame;
                    let p2 = hover_norm_pos.to_vec2() / frame;

                    let theta = f32::atan2(p2.y, p2.x) - f32::atan2(p1.y, p1.x);

                    *image_rotation += theta;

                    let painter = ui.painter();
                    painter.add(Shape::dashed_line(
                        &[image_pos_center, hover_pos],
                        Stroke::new(2., Color32::GREEN),
                        2.0,
                        3.0,
                    ));
                }
            }
        }

        // Zooming
        if allow_boxed_zoom {
            // Save last click to allow boxed zooming
            if response.drag_started() && response.dragged_by(boxed_zoom_pointer) {
                // it would be best for egui that input has a memory of the last click pos because it's a common pattern
                last_click_pos_for_zoom = response.hover_pos();
            }
            let box_start_pos = last_click_pos_for_zoom;
            let box_end_pos = response.hover_pos();
            if let (Some(box_start_pos), Some(box_end_pos)) = (box_start_pos, box_end_pos) {
                response = response.on_hover_cursor(CursorIcon::Crosshair);

                let painter = ui.painter().with_clip_rect(transform.frame);

                let theta = *image_rotation;
                let x_rotated_unit_vector = vec2(f32::cos(theta), f32::sin(theta));

                let box_dim = box_end_pos - box_start_pos;
                let box_dim_x_proj = box_dim.dot(x_rotated_unit_vector) * x_rotated_unit_vector;

                let box_pt1 = box_start_pos + box_dim_x_proj;
                let box_pt2 = box_end_pos - box_dim_x_proj;

                let draw_poly = |points: &[Pos2], stroke: Stroke| {
                    for window in points.windows(2) {
                        let &[pos1, pos2] = window else {
                            unsafe { std::hint::unreachable_unchecked() }
                        };
                        painter.line_segment([pos1, pos2], stroke);
                    }
                };

                let box_positions = [box_start_pos, box_pt1, box_end_pos, box_pt2, box_start_pos];

                draw_poly(&box_positions, epaint::Stroke::new(5., Color32::BLACK));
                draw_poly(&box_positions, epaint::Stroke::new(2., Color32::WHITE));

                // when the click is release perform the zoom
                if response.drag_stopped() {
                    let box_start_pos = transform.value_from_position(box_start_pos);
                    let box_end_pos = transform.value_from_position(box_end_pos);
                    let new_bounds = CanvasViewBounds {
                        min: box_start_pos.min(box_end_pos),
                        max: box_start_pos.max(box_end_pos),
                    };
                    if new_bounds.is_valid() {
                        transform.set_bounds(new_bounds);
                        auto_bounds = false.into();
                    }
                    // reset the boxed zoom state
                    last_click_pos_for_zoom = None;
                }
            }
        }

        if let Some(hover_pos) = response.hover_pos() {
            if allow_zoom {
                let zoom_factor = if data_aspect.is_some() {
                    Vec2::splat(ui.input(|i| i.zoom_delta()))
                } else {
                    ui.input(|i| i.zoom_delta_2d())
                };
                if zoom_factor != Vec2::splat(1.0) {
                    transform.zoom(zoom_factor, hover_pos);
                    auto_bounds = false.into();
                }
            }
            if allow_scroll {
                let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
                if scroll_delta != Vec2::ZERO {
                    transform.translate_bounds(-scroll_delta);
                    auto_bounds = false.into();
                }
            }
            if allow_rotate {
                let multi_touch = ui.input(|i| i.multi_touch());
                if let Some(multi_touch) = multi_touch {
                    *image_rotation += multi_touch.rotation_delta;
                }
            }
        }

        let memory = ViewMemory {
            auto_bounds,
            min_auto_bounds,
            last_screen_transform: transform,
            last_click_pos_for_zoom,
        };
        memory.store(ui.ctx(), plot_id);

        InnerResponse {
            inner: (),
            response,
        }
    }
}

struct PreparedView<'a> {
    image: Option<Image<'static>>,
    transform: ScreenTransform,
    image_rotation: &'a mut f32,
    show_grid: bool,
    show_extended_crosshair: bool,
}

impl PreparedView<'_> {
    fn ui(self, ui: &mut Ui, response: &Response) {
        let transform = &self.transform;

        let mut plot_ui = ui.new_child(UiBuilder::new().max_rect(*transform.frame()));
        plot_ui.set_clip_rect(*transform.frame());
        plot_ui.painter().rect(
            plot_ui.max_rect(),
            CornerRadius::default(),
            Color32::from_gray(20),
            Stroke::NONE,
            StrokeKind::Outside,
        );

        if self.show_grid {
            let painter = plot_ui.painter();

            for x in (plot_ui.max_rect().min.x as u32..plot_ui.max_rect().max.x as u32).step_by(15)
            {
                painter.vline(
                    x as f32,
                    plot_ui.max_rect().y_range(),
                    Stroke::new(1.0, Color32::from_gray(30)),
                );
            }
            for y in (plot_ui.max_rect().min.y as u32..plot_ui.max_rect().max.y as u32).step_by(15)
            {
                painter.hline(
                    plot_ui.max_rect().x_range(),
                    y as f32,
                    Stroke::new(1.0, Color32::from_gray(30)),
                );
            }
        }

        if let Some(image) = self.image {
            let image_size = image.size().unwrap();
            let rect = {
                let left_top = Vec2::new(-image_size.x / 2.0, -image_size.y / 2.0);
                let right_bottom = Vec2::new(image_size.x / 2.0, image_size.y / 2.0);
                let left_top_tf = transform.position_from_point(&left_top);
                let right_bottom_tf = transform.position_from_point(&right_bottom);
                Rect::from_two_pos(left_top_tf, right_bottom_tf)
            };
            let image_screen_center = ((rect.max - rect.min) / 2.0) / image_size;

            let painter = plot_ui.painter();
            painter.add(Shape::mesh({
                let mut mesh = Mesh::default();
                mesh.add_colored_rect(
                    rect.expand(2.0),
                    Color32::from_rgba_premultiplied(0, 0, 0, 100),
                );
                mesh.add_colored_rect(rect, Color32::from_rgba_premultiplied(10, 10, 10, 50));

                mesh.rotate(
                    emath::Rot2::from_angle(*self.image_rotation),
                    rect.min + image_screen_center * image_size,
                );
                mesh
            }));

            image
                .rotate(*self.image_rotation, Vec2::splat(0.5))
                .paint_at(&mut plot_ui, rect);
        }

        if self.show_extended_crosshair {
            let painter = plot_ui.painter();
            if let Some(pointer) = response.hover_pos() {
                painter.add(Shape::mesh({
                    let mut mesh = Mesh::default();

                    let vline = Rect::from_two_pos(
                        pos2(0.0, -plot_ui.max_rect().height()),
                        pos2(1.0, plot_ui.max_rect().height()),
                    );
                    let hline = Rect::from_two_pos(
                        pos2(-plot_ui.max_rect().width(), 0.0),
                        pos2(plot_ui.max_rect().width(), 1.0),
                    );

                    mesh.add_colored_rect(
                        vline.translate(pointer.to_vec2()),
                        ui.visuals().weak_text_color(),
                    );
                    mesh.add_colored_rect(
                        hline.translate(pointer.to_vec2()),
                        ui.visuals().weak_text_color(),
                    );

                    mesh.rotate(emath::Rot2::from_angle(*self.image_rotation), pointer);
                    mesh
                }));
            }
        }
    }
}
