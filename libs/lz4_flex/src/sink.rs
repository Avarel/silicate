use crate::fastcpy::slice_copy;

/// Returns a Sink implementation appropriate for outputing up to `required_capacity`
/// bytes at `vec[offset..offset+required_capacity]`.
/// It can be either a `SliceSink` (pre-filling the vec with zeroes if necessary)
/// when the `safe-decode` feature is enabled, or `VecSink` otherwise.
/// The argument `pos` defines the initial output position in the Sink.
#[inline]
pub fn vec_sink_for_decompression(
    vec: &mut Vec<u8>,
    offset: usize,
    pos: usize,
    required_capacity: usize,
) -> SliceSink {
    return {
        vec.resize(offset + required_capacity, 0);
        SliceSink::new(&mut vec[offset..], pos)
    };
}

pub trait Sink {
    /// Returns a raw ptr to the first unfilled byte of the Sink. Analogous to `[pos..].as_ptr()`.
    unsafe fn pos_mut_ptr(&mut self) -> *mut u8;

    /// read byte at position
    fn byte_at(&mut self, pos: usize) -> u8;

    unsafe fn base_mut_ptr(&mut self) -> *mut u8;

    fn pos(&self) -> usize;

    fn capacity(&self) -> usize;

    unsafe fn set_pos(&mut self, new_pos: usize);

    /// Extends the Sink with `data`.
    fn extend_from_slice(&mut self, data: &[u8]);

    fn extend_from_slice_wild(&mut self, data: &[u8], copy_len: usize);
}

/// SliceSink is used as target to de/compress data into a preallocated and possibly uninitialized
/// `&[u8]`
/// space.
///
/// # Handling of Capacity
/// Extend methods will panic if there's insufficient capacity left in the Sink.
///
/// # Invariants
///   - Bytes `[..pos()]` are always initialized.
pub struct SliceSink<'a> {
    /// The working slice, which may contain uninitialized bytes
    output: &'a mut [u8],
    /// Number of bytes in start of `output` guaranteed to be initialized
    pos: usize,
}

impl<'a> SliceSink<'a> {
    /// Creates a `Sink` backed by the given byte slice.
    /// `pos` defines the initial output position in the Sink.
    /// # Panics
    /// Panics if `pos` is out of bounds.
    #[inline]
    pub fn new(output: &'a mut [u8], pos: usize) -> Self {
        // SAFETY: Caller guarantees that all elements of `output[..pos]` are initialized.
        let _ = &mut output[..pos]; // bounds check pos
        SliceSink { output, pos }
    }
}

impl<'a> Sink for SliceSink<'a> {
    /// Returns a raw ptr to the first unfilled byte of the Sink. Analogous to `[pos..].as_ptr()`.
    #[inline]
    unsafe fn pos_mut_ptr(&mut self) -> *mut u8 {
        self.base_mut_ptr().add(self.pos()) as *mut u8
    }

    /// Pushes a byte to the end of the Sink.
    #[inline]
    fn byte_at(&mut self, pos: usize) -> u8 {
        self.output[pos]
    }

    unsafe fn base_mut_ptr(&mut self) -> *mut u8 {
        self.output.as_mut_ptr()
    }

    #[inline]
    fn pos(&self) -> usize {
        self.pos
    }

    #[inline]
    fn capacity(&self) -> usize {
        self.output.len()
    }

    #[inline]
    unsafe fn set_pos(&mut self, new_pos: usize) {
        debug_assert!(new_pos <= self.capacity());
        self.pos = new_pos;
    }

    /// Extends the Sink with `data`.
    #[inline]
    fn extend_from_slice(&mut self, data: &[u8]) {
        self.extend_from_slice_wild(data, data.len())
    }

    #[inline]
    fn extend_from_slice_wild(&mut self, data: &[u8], copy_len: usize) {
        assert!(copy_len <= data.len());
        slice_copy(data, &mut self.output[self.pos..(self.pos) + data.len()]);
        self.pos += copy_len;
    }
}

/// PtrSink is used as target to de/compress data into a preallocated and possibly uninitialized
/// `&[u8]`
/// space.
///
///
pub struct PtrSink {
    /// The working slice, which may contain uninitialized bytes
    output: *mut u8,
    /// Number of bytes in start of `output` guaranteed to be initialized
    pos: usize,
    /// Number of bytes in output available
    cap: usize,
}

impl PtrSink {
    /// Creates a `Sink` backed by the given byte slice.
    /// `pos` defines the initial output position in the Sink.
    /// # Panics
    /// Panics if `pos` is out of bounds.
    #[inline]
    pub fn from_vec(output: &mut Vec<u8>, pos: usize) -> Self {
        // SAFETY: Bytes behind pointer may be uninitialized.
        Self {
            output: output.as_mut_ptr(),
            pos,
            cap: output.capacity(),
        }
    }
}

impl Sink for PtrSink {
    /// Returns a raw ptr to the first unfilled byte of the Sink. Analogous to `[pos..].as_ptr()`.
    #[inline]
    unsafe fn pos_mut_ptr(&mut self) -> *mut u8 {
        self.base_mut_ptr().add(self.pos()) as *mut u8
    }

    /// Pushes a byte to the end of the Sink.
    #[inline]
    fn byte_at(&mut self, pos: usize) -> u8 {
        unsafe { self.output.add(pos).read() }
    }

    unsafe fn base_mut_ptr(&mut self) -> *mut u8 {
        self.output
    }

    #[inline]
    fn pos(&self) -> usize {
        self.pos
    }

    #[inline]
    fn capacity(&self) -> usize {
        self.cap
    }

    #[inline]
    unsafe fn set_pos(&mut self, new_pos: usize) {
        debug_assert!(new_pos <= self.capacity());
        self.pos = new_pos;
    }

    /// Extends the Sink with `data`.
    #[inline]
    fn extend_from_slice(&mut self, data: &[u8]) {
        self.extend_from_slice_wild(data, data.len())
    }

    #[inline]
    fn extend_from_slice_wild(&mut self, data: &[u8], copy_len: usize) {
        assert!(copy_len <= data.len());
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), self.pos_mut_ptr(), copy_len);
        }
        self.pos += copy_len;
    }

    /// Copies `len` bytes starting from `start` to the end of the Sink.
    /// # Panics
    /// Panics if `start` >= `pos`.
    #[inline]
    #[cfg(feature = "safe-decode")]
    fn extend_from_within(&mut self, _start: usize, _wild_len: usize, _copy_len: usize) {
        unreachable!();
    }

    #[inline]
    #[cfg(feature = "safe-decode")]
    fn extend_from_within_overlapping(&mut self, _start: usize, _num_bytes: usize) {
        unreachable!();
    }
}

#[cfg(test)]
mod tests {

    #[test]
    #[cfg(any(feature = "safe-encode", feature = "safe-decode"))]
    fn test_sink_slice() {
        use crate::sink::Sink;
        use crate::sink::SliceSink;
        use alloc::vec::Vec;
        let mut data = Vec::new();
        data.resize(5, 0);
        let sink = SliceSink::new(&mut data, 1);
        assert_eq!(sink.pos(), 1);
        assert_eq!(sink.capacity(), 5);
    }
}
