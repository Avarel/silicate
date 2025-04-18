use std::io::Read;
use std::num::NonZeroU32;
use std::sync::OnceLock;
use crate::layers::{SilicaChunk, SilicaHierarchy, SilicaImageData};
use crate::ns_archive::{NsClass, NsDecode};
use crate::ns_archive::{
    NsKeyedArchive, NsObjects, error::NsArchiveError,
};
use crate::{
    error::SilicaError,
    layers::{SilicaLayer, SilicaGroup},
};
use minilzo_rs::LZO;
use plist::{Dictionary, Value};
use rayon::iter::IntoParallelRefIterator;
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use silicate_compositor::blend::BlendingMode;
use silicate_compositor::buffer::BufferDimensions;
use silicate_compositor::dev::GpuDispatch;
use silicate_compositor::tex::GpuTexture;

use super::IRData;

pub(crate) enum SilicaIRHierarchy<'a> {
    Layer(SilicaIRLayer<'a>),
    Group(SilicaIRGroup<'a>),
}

pub(crate) struct SilicaIRLayer<'a> {
    pub(crate) nka: &'a NsKeyedArchive,
    pub(crate) coder: &'a Dictionary,
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
    pub(crate) fn parse_chunk_str(chunk_str: &str) -> Result<(u32, u32), SilicaError> {
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

    pub(super) fn load(
        self,
        dispatch: &GpuDispatch,
        atlas_texture: &GpuTexture,
        meta: &IRData<'_>,
    ) -> Result<SilicaLayer, SilicaError> {
        let nka = self.nka;
        let world = self.coder;
        let uuid = nka.fetch::<String>(world, "UUID")?;

        pub(crate) static LZO_INSTANCE: OnceLock<LZO> = OnceLock::new();

        let chunks = meta
            .file_names
            .par_iter()
            .filter(|path| path.starts_with(&uuid))
            .map(|path| -> Result<SilicaChunk, SilicaError> {
                let mut archive = meta.archive.clone();

                let chunk_str = &path[uuid.len() + 1..path.find('.').unwrap_or(path.len())];
                let (col, row) = Self::parse_chunk_str(chunk_str)?;

                let tile_extent = meta.tiling.tile_extent(col, row);

                // impossible
                let mut chunk = archive.by_name(path).expect("path not inside zip");

                let mut buf = Vec::new();
                chunk.read_to_end(&mut buf)?;

                // RGBA = 4 channels of 8 bits each, lzo decompressed to lzo data
                let data = if path.ends_with(".lz4") {
                    let mut decoder = lz4_flex::frame::FrameDecoder::new(buf.as_slice());
                    let mut dst = Vec::new();
                    decoder.read_to_end(&mut dst)?;
                    dst
                } else {
                    assert!(path.ends_with(".chunk"));
                    let data_len = tile_extent.width as usize
                        * tile_extent.height as usize
                        * usize::from(BufferDimensions::RGBA_CHANNEL_COUNT);
                    let lzo = LZO_INSTANCE.get_or_init(|| minilzo_rs::LZO::init().unwrap());
                    lzo.decompress_safe(buf.as_slice(), data_len)?
                };

                let atlas_index = NonZeroU32::new(
                    meta.chunk_id_counter
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
                )
                .unwrap();

                let origin = meta.tiling.atlas_origin(atlas_index.get());

                atlas_texture.replace_from_bytes(dispatch, &data, origin, tile_extent);
                Ok(SilicaChunk {
                    col,
                    row,
                    atlas_index,
                })
            })
            .collect::<Result<Vec<SilicaChunk>, _>>()?;

        Ok(SilicaLayer {
            blend: BlendingMode::from_u32(
                nka.fetch::<Option<u32>>(world, "extendedBlend")
                    .transpose()
                    .unwrap_or_else(|| nka.fetch::<u32>(world, "blend"))?,
            )
            .ok_or_else(|| SilicaError::InvalidValue)?,
            clipped: nka.fetch::<bool>(world, "clipped")?,
            hidden: nka.fetch::<bool>(world, "hidden")?,
            mask: None,
            name: nka.fetch::<Option<String>>(world, "name")?,
            opacity: nka.fetch::<f32>(world, "opacity")?,
            size: meta.size,
            uuid,
            version: nka.fetch::<u64>(world, "version")?,
            image: SilicaImageData { chunks },
        })
    }
}

pub(crate) struct SilicaIRGroup<'a> {
    pub(crate) nka: &'a NsKeyedArchive,
    pub(crate) coder: &'a Dictionary,
    pub(crate) children: Vec<SilicaIRHierarchy<'a>>,
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
                .fetch::<NsObjects<SilicaIRHierarchy<'a>>>(coder, "children")?
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
    pub(crate) fn count_layer(&self) -> u32 {
        self.children.iter().map(|ir| ir.count_layer()).sum::<u32>()
    }

    pub(crate) fn load(
        self,
        dispatch: &GpuDispatch,
        atlas_texture: &'a GpuTexture,
        meta: &'a IRData<'a>,
    ) -> Result<SilicaGroup, SilicaError> {
        let nka = self.nka;
        let coder = self.coder;
        Ok(SilicaGroup {
            hidden: nka.fetch::<bool>(coder, "isHidden")?,
            name: nka.fetch::<Option<String>>(coder, "name")?,
            children: self
                .children
                .into_par_iter()
                .map(|ir| ir.load(dispatch, atlas_texture, meta))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl<'a> SilicaIRHierarchy<'a> {
    pub(crate) fn count_layer(&self) -> u32 {
        match self {
            SilicaIRHierarchy::Layer(_) => 1,
            SilicaIRHierarchy::Group(group) => group.count_layer(),
        }
    }

    pub(crate) fn load(
        self,
        dispatch: &GpuDispatch,
        atlas_texture: &'a GpuTexture,
        meta: &'a IRData<'a>,
    ) -> Result<SilicaHierarchy, SilicaError> {
        Ok(match self {
            SilicaIRHierarchy::Layer(layer) => {
                SilicaHierarchy::Layer(layer.load(dispatch, atlas_texture, meta)?)
            }
            SilicaIRHierarchy::Group(group) => {
                SilicaHierarchy::Group(group.load(dispatch, atlas_texture, meta)?)
            }
        })
    }
}
