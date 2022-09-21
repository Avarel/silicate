use std::io::Read;

use super::{SilicaError, SilicaGroup, SilicaHierarchy, SilicaLayer, TilingData, ZipArchiveMmap};
use crate::compositor::{dev::GpuHandle, tex::GpuTexture};
use crate::ns_archive::{NsArchiveError, NsClass, Size, WrappedArray};
use crate::ns_archive::{NsDecode, NsKeyedArchive};
use crate::silica::BlendingMode;
use image::{Pixel, Rgba};
use minilzo_rs::LZO;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
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
    pub(super) gpu_textures: &'a Mutex<Vec<GpuTexture>>,
}

impl<'a> NsDecode<'a> for SilicaIRLayer<'a> {
    fn decode(nka: &'a NsKeyedArchive, val: &'a Value) -> Result<Self, NsArchiveError> {
        Ok(Self {
            nka,
            coder: <&'a Dictionary>::decode(nka, val)?,
        })
    }
}

impl SilicaIRLayer<'_> {
    pub(super) fn load(self, meta: &IRData<'_>) -> Result<SilicaLayer, SilicaError> {
        let nka = self.nka;
        let coder = self.coder;
        let uuid = nka.decode::<String>(coder, "UUID")?;

        static INSTANCE: OnceCell<Regex> = OnceCell::new();
        let index_regex = INSTANCE.get_or_init(|| Regex::new("(\\d+)~(\\d+)").unwrap());

        static LZO_INSTANCE: OnceCell<LZO> = OnceCell::new();
        let lzo = LZO_INSTANCE.get_or_init(|| minilzo_rs::LZO::init().unwrap());

        let gpu_texture = GpuTexture::empty(
            &meta.render,
            meta.size.width,
            meta.size.height,
            GpuTexture::LAYER_USAGE,
        );

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

                // RGBA = 4 channels of 8 bits each, lzo decompressed to lzo data
                let data_len = tile.width * tile.height * usize::from(Rgba::<u8>::CHANNEL_COUNT);
                let mut buf = Vec::with_capacity(data_len);
                chunk.read_to_end(&mut buf)?;
                let dst = lzo.decompress_safe(
                    &buf[..],
                    tile.width * tile.height * usize::from(Rgba::<u8>::CHANNEL_COUNT),
                )?;
                gpu_texture.replace(
                    &meta.render,
                    col * meta.tile.size,
                    row * meta.tile.size,
                    tile.width as u32,
                    tile.height as u32,
                    &dst,
                );
                Ok(())
            })
            .collect::<Result<(), _>>()?;

        let image = {
            let mut gpu_textures = meta.gpu_textures.lock();
            let i = gpu_textures.len();
            gpu_textures.push(gpu_texture);
            i
        };

        Ok(SilicaLayer {
            blend: BlendingMode::from_u32(
                nka.decode_nullable::<u32>(coder, "extendedBlend")
                    .transpose()
                    .unwrap_or_else(|| nka.decode::<u32>(coder, "blend"))?,
            )?,
            clipped: nka.decode::<bool>(coder, "clipped")?,
            hidden: nka.decode::<bool>(coder, "hidden")?,
            mask: None,
            name: nka.decode_nullable::<String>(coder, "name")?,
            opacity: nka.decode::<f32>(coder, "opacity")?,
            size: meta.size,
            uuid,
            version: nka.decode::<u64>(coder, "version")?,
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
    fn decode(nka: &'a NsKeyedArchive, val: &'a Value) -> Result<Self, NsArchiveError> {
        let coder = <&'a Dictionary>::decode(nka, val)?;
        Ok(Self {
            nka,
            coder,
            children: nka
                .decode::<WrappedArray<SilicaIRHierarchy<'a>>>(coder, "children")?
                .objects,
        })
    }
}

impl<'a> NsDecode<'a> for SilicaIRHierarchy<'a> {
    fn decode(nka: &'a NsKeyedArchive, val: &'a Value) -> Result<Self, NsArchiveError> {
        let coder = <&'a Dictionary>::decode(nka, val)?;
        let class = nka.decode::<NsClass>(coder, "$class")?;

        match class.class_name.as_str() {
            "SilicaGroup" => Ok(SilicaIRGroup::<'a>::decode(nka, val).map(Self::Group)?),
            "SilicaLayer" => Ok(SilicaIRLayer::<'a>::decode(nka, val).map(Self::Layer)?),
            _ => Err(NsArchiveError::TypeMismatch),
        }
    }
}

impl<'a> SilicaIRGroup<'a> {
    fn load(self, meta: &'a IRData<'a>) -> Result<SilicaGroup, SilicaError> {
        let nka = self.nka;
        let coder = self.coder;
        Ok(SilicaGroup {
            hidden: nka.decode::<bool>(coder, "isHidden")?,
            name: nka.decode::<String>(coder, "name")?,
            children: self
                .children
                .into_par_iter()
                .map(|ir| ir.load(meta))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl<'a> SilicaIRHierarchy<'a> {
    pub(crate) fn load(self, meta: &'a IRData<'a>) -> Result<SilicaHierarchy, SilicaError> {
        Ok(match self {
            SilicaIRHierarchy::Layer(layer) => SilicaHierarchy::Layer(layer.load(meta)?),
            SilicaIRHierarchy::Group(group) => SilicaHierarchy::Group(group.load(meta)?),
        })
    }
}
