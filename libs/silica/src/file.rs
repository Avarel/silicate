use crate::info::ProcreateFileInfo;
use crate::layers::{CanvasTiling, SilicaHierarchy, SilicaLayer};
use crate::{error::SilicaError, ns_archive::NsKeyedArchive};
use silicate_compositor::dev::GpuDispatch;
use silicate_compositor::tex::GpuTexture;
use std::{
    fs::OpenOptions,
    io::{Cursor, Read},
    path::Path,
};
use zip::read::ZipArchive;

pub(crate) type ZipArchiveMmap<'a> = ZipArchive<Cursor<&'a [u8]>>;

#[derive(Debug, Clone, PartialEq)]
pub struct ProcreateFile {
    pub info: ProcreateFileInfo,
    pub composite: Option<SilicaLayer>,
    pub layers: Vec<SilicaHierarchy>,
}

pub struct ProcreateFileCanvas {
    pub atlas_texture: GpuTexture,
    pub canvas_tiling: CanvasTiling,
}

impl ProcreateFile {
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
        let (unloaded_file, data) = ProcreateFileInfo::from_ns(&archive, &nka)?;
        unloaded_file.load(data, dispatch)
    }

    pub fn layer_count(&self, include_groups: bool) -> u32 {
        self.layers
            .iter()
            .map(|layer| layer.layer_count(include_groups))
            .sum()
    }
}
