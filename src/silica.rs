use crate::gpu::{GpuTexture, LogicalDevice};
use crate::ns_archive::{NsArchiveError, NsClass, Size, WrappedArray};
use crate::ns_archive::{NsDecode, NsKeyedArchive};
use image::{Pixel, Rgba};
use minilzo_rs::LZO;
use once_cell::sync::OnceCell;
use plist::{Dictionary, Value};
use rayon::iter::{IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator};
use regex::Regex;
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
    pub flipped_horizontally: bool,
    pub flipped_vertically: bool,
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

        let mapping = unsafe { memmap2::Mmap::map(&file)? };
        let mut archive = ZipArchive::new(Cursor::new(&mapping[..]))?;

        let file_names = archive.file_names().map(str::to_owned).collect::<Vec<_>>();

        let nka: NsKeyedArchive = {
            let mut document = archive.by_name("Document.archive")?;

            let mut buf = Vec::with_capacity(document.size() as usize);
            document.read_to_end(&mut buf)?;

            plist::from_reader(Cursor::new(buf))?
        };

        Self::from_ns(archive, &file_names, nka, dev)
    }

    fn from_ns(
        archive: ZipArchiveMmap<'_>,
        file_names: &[String],
        nka: NsKeyedArchive,
        dev: &LogicalDevice
    ) -> Result<Self, SilicaError> {
        let root = nka.root()?;

        // println!("{root:#?}");

        let size = nka.decode::<Size<u32>>(root, "size")?;
        let tile_size = nka.decode::<u32>(root, "tileSize")?;
        let columns = size.width / tile_size + if size.width % tile_size == 0 { 0 } else { 1 };
        let rows = size.height / tile_size + if size.height % tile_size == 0 { 0 } else { 1 };

        let meta = TilingMeta {
            columns,
            rows,
            diff: Size {
                width: columns * tile_size - size.width,
                height: rows * tile_size - size.height,
            },
            tile_size,
        };

        let mut composite = SilicaHierarchy::Layer(nka.decode::<SilicaLayer>(root, "composite")?);

        let mut layers = nka
            .decode::<WrappedArray<SilicaHierarchy>>(root, "unwrappedLayers")?
            .objects;

        layers
            .par_iter_mut()
            .chain([&mut composite])
            .for_each(|layer| {
                layer.apply_mut(&mut |layer| {
                    layer.load_image(&meta, archive.clone(), &file_names, dev)
                })
            });

        let background_color = <[f32; 4]>::try_from(
            nka.decode::<&[u8]>(root, "backgroundColor")?
                .chunks_exact(4)
                .map(|bytes| {
                    <[u8; 4]>::try_from(bytes)
                        .map(f32::from_le_bytes)
                        .map_err(|_| NsArchiveError::TypeMismatch)
                })
                .collect::<Result<Vec<f32>, _>>()?,
        )
        .unwrap();

        Ok(Self {
            author_name: nka.decode::<Option<String>>(root, "authorName")?,
            background_hidden: nka.decode::<bool>(root, "backgroundHidden")?,
            stroke_count: nka.decode::<usize>(root, "strokeCount")?,
            background_color,
            name: nka.decode::<Option<String>>(root, "name")?,
            orientation: nka.decode::<u32>(root, "orientation")?,
            flipped_horizontally: nka.decode::<bool>(root, "flippedHorizontally")?,
            flipped_vertically: nka.decode::<bool>(root, "flippedVertically")?,
            tile_size,
            size,
            composite: composite.unwrap_layer(),
            layers: SilicaGroup {
                hidden: false,
                name: String::new(),
                children: layers,
            },
        })
    }
}

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
    pub image: Option<GpuTexture>,
}

impl std::fmt::Debug for SilicaLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SilicaLayer")
            .field("blend", &self.blend)
            .field("clipped", &self.clipped)
            .field("hidden", &self.hidden)
            .field("mask", &self.mask)
            .field("name", &self.name)
            .field("opacity", &self.opacity)
            .field("size_width", &self.size)
            .field("uuid", &self.uuid)
            .field("version", &self.version)
            .finish()
    }
}

