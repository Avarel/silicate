pub mod pixel;

use std::ops::Range;

use rayon::{
    iter::{IndexedParallelIterator, ParallelIterator},
    slice::{ParallelSlice, ParallelSliceMut},
};

use self::pixel::{Compositable, Pixel, Rgba8, RgbaF};

pub type Rgba8Canvas = Canvas<Rgba8>;
pub type RgbaFCanvas = Canvas<RgbaF>;

#[derive(Clone)]
pub struct Canvas<P: Pixel> {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) data: Box<[P::DATA]>,
}

impl<P: Pixel> Canvas<P> {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            data: vec![P::DATA::default(); width * height * P::CHANNELS].into_boxed_slice(),
        }
    }

    pub fn from_vec(width: usize, height: usize, data: Vec<P::DATA>) -> Self {
        assert_eq!(width * height * P::CHANNELS, data.len());
        Self {
            width,
            height,
            data: data.into_boxed_slice(),
        }
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    #[inline(always)]
    fn pixel_indices(&self, x: usize, y: usize) -> Option<Range<usize>> {
        if x >= self.width || y >= self.height {
            return None;
        }

        Some(self.pixel_indices_unchecked(x, y))
    }

    #[inline(always)]
    fn pixel_indices_unchecked(&self, x: usize, y: usize) -> Range<usize> {
        let no_channels = P::CHANNELS;
        // If in bounds, this can't overflow as we have tested that at construction!
        let min_index = (y * self.width + x) * no_channels;
        min_index..min_index + no_channels
    }

    pub fn get_pixel(&self, x: usize, y: usize) -> &P {
        match self.pixel_indices(x, y) {
            None => panic!(
                "Image index {:?} out of bounds {:?}",
                (x, y),
                (self.width, self.height)
            ),
            Some(pixel_indices) => P::from_slice(&self.data[pixel_indices]),
        }
    }

    pub fn replace(&mut self, top: &Self, x: usize, y: usize) {
        self.iter_region_mut(x, y, top.width, top.height)
            .for_each(|((x, y), pixel)| {
                *pixel = *top.get_pixel(x, y);
            });
    }

    fn row_range_mut<'a>(
        &'a mut self,
        y: usize,
        height: usize,
    ) -> impl 'a + ParallelIterator<Item = (usize, &'a mut [P::DATA])> {
        self.data
            .par_chunks_exact_mut(self.width * P::CHANNELS)
            .skip(y)
            .take(height)
            .enumerate()
            .map(|(i, d)| (i, d))
    }

    pub fn iter_region_mut<'a>(
        &'a mut self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
    ) -> impl 'a + ParallelIterator<Item = ((usize, usize), &'a mut P)> {
        self.row_range_mut(y, h).flat_map(move |(iy, row)| {
            row.par_chunks_mut(P::CHANNELS)
                .skip(x)
                .take(w)
                .map(P::from_slice_mut)
                .enumerate()
                .map(move |(ix, d)| ((ix, iy), d))
        })
    }
}

impl<P: Pixel + Compositable> Canvas<P> {
    pub fn layer_clip(&mut self, mask: &Self, target_opacity: f32) {
        assert_eq!(self.dimensions(), mask.dimensions());

        let layer_iter = self
            .data
            .par_chunks_exact_mut(P::CHANNELS)
            .map(P::from_slice_mut);

        let mask_iter = mask.data.par_chunks_exact(P::CHANNELS).map(P::from_slice);

        layer_iter
            .zip_eq(mask_iter)
            .for_each(|(target, mask)| *target = mask.mask(*target, target_opacity));
    }

    pub fn layer_blend(
        &mut self,
        fg: &Self,
        fg_opacity: f32,
        blender: crate::composite::BlendingFunction,
    ) {
        assert_eq!(self.dimensions(), fg.dimensions());

        let bottom_iter = self
            .data
            .par_chunks_exact_mut(P::CHANNELS)
            .map(P::from_slice_mut);

        let top_iter = fg.data.par_chunks_exact(P::CHANNELS).map(P::from_slice);

        bottom_iter
            .zip_eq(top_iter)
            .for_each(|(bottom, top)| *bottom = bottom.blend(*top, fg_opacity, blender));
    }
}

impl Rgba8Canvas {
    pub fn to_f32(&self) -> RgbaFCanvas {
        let data = self
            .data
            .chunks_exact(Rgba8::CHANNELS)
            .map(Rgba8::from_slice)
            .flat_map(|v| RgbaF::from(*v).0)
            .collect::<Vec<_>>()
            .into_boxed_slice();

        RgbaFCanvas {
            width: self.width,
            height: self.height,
            data,
        }
    }
}

impl RgbaFCanvas {
    pub fn to_u8(&self) -> Rgba8Canvas {
        let data = self
            .data
            .chunks_exact(RgbaF::CHANNELS)
            .map(RgbaF::from_slice)
            .copied()
            .flat_map(|v| Rgba8::from(v).0)
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Rgba8Canvas {
            width: self.width,
            height: self.height,
            data,
        }
    }
}

pub mod adapter {
    use image::RgbaImage;

    use super::Rgba8Canvas;

    pub fn adapt(canvas: Rgba8Canvas) -> RgbaImage {
        RgbaImage::from_vec(
            canvas.width as u32,
            canvas.height as u32,
            canvas.data.to_vec(),
        )
        .unwrap()
    }
}