use std::ops::Range;

use rayon::{
    iter::{IndexedParallelIterator, ParallelIterator},
    slice::ParallelSliceMut,
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Rgba8(pub [u8; 4]);

impl Rgba8 {
    pub const CHANNELS: usize = 4;

    fn from_slice(slice: &[u8]) -> &Self {
        assert_eq!(slice.len(), Self::CHANNELS);
        unsafe { &*(slice.as_ptr() as *const Self) }
    }

    fn from_slice_mut(slice: &mut [u8]) -> &mut Self {
        assert_eq!(slice.len(), Self::CHANNELS);
        unsafe { &mut *(slice.as_mut_ptr() as *mut Self) }
    }
}

#[derive(Clone)]
pub struct Rgba8Canvas {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) data: Box<[u8]>,
}

impl Rgba8Canvas {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            data: vec![0; width * height * Rgba8::CHANNELS].into_boxed_slice(),
        }
    }

    pub fn from_vec(width: usize, height: usize, data: Vec<u8>) -> Self {
        assert_eq!(width * height * Rgba8::CHANNELS, data.len());
        Self {
            width,
            height,
            data: data.into_boxed_slice(),
        }
    }

    #[allow(dead_code)]
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
        let no_channels = Rgba8::CHANNELS;
        // If in bounds, this can't overflow as we have tested that at construction!
        let min_index = (y * self.width + x) * no_channels;
        min_index..min_index + no_channels
    }

    pub fn get_pixel(&self, x: usize, y: usize) -> &Rgba8 {
        match self.pixel_indices(x, y) {
            None => panic!(
                "Image index {:?} out of bounds {:?}",
                (x, y),
                (self.width, self.height)
            ),
            Some(pixel_indices) => Rgba8::from_slice(&self.data[pixel_indices]),
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
    ) -> impl 'a + ParallelIterator<Item = (usize, &'a mut [u8])> {
        self.data
            .par_chunks_exact_mut(self.width * Rgba8::CHANNELS)
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
    ) -> impl 'a + ParallelIterator<Item = ((usize, usize), &'a mut Rgba8)> {
        self.row_range_mut(y, h).flat_map(move |(iy, row)| {
            row.par_chunks_mut(Rgba8::CHANNELS)
                .skip(x)
                .take(w)
                .map(Rgba8::from_slice_mut)
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