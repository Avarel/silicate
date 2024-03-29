mod ir;

use self::ir::{IRData, SilicaIRHierarchy, SilicaIRLayer};
use crate::compositor::{dev::GpuHandle, tex::GpuTexture};
use crate::ns_archive::{NsArchiveError, NsKeyedArchive, Size, WrappedArray};
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendingMode {
    Normal = 0,
    Multiply = 1,
    Screen = 2,
    Add = 3,
    Lighten = 4,
    Exclusion = 5,
    Difference = 6,
    Subtract = 7,
    LinearBurn = 8,
    ColorDodge = 9,
    ColorBurn = 10,
    Overlay = 11,
    HardLight = 12,
    Color = 13,
    Luminosity = 14,
    Hue = 15,
    Saturation = 16,
    SoftLight = 17,
    Darken = 19,
    HardMix = 20,
    VividLight = 21,
    LinearLight = 22,
    PinLight = 23,
    LighterColor = 24,
    DarkerColor = 25,
    Divide = 26,
}

impl std::fmt::Display for BlendingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl BlendingMode {
    pub fn all() -> &'static [BlendingMode] {
        use BlendingMode::*;
        &[
            Normal,
            Multiply,
            Screen,
            Add,
            Lighten,
            Exclusion,
            Difference,
            Subtract,
            LinearBurn,
            ColorDodge,
            ColorBurn,
            Overlay,
            HardLight,
            Color,
            Luminosity,
            Hue,
            Saturation,
            SoftLight,
            Darken,
            HardMix,
            VividLight,
            LinearLight,
            PinLight,
            LighterColor,
            DarkerColor,
            Divide,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Multiply => "Multiply",
            Self::Screen => "Screen",
            Self::Add => "Add",
            Self::Lighten => "Lighten",
            Self::Exclusion => "Exclusion",
            Self::Difference => "Difference",
            Self::Subtract => "Subtract",
            Self::LinearBurn => "Linear Burn",
            Self::ColorDodge => "Color Dodge",
            Self::ColorBurn => "Color Burn",
            Self::Overlay => "Overlay",
            Self::HardLight => "Hard Light",
            Self::Color => "Color",
            Self::Luminosity => "Luminosity",
            Self::Hue => "Hue",
            Self::Saturation => "Saturation",
            Self::SoftLight => "Soft Light",
            Self::Darken => "Darken",
            Self::HardMix => "Hard Mix",
            Self::VividLight => "Vivid Light",
            Self::LinearLight => "Linear Light",
            Self::PinLight => "Pin Light",
            Self::LighterColor => "Lighter Color",
            Self::DarkerColor => "Darker Color",
            Self::Divide => "Divide",
        }
    }

    pub fn from_u32(blend: u32) -> Result<Self, SilicaError> {
        Ok(match blend {
            0 => Self::Normal,
            1 => Self::Multiply,
            2 => Self::Screen,
            3 => Self::Add,
            4 => Self::Lighten,
            5 => Self::Exclusion,
            6 => Self::Difference,
            7 => Self::Subtract,
            8 => Self::LinearBurn,
            9 => Self::ColorDodge,
            10 => Self::ColorBurn,
            11 => Self::Overlay,
            12 => Self::HardLight,
            13 => Self::Color,
            14 => Self::Luminosity,
            15 => Self::Hue,
            16 => Self::Saturation,
            17 => Self::SoftLight,
            19 => Self::Darken,
            20 => Self::HardMix,
            21 => Self::VividLight,
            22 => Self::LinearLight,
            23 => Self::PinLight,
            24 => Self::LighterColor,
            25 => Self::DarkerColor,
            26 => Self::Divide,
            _ => Err(SilicaError::InvalidValue)?,
        })
    }

    pub fn to_u32(self) -> u32 {
        self as u32
    }
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
    pub tile_size: u32,
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
