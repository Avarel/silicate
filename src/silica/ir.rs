use std::io::Read;
use std::sync::atomic::AtomicU32;

use super::{SilicaError, SilicaGroup, SilicaHierarchy, SilicaLayer, TilingData, ZipArchiveMmap};
use crate::compositor::{dev::GpuHandle, tex::GpuTexture};
use crate::ns_archive::{NsArchiveError, NsClass, Size, WrappedArray};
use crate::ns_archive::{NsDecode, NsKeyedArchive};
use crate::silica::BlendingMode;
use image::{Pixel, Rgba};
use minilzo_rs::LZO;
use once_cell::sync::OnceCell;
use plist::{Dictionary, Value};
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use regex::Regex;

pub(super) enum SilicaIRHierarchy<'a> {
    Layer(SilicaIRLayer<'a>),
    Group(SilicaIRGroup<'a>),
}

pub(super) struct SilicaIRLayer<'a> {
    nka: &'a NsKeyedArchive,
    coder: &'a Dictionary,
}

#[derive(Clone, Copy)]
pub(super) struct IRData<'a> {
    pub(super) tile: &'a TilingData,
    pub(super) archive: &'a ZipArchiveMmap<'a>,
    pub(super) size: Size<u32>,
    pub(super) file_names: &'a [&'a str],
    pub(super) render: &'a GpuHandle,
    pub(super) gpu_textures: &'a GpuTexture,
    pub(super) counter: &'a AtomicU32,
}

impl<'a> NsDecode<'a> for SilicaIRLayer<'a> {
    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        Ok(Self {
            nka,
            coder: <&'a Dictionary>::decode(nka, key, val)?,
        })
    }
}

// struct AppleLz4Decoder<'a> {
//     inner: std::io::Cursor<&'a [u8]>
// }

// impl<'a> AppleLz4Decoder<'a> {
//     fn new(inner: std::io::Cursor<&'a [u8]>) -> Self {
//         Self { inner }
//     }

//     fn decode_block(&mut self) -> Result<usize, std::io::Error> {
//         let mut magic = [0u8; 4];
//         self.inner.read_exact(&mut magic)?;
//         if magic != [0x62, 0x76, 0x34, 0x31] {
//             return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid magic number"));
//         }       

//         let mut header = [0u8; 4];
//         self.inner.read_exact(&mut header)?;

//         let size = u32::from_le_bytes(header) as usize;

//         let start = self.inner.position();

//         loop {
//             match self.inner.get_ref()[self.inner.position() as usize..] {
//                 [0x62, 0x76, 0x34, 0x24, ..] => {
//                     let end = self.inner.position();
//                     // lz4_flex::decompress(input, uncompressed_size)
//                 }
//                 _ => {
//                     self.inner.read(&mut [0]);
//                 }
//             }
//         }
//         unimplemented!()

//         // let mut decompressed = vec![0u8; buf.len()];
//         // let mut decompressed_size = decompressed.len();
//         // // lz4::block::decompress(&compressed, &mut decompressed, &mut decompressed_size)?;

//         // buf.copy_from_slice(&decompressed[..decompressed_size]);
//         // Ok(decompressed_size)
//     }
// }

