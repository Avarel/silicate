use crate::gpu::ir::IRData;
use crate::gpu::layers::{Addendum, SilicaChunk, SilicaHierarchyGpu, SilicaImageData};
use crate::ns_archive::{NsClass, NsDecode};
use crate::ns_archive::{NsKeyedArchive, NsObjects, error::NsArchiveError};
use crate::{
    error::SilicaError,
    gpu::layers::{SilicaGroupGpu, SilicaLayerGpu},
};
use minilzo_rs::LZO;
use plist::{Dictionary, Value};
use rayon::iter::{IntoParallelRefIterator, ParallelDrainRange};
use rayon::prelude::ParallelIterator;
use silicate_compositor::blend::BlendingMode;
use silicate_compositor::buffer::BufferDimensions;
use silicate_compositor::dev::GpuDispatch;
use silicate_compositor::tex::GpuTexture;
use std::io::Read;
use std::num::NonZeroU32;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

impl<'a> NsDecode<'a> for BlendingMode {
    fn fetch(
        nka: &'a NsKeyedArchive,
        world: &'a Dictionary,
        key: &'a str,
    ) -> Result<Self, NsArchiveError> {
        assert!(key == "extendedBlend" || key == "blend");

        let val = nka
            .fetch_value_nullable(world, "extendedBlend")
            .transpose()
            .unwrap_or_else(|| nka.fetch_value(world, "blend"))?;
        Self::decode(nka, "extendedBlend", val)
    }

    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        BlendingMode::from_u32(u32::decode(nka, key, val)?)
            .ok_or_else(|| NsArchiveError::TypeMismatch(String::from(key)))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SilicaHierarchy {
    Layer(SilicaLayer),
    Group(SilicaGroup),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaLayer {
    // animationHeldLength:Int?
    pub blend: BlendingMode,
    // bundledImagePath:String?
    // bundledMaskPath:String?
    // bundledVideoPath:String?
    pub clipped: bool,
    // contentsRect:Data?
    // contentsRectValid:Bool?
    // document:SilicaDocument?
    // extendedBlend:Int?
    pub hidden: bool,
    // locked:Bool?
    pub mask: Option<usize>,
    pub name: Option<String>,
    pub opacity: f32,
    // perspectiveAssisted:Bool?
    // preserve:Bool?
    // private:Bool?
    // text:ValkyrieText?
    // textPDF:Data?
    // transform:Data?
    // type:Int?
    pub uuid: String,
    pub version: u64,
}

impl<'a> NsDecode<'a> for SilicaLayer {
    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        let world = <&'a Dictionary>::decode(nka, key, val)?;
        let uuid = nka.fetch::<String>(world, "UUID")?;

        Ok(Self {
            blend: nka
                .fetch::<BlendingMode>(world, "extendedBlend")
                .or_else(|_| nka.fetch::<BlendingMode>(world, "blend"))?,
            clipped: nka.fetch::<bool>(world, "clipped")?,
            hidden: nka.fetch::<bool>(world, "hidden")?,
            mask: None,
            name: nka.fetch::<Option<String>>(world, "name")?,
            opacity: nka.fetch::<f32>(world, "opacity")?,
            uuid,
            version: nka.fetch::<u64>(world, "version")?,
        })
    }
}

impl SilicaLayer {
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

