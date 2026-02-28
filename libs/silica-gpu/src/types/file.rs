use crate::ZipArchiveMmap;
use crate::error::SilicaError;
use crate::tiling::{AtlasTextureTiling, CanvasTiling};
use crate::types::{hierarchy::SilicaHierarchy, layer::SilicaLayer};
use rayon::iter::ParallelDrainRange;
use rayon::prelude::ParallelIterator;
use silica::ns_archive::NsArchive;
use silica::ns_archive::Size;
use std::sync::atomic::AtomicU32;
use std::{
    fs::OpenOptions,
    io::{Cursor, Read},
    path::Path,
};
use zip::read::ZipArchive;

#[derive(Debug, Clone, PartialEq)]
pub struct ProcreateFile {
    info: silica::ProcreateFile,
    pub composite: Option<SilicaLayer>,
    pub layers: Vec<SilicaHierarchy>,
}

impl std::ops::Deref for ProcreateFile {
    type Target = silica::ProcreateFile;

    fn deref(&self) -> &Self::Target {
        &self.info
    }
}

impl std::ops::DerefMut for ProcreateFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.info
    }
}

pub struct ProcreateFileAtlas {
    pub atlas_texture: wgpu::Texture,
    pub canvas_tiling: CanvasTiling,
}

impl ProcreateFile {
    // Load a Procreate file asynchronously.
    pub fn open(
        path: &Path,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(Self, ProcreateFileAtlas), SilicaError> {
        let file = OpenOptions::new().read(true).write(false).open(path)?;

        let mapping = unsafe { memmap2::Mmap::map(&file)? };
        let mut archive = ZipArchive::new(Cursor::new(&mapping[..]))?;

        let nka: NsArchive = {
            let mut document = archive.by_name("Document.archive")?;

            let mut buf = Vec::with_capacity(document.size() as usize);
            document.read_to_end(&mut buf)?;

            NsArchive::from_reader(Cursor::new(buf))?
        };

        Self::from_ns(archive, nka, device, queue)
    }

    fn from_ns(
        archive: ZipArchiveMmap<'_>,
        nka: NsArchive,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(Self, ProcreateFileAtlas), SilicaError> {
        let info = silica::ProcreateFile::from_ns(&nka)?;

        Self::load(info, archive, device, queue)
    }

    pub fn layer_count(&self, include_groups: bool) -> u32 {
        self.layers
            .iter()
            .map(|layer| layer.layer_count(include_groups))
            .sum()
    }

    pub(crate) fn load(
        mut info: silica::ProcreateFile,
        archive: ZipArchiveMmap<'_>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(ProcreateFile, ProcreateFileAtlas), SilicaError> {
        let file_names = archive.file_names().collect::<Vec<_>>();
        let chunk_count = file_names.len() as u32;

        let size = info.size;
        let tile_size = info.tile_size;

        let (cols, rows) = (
            size.width.div_ceil(tile_size),
            size.height.div_ceil(tile_size),
        );

        let tiling = CanvasTiling {
            cols,
            rows,
            diff: Size {
                width: cols * tile_size - size.width,
                height: rows * tile_size - size.height,
            },
            size: tile_size,
            atlas: AtlasTextureTiling::compute_atlas_size(chunk_count, tile_size, &device.limits()),
        };

        let atlas_texture = Self::empty_layers(
            device,
            tiling.size * tiling.atlas.cols,
            tiling.size * tiling.atlas.rows,
            tiling.atlas.layers, // Make it an array
        );

        let params = crate::params::LoadParams {
            queue,
            archive: &archive,
            atlas_texture: &atlas_texture,
            file_names,
            tiling,
            chunk_id_counter: AtomicU32::new(1),
            addendum_id_counter: AtomicU32::new(0),
        };

        Ok((
            ProcreateFile {
                composite: info
                    .composite
                    .take()
                    .and_then(|composite| SilicaLayer::load(composite, &params, false).ok()),
                layers: info
                    .layers
                    .par_drain(..)
                    .map(|ir| SilicaHierarchy::load(ir, &params))
                    .collect::<Result<_, _>>()?,
                info,
            },
            ProcreateFileAtlas {
                atlas_texture,
                canvas_tiling: tiling,
            },
        ))
    }

    pub fn empty_layers(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        layers: u32,
    ) -> wgpu::Texture {
        const TEX_DIM: wgpu::TextureDimension = wgpu::TextureDimension::D2;
        const TEX_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

        device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: layers,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TEX_DIM,
            format: TEX_FORMAT,
            view_formats: &[
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureFormat::Rgba8UnormSrgb,
            ],
            usage: wgpu::TextureUsages::COPY_DST
                .union(wgpu::TextureUsages::COPY_SRC)
                .union(wgpu::TextureUsages::TEXTURE_BINDING),
            label: None,
        })
    }
}
