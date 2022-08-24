mod ir;

use crate::gpu::{GpuTexture, LogicalDevice};
use crate::ns_archive::{NsArchiveError, NsClass, Size, WrappedArray};
use crate::ns_archive::{NsDecode, NsKeyedArchive};
use image::{Pixel, Rgba};
use minilzo_rs::LZO;
use once_cell::sync::OnceCell;
use rayon::iter::{IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator};
use rayon::prelude::IntoParallelIterator;
use regex::Regex;
use std::fs::OpenOptions;
use std::io::Cursor;
use std::io::Read;
use std::path::Path;
use thiserror::Error;
use zip::read::ZipArchive;

use self::ir::{SilicaIRGroup, SilicaIRHierarchy, SilicaIRLayer};

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
    #[error("no graphics device")]
    NoGraphicsDevice,
}

enum BlendingMode {
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
    // pub backgroundColor: Data?
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

type ZipArchiveMmap<'a> = ZipArchive<Cursor<&'a [u8]>>;

impl ProcreateFile {
    pub fn open<P: AsRef<Path>>(p: P, dev: &LogicalDevice) -> Result<Self, SilicaError> {
        let path = p.as_ref();
        let file = OpenOptions::new().read(true).write(false).open(path)?;

        // TODO: file locking for this unsafe memmap
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
    ) -> Result<Self, SilicaError> {
        let root = nka.root()?;

        // println!("{root:#?}");

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

        let file_names = archive.file_names().map(str::to_owned).collect::<Vec<_>>();

        Ok(Self {
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
            composite: SilicaLayer::load(
                nka.decode::<SilicaIRLayer>(root, "composite")?,
                &meta,
                &archive,
                &file_names,
                dev,
            )?,
            layers: SilicaGroup {
                hidden: false,
                name: String::from("Root Layer"),
                children: nka
                    .decode::<WrappedArray<SilicaIRHierarchy>>(root, "unwrappedLayers")?
                    .objects
                    .into_par_iter()
                    .map(|ir| SilicaHierarchy::load(ir, &meta, &archive, &file_names, dev))
                    .collect::<Result<_, _>>()?,
            },
        })
    }
}

#[derive(Debug)]
pub struct SilicaLayer {
    // animationHeldLength:Int?
    pub blend: u32,
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
    pub mask: Option<Box<SilicaLayer>>,
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
    pub image: GpuTexture,
}

impl SilicaLayer {
    fn load(
        ir: SilicaIRLayer,
        meta: &TilingMeta,
        archive: &ZipArchiveMmap<'_>,
        file_names: &[String],
        render: &LogicalDevice,
    ) -> Result<Self, SilicaError> {
        let nka = ir.nka;
        let coder = ir.coder;
        let blend = nka.decode::<u32>(coder, "extendedBlend")?;
        let clipped = nka.decode::<bool>(coder, "clipped")?;
        let hidden = nka.decode::<bool>(coder, "hidden")?;
        let mask = None;
        let name = nka.decode::<Option<String>>(coder, "name")?;
        let opacity = nka.decode::<f32>(coder, "opacity")?;
        let uuid = nka.decode::<String>(coder, "UUID")?;
        let version = nka.decode::<u64>(coder, "version")?;
        let size = Size {
            width: nka.decode::<u32>(coder, "sizeWidth")?,
            height: nka.decode::<u32>(coder, "sizeHeight")?,
        };

        static INSTANCE: OnceCell<Regex> = OnceCell::new();
        let index_regex = INSTANCE.get_or_init(|| Regex::new("(\\d+)~(\\d+)").unwrap());

        static LZO_INSTANCE: OnceCell<LZO> = OnceCell::new();
        let lzo = LZO_INSTANCE.get_or_init(|| minilzo_rs::LZO::init().unwrap());

        let gpu_texture = GpuTexture::empty(&render.device, size.width, size.height, None);

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

        Ok(Self {
            blend,
            clipped,
            hidden,
            mask,
            name,
            opacity,
            size,
            uuid,
            version,
            image: gpu_texture,
        })
    }
}

#[derive(Debug)]
pub struct SilicaGroup {
    pub hidden: bool,
    pub children: Vec<SilicaHierarchy>,
    pub name: String,
}

impl SilicaGroup {
    fn load(
        ir: SilicaIRGroup,
        meta: &TilingMeta,
        archive: &ZipArchiveMmap<'_>,
        file_names: &[String],
        render: &LogicalDevice,
    ) -> Result<Self, SilicaError> {
        let nka = ir.nka;
        let coder = ir.coder;
        Ok(Self {
            hidden: nka.decode::<bool>(coder, "isHidden")?,
            name: nka.decode::<String>(coder, "name")?,
            children: ir
                .children
                .into_par_iter()
                .map(|ir| SilicaHierarchy::load(ir, meta, archive, file_names, render))
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Debug)]
pub enum SilicaHierarchy {
    Layer(SilicaLayer),
    Group(SilicaGroup),
}

impl SilicaHierarchy {
    fn load(
        ir: SilicaIRHierarchy,
        meta: &TilingMeta,
        archive: &ZipArchiveMmap<'_>,
        file_names: &[String],
        render: &LogicalDevice,
    ) -> Result<Self, SilicaError> {
        Ok(match ir {
            SilicaIRHierarchy::Layer(layer) => {
                SilicaHierarchy::Layer(SilicaLayer::load(layer, meta, archive, file_names, render)?)
            }
            SilicaIRHierarchy::Group(group) => {
                SilicaHierarchy::Group(SilicaGroup::load(group, meta, archive, file_names, render)?)
            }
        })
    }
}