    pub(super) fn load(
        self,
        dispatch: &GpuDispatch,
        atlas_texture: &GpuTexture,
        meta: &IRData<'_>,
    ) -> Result<SilicaLayerGpu, SilicaError> {
        static LZO_INSTANCE: OnceLock<LZO> = OnceLock::new();

        let chunks = meta
            .file_names
            .par_iter()
            .filter(|path| path.starts_with(self.uuid.as_str()))
            .map(|path| -> Result<SilicaChunk, SilicaError> {
                let mut archive = meta.archive.clone();

                let chunk_str = &path[self.uuid.len() + 1..path.find('.').unwrap_or(path.len())];
                let (col, row) = Self::parse_chunk_str(chunk_str)?;

                let tile_extent = meta.tiling.tile_extent(col, row);

                // impossible
                let mut chunk = archive.by_name(path).expect("path not inside zip");

                let mut buf = Vec::with_capacity(chunk.size() as usize);
                chunk.read_to_end(&mut buf)?;

                let data_len = tile_extent.width as usize
                    * tile_extent.height as usize
                    * usize::from(BufferDimensions::RGBA_CHANNEL_COUNT);

                // RGBA = 4 channels of 8 bits each, lzo decompressed to lzo data
                let data = if path.ends_with(".lz4") {
                    let mut dst = Vec::with_capacity(data_len);
                    super::lz4::decompress(buf.as_slice(), &mut dst)?;
                    dst
                } else {
                    assert!(path.ends_with(".chunk"));
                    let lzo = LZO_INSTANCE.get_or_init(|| minilzo_rs::LZO::init().unwrap());
                    lzo.decompress_safe(buf.as_slice(), data_len)?
                };

                let atlas_index = NonZeroU32::new(
                    meta.chunk_id_counter
                        .fetch_add(1, std::sync::atomic::Ordering::AcqRel),
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

        Ok(SilicaLayerGpu {
            info: self,
            image: SilicaImageData { chunks },
            addendum: Addendum {
                id: meta.addendum_id_counter.fetch_add(1, Ordering::AcqRel),
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaGroup {
    pub name: Option<String>,
    pub hidden: bool,
    pub(crate) children: Vec<SilicaHierarchy>,
}

impl<'a> NsDecode<'a> for SilicaGroup {
    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        let coder = <&'a Dictionary>::decode(nka, key, val)?;
        Ok(Self {
            hidden: nka.fetch::<bool>(coder, "isHidden")?,
            name: nka.fetch::<Option<String>>(coder, "name")?,
            children: nka
                .fetch::<NsObjects<SilicaHierarchy>>(coder, "children")?
                .objects,
        })
    }
}

impl<'a> NsDecode<'a> for SilicaHierarchy {
    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        let coder = <&'a Dictionary>::decode(nka, key, val)?;
        let class = nka.fetch::<NsClass>(coder, "$class")?;

        match class.class_name.as_str() {
            "SilicaGroup" => Ok(SilicaGroup::decode(nka, key, val).map(Self::Group)?),
            "SilicaLayer" => Ok(SilicaLayer::decode(nka, key, val).map(Self::Layer)?),
            _ => Err(NsArchiveError::TypeMismatch("$class".to_string())),
        }
    }
}

impl<'a> SilicaGroup {
    pub(crate) fn load(
        mut self,
        dispatch: &GpuDispatch,
        atlas_texture: &'a GpuTexture,
        meta: &'a IRData<'a>,
    ) -> Result<SilicaGroupGpu, SilicaError> {
        Ok(SilicaGroupGpu {
            children: self
                .children
                .par_drain(..)
                .map(|ir| ir.load(dispatch, atlas_texture, meta))
                .collect::<Result<Vec<_>, _>>()?,
            info: self,
            addendum: Addendum {
                id: meta.addendum_id_counter.fetch_add(1, Ordering::AcqRel),
            },
        })
    }
}

impl<'a> SilicaHierarchy {
    pub(crate) fn load(
        self,
        dispatch: &GpuDispatch,
        atlas_texture: &'a GpuTexture,
        meta: &'a IRData<'a>,
    ) -> Result<SilicaHierarchyGpu, SilicaError> {
        Ok(match self {
            SilicaHierarchy::Layer(layer) => {
                SilicaHierarchyGpu::Layer(layer.load(dispatch, atlas_texture, meta)?)
            }
            SilicaHierarchy::Group(group) => {
                SilicaHierarchyGpu::Group(group.load(dispatch, atlas_texture, meta)?)
            }
        })
    }
}
