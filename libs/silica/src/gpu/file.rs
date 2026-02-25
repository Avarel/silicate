use crate::ZipArchiveMmap;
use crate::gpu::tiling::AtlasTextureTiling;
use crate::gpu::{hierarchy::SilicaHierarchyGpu, layer::SilicaLayerGpu, tiling::CanvasTiling};
use crate::info::file::ProcreateFile;
use crate::ns_archive::Size;
use crate::{error::SilicaError, ns_archive::NsKeyedArchive};
use rayon::iter::ParallelDrainRange;
use rayon::prelude::ParallelIterator;
use silicate_compositor::dev::GpuDispatch;
use silicate_compositor::tex::GpuTexture;
use std::sync::atomic::AtomicU32;
use std::{
    fs::OpenOptions,
    io::{Cursor, Read},
    path::Path,
};
use zip::read::ZipArchive;

#[derive(Debug, Clone, PartialEq)]
pub struct ProcreateFileGpu {
    pub info: ProcreateFile,
    pub composite: Option<SilicaLayerGpu>,
    pub layers: Vec<SilicaHierarchyGpu>,
}

pub struct ProcreateFileCanvas {
    pub atlas_texture: GpuTexture,
    pub canvas_tiling: CanvasTiling,
}

impl ProcreateFileGpu {
    // Load a Procreate file asynchronously.
    pub fn open(
        path: &Path,
        dispatch: &GpuDispatch,
    ) -> Result<(Self, ProcreateFileCanvas), SilicaError> {
        let file = OpenOptions::new().read(true).write(false).open(path)?;

        let mapping = unsafe { memmap2::Mmap::map(&file)? };
        let mut archive = ZipArchive::new(Cursor::new(&mapping[..]))?;

        let nka: NsKeyedArchive = {
            let mut document = archive.by_name("Document.archive")?;

            let mut buf = Vec::with_capacity(document.size() as usize);
            document.read_to_end(&mut buf)?;

            NsKeyedArchive::from_reader(Cursor::new(buf))?
        };

        Self::from_ns(archive, nka, dispatch)
    }

    fn from_ns(
        archive: ZipArchiveMmap<'_>,
        nka: NsKeyedArchive,
        dispatch: &GpuDispatch,
    ) -> Result<(Self, ProcreateFileCanvas), SilicaError> {
        let info = ProcreateFile::from_ns(&nka)?;

        Self::load(info, archive, dispatch)
    }

    pub fn layer_count(&self, include_groups: bool) -> u32 {
        self.layers
            .iter()
            .map(|layer| layer.layer_count(include_groups))
            .sum()
    }

    fn load_ir_data<'a>(
        archive: &'a ZipArchiveMmap<'_>,
        unloaded_file: &ProcreateFile,
    ) -> Result<crate::gpu::ir::IRData<'a>, SilicaError> {
        let file_names = archive.file_names().collect::<Vec<_>>();
        let chunk_count = file_names.len() as u32;

        let size = unloaded_file.size;
        let tile_size = unloaded_file.tile_size;

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
            atlas: AtlasTextureTiling::compute_atlas_size(chunk_count, tile_size),
        };

        Ok(crate::gpu::ir::IRData {
            archive: &archive,
            file_names,
            tiling,
            chunk_id_counter: AtomicU32::new(1),
            addendum_id_counter: AtomicU32::new(0),
        })
    }

    pub(crate) fn load(
        mut info: ProcreateFile,
        archive: ZipArchiveMmap<'_>,
        dispatch: &GpuDispatch,
    ) -> Result<(ProcreateFileGpu, ProcreateFileCanvas), SilicaError> {
        let irinfo = Self::load_ir_data(&archive, &info)?;

        let canvas_tiling = irinfo.tiling;
        let atlas_texture = GpuTexture::empty_layers(
            &dispatch,
            canvas_tiling.size * canvas_tiling.atlas.cols,
            canvas_tiling.size * canvas_tiling.atlas.rows,
            canvas_tiling.atlas.layers, // Make it an array
            GpuTexture::ATLAS_USAGE,
        );

        Ok((
            ProcreateFileGpu {
                composite: info.composite.take().and_then(|composite| {
                    SilicaLayerGpu::load(composite, dispatch, &atlas_texture, &irinfo).ok()
                }),
                layers: info
                    .layers
                    .par_drain(..)
                    .map(|ir| SilicaHierarchyGpu::load(ir, dispatch, &atlas_texture, &irinfo))
                    .collect::<Result<_, _>>()?,
                info,
            },
            ProcreateFileCanvas {
                atlas_texture,
                canvas_tiling,
            },
        ))
    }
}
