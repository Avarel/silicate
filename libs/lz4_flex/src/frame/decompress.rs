use std::{
    fmt,
    io::{self, BufRead, Cursor, Read},
};

use super::header::BlockInfo;
use super::Error;
use crate::sink::vec_sink_for_decompression;

pub struct FrameDecoder<'a> {
    /// The underlying reader.
    r: Cursor<&'a [u8]>,
    /// The compressed bytes buffer, taken from the underlying reader.
    src: Vec<u8>,
    /// The decompressed bytes buffer. Bytes are decompressed from src to dst
    /// before being passed back to the caller.
    dst: &'a mut Vec<u8>,
    /// Index into dst: starting point of bytes not yet read by caller.
    dst_start: usize,
    /// Index into dst: ending point of bytes not yet read by caller.
    dst_end: usize,
}

impl<'a> FrameDecoder<'a> {
    /// Creates a new Decoder for the specified reader.
    pub fn new(rdr: &'a [u8], dst: &'a mut Vec<u8>) -> FrameDecoder<'a> {
        FrameDecoder {
            r: Cursor::new(rdr),
            src: Vec::new(),
            dst,
            dst_start: 0,
            dst_end: 0,
        }
    }

    fn read_block(&mut self) -> io::Result<usize> {
        debug_assert_eq!(self.dst_start, self.dst_end);

        // Read and decompress block
        let block_info = BlockInfo::read(&mut self.r)?;

        match block_info {
            BlockInfo::Uncompressed(len) => {
                let len = len as usize;
                // TODO: Attempt to avoid initialization of read buffer when
                // https://github.com/rust-lang/rust/issues/42788 stabilizes
                self.r.read_exact(vec_resize_and_get_mut(
                    &mut self.dst,
                    self.dst_start,
                    self.dst_start + len,
                ))?;

                // self.dst
                //     .extend_from_slice(&self.r.get_ref()[self.r.position() as usize..][..len]);

                self.dst_end += len;
            }
            BlockInfo::Compressed(len, block_size) => {
                if len > block_size {
                    return Err(Error::BlockTooBig.into());
                }
                let len = len as usize;
                let block_size = block_size as usize;

                let src = &self.r.get_ref()[self.r.position() as usize..];
                self.r.consume(len);

                // Independent blocks OR linked blocks with only prefix data
                let decomp_size = crate::block::decompress::decompress_internal::<false, _>(
                    &src[..len],
                    &mut vec_sink_for_decompression(
                        &mut self.dst,
                        0,
                        self.dst_start,
                        self.dst_start + block_size,
                    ),
                    b"",
                )
                .map_err(Error::DecompressionError)?;

                // self.dst.resize(self.dst_end + block_size, 0);
                // let decomp_size =
                //     crate::block::decompress_into(&src[..len], &mut self.dst[self.dst_end..])
                //         .map_err(Error::DecompressionError)?;

                if decomp_size != block_size {
                    return Err(Error::ContentLengthError {
                        expected: block_size as u64,
                        actual: decomp_size as u64,
                    }
                    .into());
                }

                debug_assert_eq!(block_size, decomp_size);

                self.dst_end += decomp_size;
            }

            BlockInfo::EndMark => {
                return Ok(0);
            }
        }

        Ok(self.dst_end - self.dst_start)
    }

    fn read_more(&mut self) -> io::Result<usize> {
        self.read_block()
    }

    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.dst_start == self.dst_end {
            self.read_more()?;
        }
        Ok(&self.dst[self.dst_start..self.dst_end])
    }

    fn consume(&mut self, amt: usize) {
        assert!(amt <= self.dst_end - self.dst_start);
        self.dst_start += amt;
    }

    pub fn read_to_end(&mut self) -> io::Result<usize> {
        let mut written = 0;
        loop {
            match self.fill_buf() {
                Ok(b) if b.is_empty() => return Ok(written),
                Ok(b) => {
                    let len = b.len();
                    self.consume(len);
                    written += len;
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }
}

/// Similar to `v.get_mut(start..end) but will adjust the len if needed.
#[inline]
fn vec_resize_and_get_mut(v: &mut Vec<u8>, start: usize, end: usize) -> &mut [u8] {
    if end > v.len() {
        v.resize(end, 0)
    }
    &mut v[start..end]
}
