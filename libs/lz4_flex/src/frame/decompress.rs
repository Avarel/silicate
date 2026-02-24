use std::{
    fmt,
    io::{self, BufRead, Cursor, Read},
};

use super::header::BlockInfo;
use super::Error;
use crate::sink::vec_sink_for_decompression;

pub struct FrameDecoder<'a> {
    /// The underlying reader.
    src: &'a [u8],
    /// The decompressed bytes buffer. Bytes are decompressed from src to dst
    /// before being passed back to the caller.
    dst: &'a mut Vec<u8>,
}

impl<'a> FrameDecoder<'a> {
    /// Creates a new Decoder for the specified reader.
    pub fn new(src: &'a [u8], dst: &'a mut Vec<u8>) -> FrameDecoder<'a> {
        FrameDecoder { src, dst }
    }

    fn read_block(&mut self) -> io::Result<bool> {
        // Read and decompress block
        let block_info = BlockInfo::read(&self.src)?;
        self.src = &self.src[block_info.encoding_bytes()..];

        match block_info {
            BlockInfo::Uncompressed(len) => {
                let len = len as usize;

                let Some((src, rest)) = self.src.split_at_checked(len) else {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Unexpected end of file",
                    ));
                };
                self.src = rest;

                self.dst.extend_from_slice(src);
            }
            BlockInfo::Compressed(len, block_size) => {
                let len = len as usize;
                let block_size = block_size as usize;

                if len > block_size {
                    return Err(Error::BlockTooBig.into());
                }

                let Some((src, rest)) = self.src.split_at_checked(len) else {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Unexpected end of file",
                    ));
                };
                self.src = rest;

                // Independent blocks OR linked blocks with only prefix data
                let dst_end = self.dst.len();
                self.dst.resize(dst_end + block_size, 0);
                // Safety: We just resized the vector to dst_end + block_size
                let (prev, dst) = unsafe { self.dst.split_at_mut(dst_end) };
                debug_assert_eq!(dst.len(), block_size);
                let decomp_size = crate::block::decompress_into_with_dict(src, dst, prev)
                    .map_err(Error::DecompressionError)?;

                if decomp_size != block_size {
                    return Err(Error::ContentLengthError {
                        expected: block_size as u64,
                        actual: decomp_size as u64,
                    }
                    .into());
                }

                debug_assert_eq!(block_size, decomp_size);
            }

            BlockInfo::EndMark => {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub fn read_to_end(&mut self) -> io::Result<usize> {
        loop {
            match self.read_block() {
                Ok(false) => return Ok(self.dst.len()),
                Ok(true) => continue,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }
}
