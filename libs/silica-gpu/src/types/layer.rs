use crate::{error::SilicaError, params::LoadParams};
#[cfg(not(target_arch = "wasm32"))]
use rayon::{iter::IntoParallelRefIterator, prelude::ParallelIterator};
use std::{io::Read, num::NonZeroU32};

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaChunk {
    pub col: u32,
    pub row: u32,
    pub atlas_index: NonZeroU32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaImageData {
    pub chunks: Vec<SilicaChunk>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaLayer {
    info: silica::SilicaLayer,
    pub image: SilicaImageData,
    pub mask: Option<Box<SilicaLayer>>,
    pub id: u32,
}

impl std::ops::Deref for SilicaLayer {
    type Target = silica::SilicaLayer;

    fn deref(&self) -> &Self::Target {
        &self.info
    }
}

impl std::ops::DerefMut for SilicaLayer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.info
    }
}

impl SilicaLayer {
    const RGBA_CHANNEL_COUNT: usize = 4;

    fn parse_chunk_str(chunk_str: &str) -> Result<(u32, u32), SilicaError> {
        let tilde_index = chunk_str
            .find('~')
            .ok_or_else(|| SilicaError::CorruptedFormat)?;
        let col = chunk_str[..tilde_index]
            .parse::<u32>()
            .map_err(|_| SilicaError::CorruptedFormat)?;
        let row = chunk_str[tilde_index + 1..]
            .parse::<u32>()
            .map_err(|_| SilicaError::CorruptedFormat)?;

        Ok((col, row))
    }

    pub(crate) fn load(
        mut info: silica::SilicaLayer,
        params: &LoadParams<'_>,
        is_mask: bool,
    ) -> Result<SilicaLayer, SilicaError> {
        let chunks = {
            #[cfg(not(target_arch = "wasm32"))]
            let iter = params.file_names.par_iter();
            #[cfg(target_arch = "wasm32")]
            let iter = params.file_names.iter();
            iter
        }
        .filter(|path| path.starts_with(info.uuid.as_str()))
        .map(|path| -> Result<SilicaChunk, SilicaError> {
            let mut archive = params.archive.clone();

            let chunk_str = &path[info.uuid.len() + 1..path.find('.').unwrap_or(path.len())];
            let (col, row) = Self::parse_chunk_str(chunk_str)?;

            let tile_extent = params.tiling.tile_extent(col, row);

            // impossible
            let mut chunk = archive.by_name(path).expect("path not inside zip");

            let mut buf = Vec::with_capacity(chunk.size() as usize);
            chunk.read_to_end(&mut buf)?;

            let data_len =
                tile_extent.width as usize * tile_extent.height as usize * Self::RGBA_CHANNEL_COUNT;

            // Try RGBA first (4 channels), but fall back to grayscale (1 channel) for masks
            let decompress_len = if is_mask {
                tile_extent.width as usize * tile_extent.height as usize
            } else {
                data_len
            };

            let mut data = Vec::with_capacity(decompress_len);

            // RGBA = 4 channels of 8 bits each
            // Masks are grayscale = 1 channel of 8 bits
            let data = if path.ends_with(".lz4") {
                lz4::decompress(buf.as_slice(), &mut data)?;
                data
            } else {
                assert!(path.ends_with(".chunk"));
                data.resize(decompress_len, 0);
                lzokay::decompress::decompress(buf.as_slice(), &mut data)?;
                data
            };

            let data = if is_mask {
                // Expand grayscale mask to RGBA by replicating the single channel into R, G, B and setting A to the same value
                data.into_iter()
                    .flat_map(|v| [v; Self::RGBA_CHANNEL_COUNT])
                    .collect()
            } else {
                data
            };

            assert_eq!(data.len(), data_len);

            let atlas_index = NonZeroU32::new(params.allocate_chunk_id()).unwrap();

            let origin = params.tiling.atlas_origin(atlas_index.get());

            Self::replace_from_bytes(
                params.queue,
                params.atlas_texture,
                &data,
                origin,
                tile_extent,
            );
            Ok(SilicaChunk {
                col,
                row,
                atlas_index,
            })
        })
        .collect::<Result<Vec<SilicaChunk>, _>>()?;

        Ok(SilicaLayer {
            image: SilicaImageData { chunks },
            mask: info
                .mask
                .take()
                .map(|mask| Self::load(*mask, params, true).map(Box::new))
                .transpose()?,
            info,
            id: params.allocate_layer_id(),
        })
    }

    /// Replace a section of the texture with raw RGBA data.
    ///
    /// ### Note
    /// The position `x` and `y` and size `width` and `height` data
    /// should strictly fit within the texture boundaries.
    fn replace_from_bytes(
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        data: &[u8],
        origin: wgpu::Origin3d,
        size: wgpu::Extent3d,
    ) {
        let layers = texture.size().depth_or_array_layers;
        assert!(
            origin.z < layers,
            "index {} must be less than {}",
            origin.z,
            layers
        );
        queue.write_texture(
            // Tells wgpu where to copy the pixel data
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin,
                aspect: wgpu::TextureAspect::All,
            },
            // The actual pixel data
            data,
            // The layout of the texture
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * size.width),
                rows_per_image: Some(size.height),
            },
            size,
        );
    }
}

