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
    fn read_len(r: &mut impl Read) -> io::Result<u32> {
        let mut data = [0u8; 4];
        r.read_exact(&mut data)?;
        Ok(u32::from_le_bytes(data))
    }

    pub(crate) fn read(r: &mut impl Read) -> Result<Self, Error> {
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;

        match magic {
            BLOCK_MAGIC_COMPRESSED => {
                // A compressed block header consists of the octets
                // 0x62, 0x76, 0x34, and 0x31, followed by:

                // the size in bytes of the decoded (plaintext) data
                let decoded_len = Self::read_len(r)?;
                // the size (in bytes) of the encoded data stored
                let encoded_len = Self::read_len(r)?;
                // both size fields as (possibly unaligned) 32-bit little-endian values

                Ok(BlockInfo::Compressed(encoded_len, decoded_len))
            }
            BLOCK_MAGIC_UNCOMPRESSED => {
                // An uncompressed block header consists of the octets
                // 0x62, 0x76, 0x34, and 0x2d, followed by:

                // the size in bytes of the decoded (plaintext) data
                let decoded_len = Self::read_len(r)?;
                // the size (in bytes) of the encoded data stored
                let encoded_len = Self::read_len(r)?;

                if decoded_len != encoded_len {
                    return Err(Error::BlockTooBig);
                }

                Ok(BlockInfo::Uncompressed(decoded_len))
            }
            BLOCK_MAGIC_END => Ok(BlockInfo::EndMark),
            _ => Err(Error::WrongMagicNumber),
        }
    }
}