impl SilicaLayer {
    fn load_image(
        &mut self,
        meta: &TilingMeta,
        archive: ZipArchiveMmap<'_>,
        file_names: &[String],
        render: &LogicalDevice,
    ) {
        static INSTANCE: OnceCell<Regex> = OnceCell::new();
        let index_regex = INSTANCE.get_or_init(|| Regex::new("(\\d+)~(\\d+)").unwrap());

        static LZO_INSTANCE: OnceCell<LZO> = OnceCell::new();
        let lzo = LZO_INSTANCE.get_or_init(|| minilzo_rs::LZO::init().unwrap());

        let gpu_texture =
            GpuTexture::empty(&render.device, self.size.width, self.size.height, None);

        file_names
            .par_iter()
            .filter(|path| path.starts_with(&self.uuid))
            .for_each(|path| {
                let mut archive = archive.clone();

                let chunk_str = &path[self.uuid.len()..path.find('.').unwrap_or(path.len())];
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

                let mut chunk = archive.by_name(path).unwrap();
                let mut buf = Vec::new();
                chunk.read_to_end(&mut buf).unwrap();
                // RGBA = 4 channels of 8 bits each, lzo decompressed to lzo data
                let dst = lzo.decompress_safe(&buf[..], tile_width * tile_height * usize::from(Rgba::<u8>::CHANNEL_COUNT)).unwrap();
                gpu_texture.replace(
                    &render.queue,
                    col * meta.tile_size,
                    row * meta.tile_size,
                    tile_width as u32,
                    tile_height as u32,
                    &dst,
                );
            });

        // Note: the adapter is considerably slow since it checks if the image fits
        self.image = Some(gpu_texture);
    }
}

impl NsDecode<'_> for SilicaLayer {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        let coder = <&'_ Dictionary>::decode(nka, val)?;
        Ok(Self {
            blend: nka.decode::<u32>(coder, "extendedBlend")?,
            clipped: nka.decode::<bool>(coder, "clipped")?,
            hidden: nka.decode::<bool>(coder, "hidden")?,
            mask: None,
            name: nka.decode::<Option<String>>(coder, "name")?,
            opacity: nka.decode::<f32>(coder, "opacity")?,
            uuid: nka.decode::<String>(coder, "UUID")?,
            version: nka.decode::<u64>(coder, "version")?,
            size: Size {
                width: nka.decode::<u32>(coder, "sizeWidth")?,
                height: nka.decode::<u32>(coder, "sizeHeight")?,
            },
            image: None,
        })
    }
}

#[derive(Debug)]
pub struct SilicaGroup {
    pub hidden: bool,
    pub children: Vec<SilicaHierarchy>,
    pub name: String,
}

impl NsDecode<'_> for SilicaGroup {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        let coder = <&'_ Dictionary>::decode(nka, val)?;
        Ok(Self {
            hidden: nka.decode::<bool>(coder, "isHidden")?,
            name: nka.decode::<String>(coder, "name")?,
            children: nka
                .decode::<WrappedArray<SilicaHierarchy>>(coder, "children")?
                .objects,
        })
    }
}

#[derive(Debug)]
pub enum SilicaHierarchy {
    Layer(SilicaLayer),
    Group(SilicaGroup),
}

impl SilicaHierarchy {
    pub fn apply_mut(&mut self, f: &mut dyn FnMut(&mut SilicaLayer)) {
        match self {
            Self::Layer(layer) => f(layer),
            Self::Group(group) => group
                .children
                .iter_mut()
                .for_each(|child| child.apply_mut(f)),
        }
    }

    pub fn unwrap_layer(self) -> SilicaLayer {
        match self {
            Self::Layer(layer) => layer,
            _ => panic!(),
        }
    }
}

impl NsDecode<'_> for SilicaHierarchy {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        let coder = <&'_ Dictionary>::decode(nka, val)?;
        let class = nka.decode::<NsClass>(coder, "$class")?;

        match class.class_name.as_str() {
            "SilicaGroup" => Ok(SilicaGroup::decode(nka, val).map(Self::Group)?),
            "SilicaLayer" => Ok(SilicaLayer::decode(nka, val).map(Self::Layer)?),
            _ => Err(NsArchiveError::TypeMismatch),
        }
    }
}
