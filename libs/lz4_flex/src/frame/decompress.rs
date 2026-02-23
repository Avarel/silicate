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
    /// The decompressed bytes buffer. Bytes are decompressed from src to dst
    /// before being passed back to the caller.
    dst: &'a mut Vec<u8>,
    /// Index into dst: ending point of bytes not yet read by caller.
    dst_end: usize,
}

impl<'a> FrameDecoder<'a> {
    /// Creates a new Decoder for the specified reader.
    pub fn new(rdr: &'a [u8], dst: &'a mut Vec<u8>) -> FrameDecoder<'a> {
        FrameDecoder {
            r: Cursor::new(rdr),
            dst,
            // dst_start: 0,
            dst_end: 0,
        }
    }

    fn read_block(&mut self) -> io::Result<usize> {
        // Read and decompress block
        let block_info = BlockInfo::read(&mut self.r)?;

        match block_info {
            BlockInfo::Uncompressed(len) => {
                let len = len as usize;

                self.dst
                    .extend_from_slice(&self.r.get_ref()[self.r.position() as usize..][..len]);

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
                self.dst.resize(self.dst_end + block_size, 0);
                let (prev, dst) = self.dst.split_at_mut(self.dst_end);
                debug_assert_eq!(dst.len(), block_size);
                let decomp_size = crate::block::decompress_into_with_dict(&src[..len], dst, prev)
                    .map_err(Error::DecompressionError)?;

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

        debug_assert_eq!(self.dst.len(), self.dst_end);
        Ok(self.dst_end)
    }

    pub fn read_to_end(&mut self) -> io::Result<usize> {
        loop {
            match self.read_block() {
                Ok(0) => return Ok(self.dst.len()),
                Ok(_) => continue,
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
