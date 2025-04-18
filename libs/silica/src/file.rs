use crate::data::{Flipped, Orientation};
use crate::ir::ProcreateUnloadedFile;
use crate::layers::{CanvasTiling, SilicaGroup, SilicaLayer};
use crate::{
    error::SilicaError,
    ns_archive::{NsKeyedArchive, Size},
};
use silicate_compositor::dev::GpuDispatch;
use silicate_compositor::tex::GpuTexture;
use std::{
    fs::OpenOptions,
    io::{Cursor, Read},
    path::Path,
};
use zip::read::ZipArchive;

pub(crate) type ZipArchiveMmap<'a> = ZipArchive<Cursor<&'a [u8]>>;

#[derive(Debug)]
pub struct ProcreateFile {
    pub author_name: Option<String>,
    pub background_hidden: bool,
    pub background_color: [f32; 4],
    pub flipped: Flipped,
    pub name: Option<String>,
    pub orientation: Orientation,
    pub stroke_count: usize,
    pub tile_size: u32,
    pub composite: Option<SilicaLayer>,
    pub layers: SilicaGroup,
    pub size: Size<u32>,
    pub layer_count: u32,
}

pub struct ProcreateFileMetadata {
    pub atlas_texture: GpuTexture,
    pub canvas_tiling: CanvasTiling,
}

impl ProcreateFile {
    // Load a Procreate file asynchronously.
    pub fn open(
        path: &Path,
        dispatch: &GpuDispatch,
    ) -> Result<(Self, ProcreateFileMetadata), SilicaError> {
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
    ) -> Result<(Self, ProcreateFileMetadata), SilicaError> {
        ProcreateUnloadedFile::from_ns(&archive, &nka)?.load(dispatch)
    }
}
