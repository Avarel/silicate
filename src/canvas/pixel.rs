pub trait Pixel: Sync + Send + Copy {
    type DATA: Default + Copy + Sync + Send;
    const CHANNELS: usize;

    fn from_slice(slice: &[Self::DATA]) -> &Self;
    fn from_slice_mut(slice: &mut [Self::DATA]) -> &mut Self;
}

pub trait Compositable {
    fn blend(
        self,
        fg_rgba: Self,
        fg_opacity: f32,
        blender: crate::composite::BlendingFunction,
    ) -> Self;

    fn mask(self, fg_rgb: Self, fg_opacity: f32) -> Self;
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Rgba<T, const C: usize>(pub [T; C]);

impl<T: Default + Copy + Sync + Send, const C: usize> Pixel for Rgba<T, C> {
    type DATA = T;
    const CHANNELS: usize = C;

    fn from_slice(slice: &[T]) -> &Self {
        assert_eq!(slice.len(), C);
        unsafe { &*(slice.as_ptr() as *const Self) }
    }

    fn from_slice_mut(slice: &mut [T]) -> &mut Self {
        assert_eq!(slice.len(), C);
        unsafe { &mut *(slice.as_mut_ptr() as *mut Self) }
    }
}

pub type Rgba8 = Rgba<u8, 4>;
pub type RgbaF = Rgba<f32, 4>;

impl Compositable for Rgba8 {
    fn blend(
        self,
        fg_rgba: Self,
        fg_opacity: f32,
        blender: crate::composite::BlendingFunction,
    ) -> Self {
        // First, as we don't know what type our pixel is, we have to convert to floats between 0.0 and 1.0
        RgbaF::from(self)
            .blend(RgbaF::from(fg_rgba), fg_opacity, blender)
            .into()
    }

    fn mask(self, mut fg_rgb: Rgba8, fg_opacity: f32) -> Rgba8 {
        let max_t = f32::from(u8::MAX);
        let mask_a = f32::from(self.0[3]) / max_t;
        let fg_a = (f32::from(fg_rgb.0[3]) / max_t * fg_opacity).min(mask_a);

        fg_rgb.0[3] = (fg_a * max_t) as u8;
        fg_rgb
    }
}

impl From<Rgba8> for RgbaF {
    fn from(rgba: Rgba8) -> Self {
        let max_t = f32::from(u8::MAX);
        Self(rgba.0.map(f32::from).map(|v| v / max_t))
    }
}

impl From<RgbaF> for Rgba8 {
    fn from(rgba: RgbaF) -> Self {
        let max_t = f32::from(u8::MAX);
        Self(rgba.0.map(|v| v * max_t).map(|v| v as u8))
    }
}

impl Compositable for RgbaF {
    fn blend(
        self,
        fg_rgba: Self,
        fg_opacity: f32,
        blender: crate::composite::BlendingFunction,
    ) -> Self {
        // http://stackoverflow.com/questions/7438263/alpha-compositing-algorithm-blend-modes#answer-11163848

        let [bg @ .., bg_a] = self.0.map(|v| f32::from(v));
        let [fg @ .., mut fg_a] = fg_rgba.0.map(|v| f32::from(v));
        fg_a *= fg_opacity;

        // Work out what the final alpha level will be
        let alpha_final = bg_a + fg_a - bg_a * fg_a;
        if alpha_final == 0.0 {
            return self;
        }

        // We premultiply our channels by their alpha, as this makes it easier to calculate
        let bga = bg.map(|v| v * bg_a);
        let fga = fg.map(|v| v * fg_a);

        // Standard formula for src-over alpha compositing
        let outa = [
            blender(bga[0], fga[0], bg_a, fg_a),
            blender(bga[1], fga[1], bg_a, fg_a),
            blender(bga[2], fga[2], bg_a, fg_a),
        ];

        // Unmultiply the channels by our resultant alpha channel
        let out = outa.map(|v| v / alpha_final);

        // Cast back to our initial type on return
        Self([out[0], out[1], out[2], alpha_final])
    }

    fn mask(self, mut fg_rgb: Self, fg_opacity: f32) -> Self {
        fg_rgb.0[3] = (fg_rgb.0[3] * fg_opacity).min(self.0[3]);
        fg_rgb
    }
}
