//! # FastCpy
//!
//! The Rust Compiler calls `memcpy` for slices of unknown length.
//! This crate provides a faster implementation of `memcpy` for slices up to 32bytes (64bytes with `avx`).
//! If you know most of you copy operations are not too big you can use `fastcpy` to speed up your program.
//!
//! `fastcpy` is designed to contain not too much assembly, so the overhead is low.
//!
//! As fall back the standard `memcpy` is called
//!
//! ## Double Copy Trick
//! `fastcpy` employs a double copy trick to copy slices of length 4-32bytes (64bytes with `avx`).
//! E.g. Slice of length 6 can be copied with two uncoditional copy operations.
//!
//! /// [1, 2, 3, 4, 5, 6]
//! /// [1, 2, 3, 4]
//! ///       [3, 4, 5, 6]
//!

#[inline]
pub fn slice_copy(src: *const u8, dst: *mut u8, num_bytes: usize) {
    if num_bytes < 4 {
        short_copy(src, dst, num_bytes);
        return;
    }

    if num_bytes < 8 {
        double_copy_trick::<4>(src, dst, num_bytes);
        return;
    }

    if num_bytes <= 16 {
        double_copy_trick::<8>(src, dst, num_bytes);
        return;
    }

    //if num_bytes <= 32 {
    //double_copy_trick::<16>(src, dst, num_bytes);
    //return;
    //}

    // /// The code will use the vmovdqu instruction to copy 32 bytes at a time.
    //#[cfg(target_feature = "avx")]
    //{
    //if num_bytes <= 64 {
    //double_copy_trick::<32>(src, dst, num_bytes);
    //return;
    //}
    //}

    // For larger sizes we use the default, which calls memcpy
    // memcpy does some virtual memory tricks to copy large chunks of memory.
    //
    // The theory should be that the checks above don't cost much relative to the copy call for
    // larger copies.
    // The bounds checks in `copy_from_slice` are elided.

    //unsafe { core::ptr::copy_nonoverlapping(src, dst, num_bytes) }
    wild_copy_from_src::<16>(src, dst, num_bytes)
}

// Inline never because otherwise we get a call to memcpy -.-
#[inline]
fn wild_copy_from_src<const SIZE: usize>(
    mut source: *const u8,
    mut dst: *mut u8,
    num_bytes: usize,
) {
    // Note: if the compiler auto-vectorizes this it'll hurt performance!
    // It's not the case for 16 bytes stepsize, but for 8 bytes.
    let l_last = unsafe { source.add(num_bytes - SIZE) };
    let r_last = unsafe { dst.add(num_bytes - SIZE) };
    let num_bytes = (num_bytes / SIZE) * SIZE;

    unsafe {
        let dst_ptr_end = dst.add(num_bytes);
        loop {
            core::ptr::copy_nonoverlapping(source, dst, SIZE);
            source = source.add(SIZE);
            dst = dst.add(SIZE);
            if dst >= dst_ptr_end {
                break;
            }
        }
    }

    unsafe {
        core::ptr::copy_nonoverlapping(l_last, r_last, SIZE);
    }
}

#[inline]
fn short_copy(src: *const u8, dst: *mut u8, len: usize) {
    unsafe {
        *dst = *src;
    }
    if len >= 2 {
        double_copy_trick::<2>(src, dst, len);
    }
}

#[inline(always)]
/// [1, 2, 3, 4, 5, 6]
/// [1, 2, 3, 4]
///       [3, 4, 5, 6]
fn double_copy_trick<const SIZE: usize>(src: *const u8, dst: *mut u8, len: usize) {
    let l_end = unsafe { src.add(len - SIZE) };
    let r_end = unsafe { dst.add(len - SIZE) };

    unsafe {
        core::ptr::copy_nonoverlapping(src, dst, SIZE);
        core::ptr::copy_nonoverlapping(l_end, r_end, SIZE);
    }
}
