use std::io::Read;

use super::{SilicaError, SilicaGroup, SilicaHierarchy, SilicaLayer, TilingMeta, ZipArchiveMmap};
use crate::gpu::{dev::LogicalDevice, tex::GpuTexture};
use crate::ns_archive::{NsArchiveError, NsClass, Size, WrappedArray};
use crate::ns_archive::{NsDecode, NsKeyedArchive};
use crate::silica::BlendingMode;
use image::{Pixel, Rgba};
use minilzo_rs::LZO;
use once_cell::sync::OnceCell;
use plist::{Dictionary, Value};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use regex::Regex;

pub(super) enum SilicaIRHierarchy<'a> {
    Layer(SilicaIRLayer<'a>),
    Group(SilicaIRGroup<'a>),
}

pub(super) struct SilicaIRLayer<'a> {
    nka: &'a NsKeyedArchive,
    coder: &'a Dictionary,
}

impl<'a> NsDecode<'a> for SilicaIRLayer<'a> {
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        Ok(Self {
            nka,
            coder: <&'a Dictionary>::decode(nka, val)?,
        })
    }
}

impl SilicaIRLayer<'_> {
    pub(super) fn load(
        self,
        meta: &TilingMeta,
        archive: &ZipArchiveMmap<'_>,
        file_names: &[&str],
        render: &LogicalDevice,
        gpu_textures: &mut Vec<GpuTexture>,
    ) -> Result<SilicaLayer, SilicaError> {
        let nka = self.nka;
        let coder = self.coder;
        let uuid = nka.decode::<String>(coder, "UUID")?;
        let size = Size {
            width: nka.decode::<u32>(coder, "sizeWidth")?,
            height: nka.decode::<u32>(coder, "sizeHeight")?,
        };

        static INSTANCE: OnceCell<Regex> = OnceCell::new();
        let index_regex = INSTANCE.get_or_init(|| Regex::new("(\\d+)~(\\d+)").unwrap());

        static LZO_INSTANCE: OnceCell<LZO> = OnceCell::new();
        let lzo = LZO_INSTANCE.get_or_init(|| minilzo_rs::LZO::init().unwrap());

        let gpu_texture = GpuTexture::empty(
            &render.device,
            size.width,
            size.height,
            None,
            GpuTexture::layer_usage(),
        );

        file_names
            .par_iter()
            .filter(|path| path.starts_with(&uuid))
            .try_for_each(|path| -> Result<(), SilicaError> {
                let mut archive = archive.clone();

                let chunk_str = &path[uuid.len()..path.find('.').unwrap_or(path.len())];
                let captures = index_regex.captures(&chunk_str).unwrap();
                let col = u32::from_str_radix(&captures[1], 10).unwrap();
                let row = u32::from_str_radix(&captures[2], 10).unwrap();

                let tile_width = (meta.tile_size
                    - if col != meta.columns - 1 {
                        0
                    } else {
                        meta.diff.width
                    }) as usize;
                let tile_height = (meta.tile_size
                    - if row != meta.rows - 1 {
                        0
                    } else {
                        meta.diff.height
                    }) as usize;

                let mut chunk = archive.by_name(path)?;
                let mut buf = Vec::new();
                chunk.read_to_end(&mut buf)?;
                // RGBA = 4 channels of 8 bits each, lzo decompressed to lzo data
                let dst = lzo.decompress_safe(
                    &buf[..],
                    tile_width * tile_height * usize::from(Rgba::<u8>::CHANNEL_COUNT),
                )?;
                gpu_texture.replace(
                    &render.queue,
                    col * meta.tile_size,
                    row * meta.tile_size,
                    tile_width as u32,
                    tile_height as u32,
                    &dst,
                );
                Ok(())
            })?;

        let image = {
            let i = gpu_textures.len();
            gpu_textures.push(gpu_texture);
            i
        };

        Ok(SilicaLayer {
            blend: BlendingMode::from_u32(nka.decode::<u32>(coder, "extendedBlend")?)?,
            clipped: nka.decode::<bool>(coder, "clipped")?,
            hidden: nka.decode::<bool>(coder, "hidden")?,
            mask: None,
            name: nka.decode::<Option<String>>(coder, "name")?,
            opacity: nka.decode::<f32>(coder, "opacity")?,
            size,
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
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
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
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        let coder = <&'a Dictionary>::decode(nka, val)?;
        let class = nka.decode::<NsClass>(coder, "$class")?;

        match class.class_name.as_str() {
            "SilicaGroup" => Ok(SilicaIRGroup::<'a>::decode(nka, val).map(Self::Group)?),
            "SilicaLayer" => Ok(SilicaIRLayer::<'a>::decode(nka, val).map(Self::Layer)?),
            _ => Err(NsArchiveError::TypeMismatch),
        }
    }
}

impl SilicaIRGroup<'_> {
    fn load(
        self,
        meta: &TilingMeta,
        archive: &ZipArchiveMmap<'_>,
        file_names: &[&str],
        render: &LogicalDevice,
        gpu_textures: &mut Vec<GpuTexture>,
    ) -> Result<SilicaGroup, SilicaError> {
        let nka = self.nka;
        let coder = self.coder;
        Ok(SilicaGroup {
            hidden: nka.decode::<bool>(coder, "isHidden")?,
            name: nka.decode::<String>(coder, "name")?,
            children: self
                .children
                // .into_par_iter()
                .into_iter()
                .map(|ir| ir.load(meta, archive, file_names, render, gpu_textures))
                .collect::<Result<_, _>>()?,
        })
    }
}

impl SilicaIRHierarchy<'_> {
    pub(crate) fn load(
        self,
        meta: &TilingMeta,
        archive: &ZipArchiveMmap<'_>,
        file_names: &[&str],
        render: &LogicalDevice,
        gpu_textures: &mut Vec<GpuTexture>,
    ) -> Result<SilicaHierarchy, SilicaError> {
        Ok(match self {
            SilicaIRHierarchy::Layer(layer) => SilicaHierarchy::Layer(layer.load(
                meta,
                archive,
                file_names,
                render,
                gpu_textures,
            )?),
            SilicaIRHierarchy::Group(group) => SilicaHierarchy::Group(group.load(
                meta,
                archive,
                file_names,
                render,
                gpu_textures,
            )?),
        })
    }
}
