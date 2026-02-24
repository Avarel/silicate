use super::Error;
use std::{fmt::Debug, io, io::Read};

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
        let len = r.len();
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
        let mut data = [0u8; 4];
        data.copy_from_slice(&r[..4]);
        Ok(u32::from_le_bytes(data))
    }

    pub(crate) fn read(mut r: &[u8]) -> Result<Self, Error> {
        let mut magic = [0u8; 4];

        magic.copy_from_slice(Self::read_bytes(&mut r)?);

        match magic {
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
