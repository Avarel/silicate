mod ir;

use self::ir::{IRData, SilicaIRHierarchy, SilicaIRLayer};
use crate::ns_archive::{NsArchiveError, NsKeyedArchive, Size, WrappedArray};
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use silicate_compositor::{blend::BlendingMode, dev::GpuHandle, tex::GpuTexture};
use std::fs::OpenOptions;
use std::io::Cursor;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::AtomicU32;
use thiserror::Error;
use zip::read::ZipArchive;

#[derive(Error, Debug)]
pub enum SilicaError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Plist error: {0}")]
    PlistError(#[from] plist::Error),
    #[error("Zip error: {0}")]
    ZipError(#[from] zip::result::ZipError),
    #[error("LZO error: {0}")]
    LzoError(#[from] minilzo_rs::Error),
    #[error("LZ4 error: {0}")]
    Lz4Error(#[from] lz4_flex::block::DecompressError),
    #[error("Ns archive error: {0}")]
    NsArchiveError(#[from] NsArchiveError),
    #[error("Invalid values in file")]
    InvalidValue,
    #[error("Unknown decoding error")]
    #[allow(dead_code)]
    Unknown,
}

#[derive(Debug)]
struct TilingData {
    columns: u32,
    rows: u32,
    diff: Size<u32>,
    size: u32,
}

impl TilingData {
    pub fn tile_size(&self, col: u32, row: u32) -> Size<u32> {
        Size {
            width: if col != self.columns - 1 {
                self.size
            } else {
                self.size - self.diff.width
            },
            height: if row != self.rows - 1 {
                self.size
            } else {
                self.size - self.diff.height
            },
        }
    }
}

#[derive(Debug)]
pub struct Flipped {
    pub horizontally: bool,
    pub vertically: bool,
}

#[derive(Debug)]
pub struct ProcreateFile {
    pub author_name: Option<String>,
    pub background_hidden: bool,
    pub background_color: [f32; 4],
    //     closedCleanlyKey:Bool?
    //     colorProfile:ValkyrieColorProfile?

    // //  public var drawingguide
    //     faceBackgroundHidden:Bool?
    //     1 => BlendingMode::featureSet:Int?
    pub flipped: Flipped,
    pub layers: SilicaGroup,
    //     mask:SilicaLayer?
    pub name: Option<String>,
    pub orientation: u32,
    //     primaryItem:Any?
    // //  skipping a bunch of reference window related stuff here
    //     selectedLayer:Any?
    //     selectedSamplerLayer:SilicaLayer?
    //     SilicaDocumentArchiveDPIKey:Float?
    //     SilicaDocumentArchiveUnitKey:Int?
    //     SilicaDocumentTrackedTimeKey:Float?
    //     SilicaDocumentVideoPurgedKey:Bool?
    //     SilicaDocumentVideoSegmentInfoKey:VideoSegmentInfo? // not finished
    //     size: CGSize?
    //     solo: SilicaLayer?
    pub stroke_count: usize,
    //     videoEnabled: Bool? = true
    //     videoQualityKey: String?
    //     videoResolutionKey: String?
    //     videoDuration: String? = "Calculating..."
    #[allow(dead_code)]
    pub tile_size: u32,
    #[allow(dead_code)]
    pub composite: Option<SilicaLayer>,
    pub size: Size<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SilicaHierarchy {
    Layer(SilicaLayer),
    Group(SilicaGroup),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaGroup {
    pub hidden: bool,
    pub children: Vec<SilicaHierarchy>,
    pub name: Option<String>,
}

impl SilicaGroup {
    #[allow(dead_code)]
    pub const fn empty() -> Self {
        Self {
            hidden: true,
            children: Vec::new(),
            name: None,
        }
    }
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
    pub size: Size<u32>,
    pub uuid: String,
    pub version: u64,
    pub image: u32,
}

type ZipArchiveMmap<'a> = ZipArchive<Cursor<&'a [u8]>>;

impl ProcreateFile {
    // Load a Procreate file asynchronously.
    pub fn open<P: AsRef<Path>>(p: P, dev: &GpuHandle) -> Result<(Self, GpuTexture), SilicaError> {
        let path = p.as_ref();
        let file = OpenOptions::new().read(true).write(false).open(path)?;

        let mapping = unsafe { memmap2::Mmap::map(&file)? };
        let mut archive = ZipArchive::new(Cursor::new(&mapping[..]))?;

        let nka: NsKeyedArchive = {
            let mut document = archive.by_name("Document.archive")?;

            let mut buf = Vec::with_capacity(document.size() as usize);
            document.read_to_end(&mut buf)?;

            NsKeyedArchive::from_reader(Cursor::new(buf))?
        };

        Self::from_ns(archive, nka, dev)
    }

    fn from_ns(
        archive: ZipArchiveMmap<'_>,
        nka: NsKeyedArchive,
        dev: &GpuHandle,
    ) -> Result<(Self, GpuTexture), SilicaError> {
        let root = nka.root()?;

        let size = nka.fetch::<Size<u32>>(root, "size")?;
        let tile_size = nka.fetch::<u32>(root, "tileSize")?;
        let columns = (size.width + tile_size - 1) / tile_size;
        let rows = (size.height + tile_size - 1) / tile_size;

        let tile = TilingData {
            columns,
            rows,
            diff: Size {
                width: columns * tile_size - size.width,
                height: rows * tile_size - size.height,
            },
            size: tile_size,
        };

        let file_names = archive.file_names().collect::<Vec<_>>();

        let ir_hierachy = nka
            .fetch::<WrappedArray<SilicaIRHierarchy>>(root, "unwrappedLayers")?
            .objects;

        let gpu_textures = GpuTexture::empty_layers(
            dev,
            size.width,
            size.height,
            ir_hierachy.iter().map(|ir| ir.count_layer()).sum::<u32>() + 1,
            GpuTexture::LAYER_USAGE,
        );

        let ir_data = IRData {
            tile: &tile,
            archive: &archive,
            size,
            file_names: &file_names,
            render: dev,
            gpu_textures: &gpu_textures,
            counter: &AtomicU32::new(0),
        };

        Ok((
            Self {
                author_name: nka.fetch::<Option<String>>(root, "authorName")?,
                background_hidden: nka.fetch::<bool>(root, "backgroundHidden")?,
                stroke_count: nka.fetch::<usize>(root, "strokeCount")?,
                background_color: <[f32; 4]>::try_from(
                    nka.fetch::<&[u8]>(root, "backgroundColor")?
                        .chunks_exact(4)
                        .map(|bytes| {
                            <[u8; 4]>::try_from(bytes)
                                .map(f32::from_le_bytes)
                                .map_err(|_| {
                                    NsArchiveError::TypeMismatch("backgroundColor".to_string())
                                })
                        })
                        .collect::<Result<Vec<f32>, _>>()?,
                )
                .unwrap(),
                name: nka.fetch::<Option<String>>(root, "name")?,
                orientation: nka.fetch::<u32>(root, "orientation")?,
                flipped: Flipped {
                    horizontally: nka.fetch::<bool>(root, "flippedHorizontally")?,
                    vertically: nka.fetch::<bool>(root, "flippedVertically")?,
                },
                tile_size,
                size,
                composite: nka
                    .fetch::<SilicaIRLayer>(root, "composite")?
                    .load(&ir_data)
                    .ok(),
                layers: SilicaGroup {
                    hidden: false,
                    name: Some(String::from("Root Layer")),
                    children: ir_hierachy
                        .into_par_iter()
                        .map(|ir| ir.load(&ir_data))
                        .collect::<Result<_, _>>()?,
                },
            },
            gpu_textures,
        ))
    }
}
