use image::{Pixel, Rgba, RgbaImage};
use rayon::iter::{IndexedParallelIterator, ParallelIterator};
use rayon::slice::{ParallelSlice, ParallelSliceMut};

use crate::Rgba8;

pub fn layer_clip(layer: &mut RgbaImage, mask: &RgbaImage, layer_opacity: f32) {
    assert_eq!(layer.dimensions(), mask.dimensions());

    let layer_iter = layer
        .par_chunks_exact_mut(usize::from(Rgba8::CHANNEL_COUNT))
        .map(Rgba8::from_slice_mut);

    let mask_iter = mask
        .par_chunks_exact(usize::from(Rgba8::CHANNEL_COUNT))
        .map(Rgba8::from_slice);

    layer_iter
        .zip_eq(mask_iter)
        .for_each(|(layer, mask)| *layer = mask_pixel(*mask, *layer, layer_opacity));
}

pub fn mask_pixel(mask_rgb: Rgba8, mut fg_rgb: Rgba8, fg_opacity: f32) -> Rgba8 {
    let max_t = f32::from(u8::MAX);
    let mask_a = f32::from(mask_rgb.0[3]) / max_t;
    let fg_a = (f32::from(fg_rgb.0[3]) / max_t * fg_opacity).min(mask_a);

    fg_rgb.0[3] = (fg_a * max_t) as u8;
    fg_rgb
}

pub fn layer_blend(
    bottom: &mut RgbaImage,
    top: &RgbaImage,
    top_opacity: f32,
    blender: BlendingFunction,
) {
    assert_eq!(bottom.dimensions(), top.dimensions());

    let bottom_iter = bottom
        .par_chunks_exact_mut(usize::from(Rgba8::CHANNEL_COUNT))
        .map(Rgba8::from_slice_mut);

    let top_iter = top
        .par_chunks_exact(usize::from(Rgba8::CHANNEL_COUNT))
        .map(Rgba8::from_slice);

    bottom_iter
        .zip_eq(top_iter)
        .for_each(|(bottom, top)| *bottom = blend_pixel(*bottom, *top, top_opacity, blender));
}

pub fn comp(cv: f32, alpha: f32) -> f32 {
    cv * (1.0 - alpha)
}

pub fn normal(c1: f32, c2: f32, _: f32, a2: f32) -> f32 {
    c2 + comp(c1, a2)
}

pub fn multiply(c1: f32, c2: f32, a1: f32, a2: f32) -> f32 {
    c2 * c1 + comp(c2, a1) + comp(c1, a2)
}

// works great!
pub fn screen(c1: f32, c2: f32, _: f32, _: f32) -> f32 {
    c2 + c1 - c2 * c1
}

// works great!
pub fn overlay(c1: f32, c2: f32, a1: f32, a2: f32) -> f32 {
    if c1 * 2.0 <= a1 {
        c2 * c1 * 2.0 + comp(c2, a1) + comp(c1, a2)
    } else {
        comp(c2, a1) + comp(c1, a2) - 2.0 * (a1 - c1) * (a2 - c2) + a2 * a1
    }
}

type BlendingFunction = fn(f32, f32, f32, f32) -> f32;

pub fn blend_pixel(
    bg_rgb: Rgba8,
    fg_rgb: Rgba8,
    fg_opacity: f32,
    blender: BlendingFunction,
) -> Rgba8 {
    // http://stackoverflow.com/questions/7438263/alpha-compositing-algorithm-blend-modes#answer-11163848

    // First, as we don't know what type our pixel is, we have to convert to floats between 0.0 and 1.0
    let max_t = f32::from(u8::MAX);
    let [bg @ .., bg_a] = bg_rgb.0.map(|v| f32::from(v) / max_t);
    let [fg @ .., mut fg_a] = fg_rgb.0.map(|v| f32::from(v) / max_t);
    fg_a *= fg_opacity;

    // Work out what the final alpha level will be
    let alpha_final = bg_a + fg_a - bg_a * fg_a;
    if alpha_final == 0.0 {
        return bg_rgb;
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
    Rgba([
        (max_t * out[0]) as u8,
        (max_t * out[1]) as u8,
        (max_t * out[2]) as u8,
        (max_t * alpha_final) as u8,
    ])
}
