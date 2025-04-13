use std::io::Read;
use std::sync::atomic::AtomicU32;

use super::{SilicaError, SilicaGroup, SilicaHierarchy, SilicaLayer, TilingData, ZipArchiveMmap};
use crate::ns_archive::{NsArchiveError, NsClass, Size, WrappedArray};
use crate::ns_archive::{NsDecode, NsKeyedArchive};
use crate::silica::BlendingMode;
use image::{Pixel, Rgba};
use minilzo_rs::LZO;
use once_cell::sync::OnceCell;
use plist::{Dictionary, Value};
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use regex::Regex;
use silicate_compositor::{dev::GpuHandle, tex::GpuTexture};

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

impl SilicaIRLayer<'_> {
    pub(super) fn load(self, meta: &IRData<'_>) -> Result<SilicaLayer, SilicaError> {
        let nka = self.nka;
        let coder = self.coder;
        let uuid = nka.fetch::<String>(coder, "UUID")?;

        static INSTANCE: OnceCell<Regex> = OnceCell::new();
        let index_regex = INSTANCE.get_or_init(|| Regex::new("(\\d+)~(\\d+)").unwrap());

        static LZO_INSTANCE: OnceCell<LZO> = OnceCell::new();

        let image = meta
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        meta.file_names
            .into_par_iter()
            .filter(|path| path.starts_with(&uuid))
            .map(|path| -> Result<(), SilicaError> {
                let mut archive = meta.archive.clone();

                let chunk_str = &path[uuid.len()..path.find('.').unwrap_or(path.len())];
                let captures = index_regex.captures(chunk_str).unwrap();
                let col = captures[1].parse::<u32>().unwrap();
                let row = captures[2].parse::<u32>().unwrap();

                let tile = meta.tile.tile_size(col, row);

                // impossible
                let mut chunk = archive.by_name(path).expect("path not inside zip");

                let mut buf = Vec::new();
                chunk.read_to_end(&mut buf)?;

                // RGBA = 4 channels of 8 bits each, lzo decompressed to lzo data
                let data_len = tile.width as usize
                    * tile.height as usize
                    * usize::from(Rgba::<u8>::CHANNEL_COUNT);
                let dst = if path.ends_with(".lz4") {
                    let mut decoder = lz4_flex::frame::FrameDecoder::new(buf.as_slice());
                    let mut dst = Vec::new();
                    decoder.read_to_end(&mut dst)?;
                    dst
                } else {
                    let lzo = LZO_INSTANCE.get_or_init(|| minilzo_rs::LZO::init().unwrap());
                    lzo.decompress_safe(buf.as_slice(), data_len)?
                };

                meta.gpu_textures.replace(
                    meta.render,
                    (col * meta.tile.size, row * meta.tile.size),
                    (tile.width, tile.height),
                    image,
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
            )
            .ok_or_else(|| SilicaError::InvalidValue)?,
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