mod lz4 {
    use std::fmt::Debug;
    use std::io;

    use lz4_flex::frame::Error;

    const BLOCK_MAGIC_COMPRESSED: [u8; 4] = [0x62, 0x76, 0x34, 0x31];
    const BLOCK_MAGIC_UNCOMPRESSED: [u8; 4] = [0x62, 0x76, 0x34, 0x2d];
    const BLOCK_MAGIC_END: [u8; 4] = [0x62, 0x76, 0x34, 0x24];

    #[derive(Debug)]
    pub(crate) enum BlockInfo {
        Compressed(u32, u32),
        Uncompressed(u32),
        EndMark,
    }

    impl BlockInfo {
        fn read_bytes<'b>(r: &mut &'b [u8]) -> io::Result<&'b [u8; 4]> {
            let Some((bytes, rest)) = r.split_first_chunk::<4>() else {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Unexpected end of file",
                ));
            };
            *r = rest;
            Ok(bytes)
        }

        fn read_len(r: &[u8; 4]) -> io::Result<u32> {
            Ok(u32::from_le_bytes(*r))
        }

        pub(crate) fn read(mut r: &[u8]) -> Result<Self, Error> {
            match *Self::read_bytes(&mut r)? {
                BLOCK_MAGIC_COMPRESSED => {
                    // A compressed block header consists of the octets
                    // 0x62, 0x76, 0x34, and 0x31, followed by:

                    // the size in bytes of the decoded (plaintext) data
                    let decoded_len = Self::read_len(Self::read_bytes(&mut r)?)?;
                    // the size (in bytes) of the encoded data stored
                    let encoded_len = Self::read_len(Self::read_bytes(&mut r)?)?;
                    // both size fields as (possibly unaligned) 32-bit little-endian values

                    Ok(BlockInfo::Compressed(encoded_len, decoded_len))
                }
                BLOCK_MAGIC_UNCOMPRESSED => {
                    // An uncompressed block header consists of the octets
                    // 0x62, 0x76, 0x34, and 0x2d, followed by:

                    // the size in bytes of the decoded (plaintext) data
                    let decoded_len = Self::read_len(Self::read_bytes(&mut r)?)?;
                    // the size (in bytes) of the encoded data stored
                    let encoded_len = Self::read_len(Self::read_bytes(&mut r)?)?;

                    if decoded_len != encoded_len {
                        return Err(Error::BlockTooBig);
                    }

                    Ok(BlockInfo::Uncompressed(decoded_len))
                }
                BLOCK_MAGIC_END => Ok(BlockInfo::EndMark),
                _ => Err(Error::WrongMagicNumber),
            }
        }

        pub(crate) fn encoding_bytes(&self) -> usize {
            match self {
                BlockInfo::Compressed(_, _) | BlockInfo::Uncompressed(_) => 12,
                BlockInfo::EndMark => 4,
            }
        }
    }

    struct ChainDecoder<'a> {
        /// The underlying reader.
        src: &'a [u8],
        /// The decompressed bytes buffer. Bytes are decompressed from src to dst
        /// before being passed back to the caller.
        dst: &'a mut Vec<u8>,
    }

    impl<'a> ChainDecoder<'a> {
        /// Creates a new Decoder for the specified reader.
        fn new(src: &'a [u8], dst: &'a mut Vec<u8>) -> ChainDecoder<'a> {
            ChainDecoder { src, dst }
        }

        fn read_raw<'b>(r: &mut &'b [u8], len: usize) -> io::Result<&'b [u8]> {
            let Some((src, rest)) = r.split_at_checked(len) else {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Unexpected end of file",
                ));
            };
            *r = rest;
            Ok(src)
        }

        fn read_block(&mut self) -> io::Result<bool> {
            // Read and decompress block
            let block_info = BlockInfo::read(&self.src)?;
            self.src = &self.src[block_info.encoding_bytes()..];

            match block_info {
                BlockInfo::Uncompressed(len) => {
                    let len = len as usize;

                    let src = Self::read_raw(&mut self.src, len)?;

                    self.dst.extend_from_slice(src);
                }
                BlockInfo::Compressed(len, block_size) => {
                    let len = len as usize;
                    let block_size = block_size as usize;

                    if len > block_size {
                        return Err(Error::BlockTooBig.into());
                    }

                    let src = Self::read_raw(&mut self.src, len)?;

                    // Independent blocks OR linked blocks with only prefix data
                    let dst_end = self.dst.len();
                    self.dst.resize(dst_end + block_size, 0);
                    // Safety: We just resized the vector to dst_end + block_size
                    let (prev, dst) = unsafe { self.dst.split_at_mut_unchecked(dst_end) };
                    debug_assert_eq!(dst.len(), block_size);
                    let decomp_size = lz4_flex::block::decompress_into_with_dict(src, dst, prev)
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

        fn decode(mut self) -> io::Result<()> {
            loop {
                match self.read_block() {
                    Ok(false) => return Ok(()),
                    Ok(true) => continue,
                    Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
        }
    }

    pub fn decompress(src: &[u8], dst: &mut Vec<u8>) -> io::Result<()> {
        ChainDecoder::new(src, dst).decode()
    }
}