impl SilicaIRLayer<'_> {
    pub(super) fn load(self, meta: &IRData<'_>) -> Result<SilicaLayer, SilicaError> {
        let nka = self.nka;
        let coder = self.coder;
        let uuid = nka.fetch::<String>(coder, "UUID")?;

        static INSTANCE: OnceCell<Regex> = OnceCell::new();
        let index_regex = INSTANCE.get_or_init(|| Regex::new("(\\d+)~(\\d+)").unwrap());

        static LZO_INSTANCE: OnceCell<LZO> = OnceCell::new();
        let lzo = LZO_INSTANCE.get_or_init(|| minilzo_rs::LZO::init().unwrap());

        let image = meta
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        meta.file_names
            .into_par_iter()
            .filter(|path| path.starts_with(&uuid))
            .map(|path| -> Result<(), SilicaError> {
                let mut archive = meta.archive.clone();

                let chunk_str = &path[uuid.len()..path.find('.').unwrap_or(path.len())];
                let captures = index_regex.captures(&chunk_str).unwrap();
                let col = u32::from_str_radix(&captures[1], 10).unwrap();
                let row = u32::from_str_radix(&captures[2], 10).unwrap();

                let tile = meta.tile.tile_size(col, row);

                // impossible
                let mut chunk = archive.by_name(path).expect("path not inside zip");

                let mut buf = Vec::new();
                chunk.read_to_end(&mut buf)?;

                // RGBA = 4 channels of 8 bits each, lzo decompressed to lzo data
                let data_len = tile.width * tile.height * usize::from(Rgba::<u8>::CHANNEL_COUNT);
                let dst = if path.ends_with(".lz4") {
                    // println!("{:x?} {:x?} {}", &buf[0..20],  &buf[buf.len() - 20..], data_len);
                    // let mut decoder = lz4_flex::frame::FrameDecoder::new(std::io::Cursor::new(&buf[4..buf.len() - 4]));
                    // let mut dst = Vec::with_capacity(data_len);
                    // decoder.read_to_end(&mut dst)?;
                    // dst
                    // lz4_flex::decompress(&buf[8..buf.len() - 4], data_len)?
                    
                    // todo!("lz4 decompression not implemented yet")
                    // lz4::block::decompress(&buf[4..buf.len() - 4], None).unwrap()
                    return Err(SilicaError::Lz4Unsupported)
                } else {
                    lzo.decompress_safe(buf.as_slice(), data_len)?
                };

                meta.gpu_textures.replace(
                    &meta.render,
                    col * meta.tile.size,
                    row * meta.tile.size,
                    tile.width as u32,
                    tile.height as u32,
                    image as u32,
                    &dst,
                );
                Ok(())
            })
            .collect::<Result<(), _>>()?;

        Ok(SilicaLayer {
            blend: BlendingMode::from_u32(
                nka.fetch::<Option<u32>>(coder, "extendedBlend")
                    .transpose()
                    .unwrap_or_else(|| nka.fetch::<u32>(coder, "blend"))?,
            )?,
            clipped: nka.fetch::<bool>(coder, "clipped")?,
            hidden: nka.fetch::<bool>(coder, "hidden")?,
            mask: None,
            name: nka.fetch::<Option<String>>(coder, "name")?,
            opacity: nka.fetch::<f32>(coder, "opacity")?,
            size: meta.size,
            uuid,
            version: nka.fetch::<u64>(coder, "version")?,
            image,
        })
    }
}

pub(super) struct SilicaIRGroup<'a> {
    nka: &'a NsKeyedArchive,
    coder: &'a Dictionary,
    children: Vec<SilicaIRHierarchy<'a>>,
}

impl<'a> NsDecode<'a> for SilicaIRGroup<'a> {
    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        let coder = <&'a Dictionary>::decode(nka, key, val)?;
        Ok(Self {
            nka,
            coder,
            children: nka
                .fetch::<WrappedArray<SilicaIRHierarchy<'a>>>(coder, "children")?
                .objects,
        })
    }
}

impl<'a> NsDecode<'a> for SilicaIRHierarchy<'a> {
    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        let coder = <&'a Dictionary>::decode(nka, key, val)?;
        let class = nka.fetch::<NsClass>(coder, "$class")?;

        match class.class_name.as_str() {
            "SilicaGroup" => Ok(SilicaIRGroup::<'a>::decode(nka, key, val).map(Self::Group)?),
            "SilicaLayer" => Ok(SilicaIRLayer::<'a>::decode(nka, key, val).map(Self::Layer)?),
            _ => Err(NsArchiveError::TypeMismatch("$class".to_string())),
        }
    }
}

impl<'a> SilicaIRGroup<'a> {
    pub(super) fn count_layer(&self) -> u32 {
        self.children.iter().map(|ir| ir.count_layer()).sum::<u32>()
    }

    fn load(self, meta: &'a IRData<'a>) -> Result<SilicaGroup, SilicaError> {
        let nka = self.nka;
        let coder = self.coder;
        Ok(SilicaGroup {
            hidden: nka.fetch::<bool>(coder, "isHidden")?,
            name: nka.fetch::<Option<String>>(coder, "name")?,
            children: self
                .children
                .into_par_iter()
                .map(|ir| ir.load(meta))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl<'a> SilicaIRHierarchy<'a> {
    pub(super) fn count_layer(&self) -> u32 {
        match self {
            SilicaIRHierarchy::Layer(_) => 1,
            SilicaIRHierarchy::Group(group) => group.count_layer(),
        }
    }

    pub(crate) fn load(self, meta: &'a IRData<'a>) -> Result<SilicaHierarchy, SilicaError> {
        Ok(match self {
            SilicaIRHierarchy::Layer(layer) => SilicaHierarchy::Layer(layer.load(meta)?),
            SilicaIRHierarchy::Group(group) => SilicaHierarchy::Group(group.load(meta)?),
        })
    }
}
