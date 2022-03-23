use std::ops::Range;

use rayon::{
    iter::{IndexedParallelIterator, ParallelIterator},
    slice::ParallelSliceMut,
};

pub trait Pixel: Sync + Send + Copy {
    type DATA: Default + Copy + Sync + Send;
    const CHANNELS: usize;

    fn from_slice(slice: &[Self::DATA]) -> &Self;
    fn from_slice_mut(slice: &mut [Self::DATA]) -> &mut Self;
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Rgba<T, const C: usize>([T; C]);

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
pub type Rgba8Canvas = Canvas<Rgba8>;
pub type RgbaFCanvas = Canvas<RgbaF>;

pub struct Canvas<P: Pixel> {
    width: usize,
    height: usize,
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
}

impl<P: Pixel> Canvas<P> {
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

    pub fn replace(&mut self, top: &Canvas<P>, x: usize, y: usize) {
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
            row.par_chunks_mut(Rgba8::CHANNELS)
                .skip(x)
                .take(w)
                .map(P::from_slice_mut)
                .enumerate()
                .map(move |(ix, d)| ((ix, iy), d))
        })
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
