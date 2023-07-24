use std::{
    fmt,
    io::{self, BufRead},
};

use super::header::BlockInfo;
use super::Error;
use crate::sink::vec_sink_for_decompression;

pub struct FrameDecoder<R: io::Read> {
    /// The underlying reader.
    r: R,
    /// Total length of decompressed output for the current frame.
    content_len: u64,
    /// The compressed bytes buffer, taken from the underlying reader.
    src: Vec<u8>,
    /// The decompressed bytes buffer. Bytes are decompressed from src to dst
    /// before being passed back to the caller.
    dst: Vec<u8>,
    /// Index into dst: starting point of bytes not yet read by caller.
    dst_start: usize,
    /// Index into dst: ending point of bytes not yet read by caller.
    dst_end: usize,
}

impl<R: io::Read> FrameDecoder<R> {
    /// Creates a new Decoder for the specified reader.
    pub fn new(rdr: R) -> FrameDecoder<R> {
        FrameDecoder {
            r: rdr,
            src: Vec::new(),
            dst: Vec::new(),
            dst_start: 0,
            dst_end: 0,
            content_len: 0,
        }
    }

    /// Gets a reference to the underlying reader in this decoder.
    pub fn get_ref(&self) -> &R {
        &self.r
    }

    /// Gets a mutable reference to the underlying reader in this decoder.
    ///
    /// Note that mutation of the stream may result in surprising results if
    /// this decoder is continued to be used.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.r
    }

    /// Consumes the FrameDecoder and returns the underlying reader.
    pub fn into_inner(self) -> R {
        self.r
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

                self.dst_end += len;
                self.content_len += len as u64;
            }
            BlockInfo::Compressed(len, block_size) => {
                if len > block_size {
                    return Err(Error::BlockTooBig.into());
                }
                let len = len as usize;
                let block_size = block_size as usize;

                // TODO: Attempt to avoid initialization of read buffer when
                // https://github.com/rust-lang/rust/issues/42788 stabilizes
                self.r
                    .read_exact(vec_resize_and_get_mut(&mut self.src, 0, len))?;

                // Independent blocks OR linked blocks with only prefix data
                let decomp_size = crate::block::decompress::decompress_internal::<false, _>(
                    &self.src[..len],
                    &mut vec_sink_for_decompression(
                        &mut self.dst,
                        0,
                        self.dst_start,
                        self.dst_start + block_size,
                    ),
                    b"",
                )
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
                self.content_len += decomp_size as u64;
            }

            BlockInfo::EndMark => {
                // if let Some(expected) = frame_info.content_size {
                //     if self.content_len != expected {
                //         return Err(Error::ContentLengthError {
                //             expected,
                //             actual: self.content_len,
                //         }
                //         .into());
                //     }
                // }
                return Ok(0);
            }
        }

        Ok(self.dst_end - self.dst_start)
    }

    fn read_more(&mut self) -> io::Result<usize> {
        self.read_block()
    }
}

impl<R: io::Read> io::Read for FrameDecoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            // Fill read buffer if there's uncompressed data left
            if self.dst_start < self.dst_end {
                let read_len = std::cmp::min(self.dst_end - self.dst_start, buf.len());
                let dst_read_end = self.dst_start + read_len;
                buf[..read_len].copy_from_slice(&self.dst[self.dst_start..dst_read_end]);
                self.dst_start = dst_read_end;
                return Ok(read_len);
            }
            if self.read_more()? == 0 {
                return Ok(0);
            }
        }
    }

    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        let mut written = 0;
        loop {
            match self.fill_buf() {
                Ok(b) if b.is_empty() => return Ok(written),
                Ok(b) => {
                    let s = std::str::from_utf8(b).map_err(|_| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "stream did not contain valid UTF-8",
                        )
                    })?;
                    buf.push_str(s);
                    let len = s.len();
                    self.consume(len);
                    written += len;
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let mut written = 0;
        loop {
            match self.fill_buf() {
                Ok(b) if b.is_empty() => return Ok(written),
                Ok(b) => {
                    buf.extend_from_slice(b);
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

impl<R: io::Read> io::BufRead for FrameDecoder<R> {
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
}

impl<R: fmt::Debug + io::Read> fmt::Debug for FrameDecoder<R> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("FrameDecoder")
            .field("r", &self.r)
            .field("content_len", &self.content_len)
            .field("src", &"[...]")
            .field("dst", &"[...]")
            .field("dst_start", &self.dst_start)
            .field("dst_end", &self.dst_end)
            .finish()
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
