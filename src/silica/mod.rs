mod ir;

use self::ir::{SilicaIRHierarchy, SilicaIRLayer};
use crate::compositor::{dev::LogicalDevice, tex::GpuTexture};
use crate::ns_archive::{NsArchiveError, NsKeyedArchive, Size, WrappedArray};
use std::fs::OpenOptions;
use std::io::Cursor;
use std::io::Read;
use std::path::Path;
use thiserror::Error;
use zip::read::ZipArchive;

#[derive(Error, Debug)]
pub enum SilicaError {
    #[error("i/o error")]
    Io(#[from] std::io::Error),
    #[error("plist error")]
    PlistError(#[from] plist::Error),
    #[error("zip error")]
    ZipError(#[from] zip::result::ZipError),
    #[error("LZO decompression error")]
    LzoError(#[from] minilzo_rs::Error),
    #[error("ns archive error")]
    NsArchiveError(#[from] NsArchiveError),
    #[error("invalid values in file")]
    InvalidValue,
    #[error("unknown decoding error")]
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
        f.write_str(self.to_str())
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

    pub fn to_str(&self) -> &'static str {
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

struct TilingMeta {
    columns: u32,
    rows: u32,
    diff: Size<u32>,
    tile_size: u32,
}

#[derive(Debug)]
pub struct Flipped {
    pub horizontally: bool,
    pub vertically: bool,
}

#[derive(Debug)]
pub struct ProcreateFile {
    // animation:ValkyrieDocumentAnimation?
    pub author_name: Option<String>,
    pub background_hidden: bool,
    pub background_color: [f32; 4],
    //     backgroundColorHSBA:Data?
    //     closedCleanlyKey:Bool?
    //     colorProfile:ValkyrieColorProfile?
    //     composite:SilicaLayer?
    // //  public var drawingguide
    //     faceBackgroundHidden:Bool?
    //     featureSet:Int? = 1
    pub flipped: Flipped,
    //     isFirstItemAnimationForeground:Bool?
    //     isLastItemAnimationBackground:Bool?
    // //  public var lastTextStyling
    pub layers: SilicaGroup,
    //     mask:SilicaLayer?
    pub name: Option<String>,
    pub orientation: u32,
    //     orientation:Int?
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
    pub composite: SilicaLayer,
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
    pub name: String,
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
    pub mask: Option<Box<usize>>,
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
    pub image: usize,
}

type ZipArchiveMmap<'a> = ZipArchive<Cursor<&'a [u8]>>;

impl ProcreateFile {
    pub fn open<P: AsRef<Path>>(
        p: P,
        dev: &LogicalDevice,
    ) -> Result<(Self, Vec<GpuTexture>), SilicaError> {
        let path = p.as_ref();
        let file = OpenOptions::new().read(true).write(false).open(path)?;

        let mapping = unsafe { memmap2::Mmap::map(&file)? };
        let mut archive = ZipArchive::new(Cursor::new(&mapping[..]))?;

        let nka: NsKeyedArchive = {
            let mut document = archive.by_name("Document.archive")?;

            let mut buf = Vec::with_capacity(document.size() as usize);
            document.read_to_end(&mut buf)?;

            plist::from_reader(Cursor::new(buf))?
        };

        Self::from_ns(archive, nka, dev)
    }

    fn from_ns(
        archive: ZipArchiveMmap<'_>,
        nka: NsKeyedArchive,
        dev: &LogicalDevice,
    ) -> Result<(Self, Vec<GpuTexture>), SilicaError> {
        let root = nka.root()?;

        let size = nka.decode::<Size<u32>>(root, "size")?;
        let tile_size = nka.decode::<u32>(root, "tileSize")?;
        let columns = (size.width + tile_size - 1) / tile_size;
        let rows = (size.height + tile_size - 1) / tile_size;

        let meta = TilingMeta {
            columns,
            rows,
            diff: Size {
                width: columns * tile_size - size.width,
                height: rows * tile_size - size.height,
            },
            tile_size,
        };

        let file_names = archive.file_names().collect::<Vec<_>>();

        let mut gpu_textures = Vec::new();

        Ok((
            Self {
                author_name: nka.decode::<Option<String>>(root, "authorName")?,
                background_hidden: nka.decode::<bool>(root, "backgroundHidden")?,
                stroke_count: nka.decode::<usize>(root, "strokeCount")?,
                background_color: <[f32; 4]>::try_from(
                    nka.decode::<&[u8]>(root, "backgroundColor")?
                        .chunks_exact(4)
                        .map(|bytes| {
                            <[u8; 4]>::try_from(bytes)
                                .map(f32::from_le_bytes)
                                .map_err(|_| NsArchiveError::TypeMismatch)
                        })
                        .collect::<Result<Vec<f32>, _>>()?,
                )
                .unwrap(),
                name: nka.decode::<Option<String>>(root, "name")?,
                orientation: nka.decode::<u32>(root, "orientation")?,
                flipped: Flipped {
                    horizontally: nka.decode::<bool>(root, "flippedHorizontally")?,
                    vertically: nka.decode::<bool>(root, "flippedVertically")?,
                },
                tile_size,
                size,
                composite: nka.decode::<SilicaIRLayer>(root, "composite")?.load(
                    &meta,
                    &archive,
                    &file_names,
                    dev,
                    &mut gpu_textures,
                )?,
                layers: SilicaGroup {
                    hidden: false,
                    name: String::from("Root Layer"),
                    children: nka
                        .decode::<WrappedArray<SilicaIRHierarchy>>(root, "unwrappedLayers")?
                        .objects
                        .into_iter()
                        .map(|ir| ir.load(&meta, &archive, &file_names, dev, &mut gpu_textures))
                        .collect::<Result<_, _>>()?,
                },
            },
            gpu_textures,
        ))
    }
}
