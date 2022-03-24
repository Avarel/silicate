use crate::canvas::{Rgba8Canvas, Rgba8};
use crate::ns_archive::{NsArchiveError, NsClass, Size, WrappedArray};
use crate::ns_archive::{NsDecode, NsKeyedArchive};
use lzokay::decompress::decompress;
use once_cell::sync::OnceCell;
use plist::{Dictionary, Value};
use regex::Regex;
use std::fs::OpenOptions;
use std::io::Cursor;
use std::path::Path;
use std::{fs::File, io::Read};
use zip::read::ZipArchive;

struct TilingMeta {
    columns: usize,
    rows: usize,
    diff: Size,
    tile_size: usize,
}

#[derive(Debug)]
pub struct ProcreateFile {
    // animation:ValkyrieDocumentAnimation?
    pub author_name: Option<String>,
    //     backgroundColor:Data?
    // backgroundHidden:Bool?
    //     backgroundColorHSBA:Data?
    //     closedCleanlyKey:Bool?
    //     colorProfile:ValkyrieColorProfile?
    //     composite:SilicaLayer?
    // //  public var drawingguide
    //     faceBackgroundHidden:Bool?
    //     featureSet:Int? = 1
    //     flippedHorizontally:Bool?
    //     flippedVertically:Bool?
    //     isFirstItemAnimationForeground:Bool?
    //     isLastItemAnimationBackground:Bool?
    // //  public var lastTextStyling
    pub layers: SilicaGroup,
    //     mask:SilicaLayer?
    pub name: Option<String>,
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
    //     strokeCount: Int?
    //     tileSize: Int?
    //     videoEnabled: Bool? = true
    //     videoQualityKey: String?
    //     videoResolutionKey: String?
    //     videoDuration: String? = "Calculating..."
    pub tile_size: usize,
    pub composite: SilicaLayer,
    pub size: Size,
}

impl ProcreateFile {
    pub fn open<P: AsRef<Path>>(p: P) -> Result<Self, NsArchiveError> {
        Self::from_file(OpenOptions::new().read(true).write(false).open(p)?)
    }

    pub fn from_file(f: File) -> Result<Self, NsArchiveError> {
        let mut archive = ZipArchive::new(f)?;

        let mut document = archive.by_name("Document.archive")?;

        let mut buf = Vec::with_capacity(document.size() as usize);
        document.read_to_end(&mut buf)?;

        drop(document);

        let nka: NsKeyedArchive = plist::from_reader(Cursor::new(buf))?;

        Self::from_ns(archive, nka)
    }

    fn from_ns(mut archive: ZipArchive<File>, nka: NsKeyedArchive) -> Result<Self, NsArchiveError> {
        let root = nka.root()?;

        println!("{root:#?}");

        let file_names = archive.file_names().map(str::to_owned).collect::<Vec<_>>();

        let size = nka.decode::<Size>(root, "size")?;
        let tile_size = nka.decode::<usize>(root, "tileSize")?;
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

        // let mut composite = SilicaLayer::from_ns(&nka, nka.decode(root, "composite")?)?;
        let mut composite = nka.decode::<SilicaLayer>(root, "composite")?;
        composite.load_image(&meta, &mut archive, &file_names);

        let mut layers = nka
            .decode::<WrappedArray<SilicaHierarchy>>(root, "unwrappedLayers")?
            .objects;

        layers
            .iter_mut()
            .for_each(|v| v.apply(&mut |layer| layer.load_image(&meta, &mut archive, &file_names)));

        Ok(Self {
            author_name: nka.decode::<Option<String>>(root, "authorName")?,
            name: nka.decode::<Option<String>>(root, "name")?,
            tile_size,
            size,
            composite,
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
    pub size_width: u32,
    pub size_height: u32,
    pub uuid: String,
    pub version: u64,
    pub image: Option<Rgba8Canvas>,
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
            .field("size_width", &self.size_width)
            .field("size_height", &self.size_height)
            .field("uuid", &self.uuid)
            .field("version", &self.version)
            .finish()
    }
}

impl SilicaLayer {
    fn load_image(
        &mut self,
        meta: &TilingMeta,
        archive: &mut ZipArchive<File>,
        file_names: &[String],
    ) {
        static INSTANCE: OnceCell<Regex> = OnceCell::new();
        let index_regex = INSTANCE.get_or_init(|| Regex::new("(\\d+)~(\\d+)").unwrap());

        let mut image_layer = Rgba8Canvas::new(self.size_width as usize, self.size_height as usize);
        //RgbaImage::new(self.size_width, self.size_height);

        for path in file_names {
            if !path.starts_with(&self.uuid) {
                continue;
            }

            let chunk_str = &path[self.uuid.len()..path.find('.').unwrap_or(path.len())];
            let captures = index_regex.captures(&chunk_str).unwrap();
            let col = usize::from_str_radix(captures.get(1).unwrap().as_str(), 10).unwrap();
            let row = usize::from_str_radix(captures.get(2).unwrap().as_str(), 10).unwrap();

            let tile_width = meta.tile_size
                - if col != meta.columns - 1 {
                    0
                } else {
                    meta.diff.width
                };
            let tile_height = meta.tile_size
                - if row != meta.rows - 1 {
                    0
                } else {
                    meta.diff.height
                };

            let mut chunk = archive.by_name(path).unwrap();
            let mut buf = Vec::new();
            chunk.read_to_end(&mut buf).unwrap();
            // RGBA = 4 channels of 8 bits each, lzo decompressed to lzo data
            let mut dst = vec![0; tile_width * tile_height * Rgba8::CHANNELS];
            decompress(&buf, &mut dst).unwrap();
            let chunked_image =
                Rgba8Canvas::from_vec(tile_width as usize, tile_height as usize, dst);
            // imageops::replace(
            //     &mut image_layer,
            //     &chunked_image,
            //     (col * meta.tile_size) as i64,
            //     (row * meta.tile_size) as i64,
            // );
            // composite::replace(
            //     &mut image_layer,
            //     &chunked_image,
            //     (col * meta.tile_size) as usize,
            //     (row * meta.tile_size) as usize,
            // );
            image_layer.replace(
                &chunked_image,
                (col * meta.tile_size) as usize,
                (row * meta.tile_size) as usize,
            );
        }

        // Note: the adapter is considerably slow since it checks if the image fits
        self.image = Some(image_layer);
    }
}

impl NsDecode<'_> for SilicaLayer {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        let coder = <&'_ Dictionary>::decode(nka, val)?;
        Ok(Self {
            blend: nka.decode::<u32>(coder, "blend")?,
            clipped: nka.decode::<bool>(coder, "clipped")?,
            hidden: nka.decode::<bool>(coder, "hidden")?,
            mask: None,
            name: nka.decode::<Option<String>>(coder, "name")?,
            opacity: nka.decode::<f32>(coder, "opacity")?,
            uuid: nka.decode::<String>(coder, "UUID")?,
            version: nka.decode::<u64>(coder, "version")?,
            size_width: nka.decode::<u32>(coder, "sizeWidth")?,
            size_height: nka.decode::<u32>(coder, "sizeHeight")?,
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
    pub fn apply(&mut self, f: &mut impl FnMut(&mut SilicaLayer)) {
        match self {
            Self::Layer(layer) => f(layer),
            Self::Group(group) => group.children.iter_mut().for_each(|child| child.apply(f)),
        }
    }
}

impl NsDecode<'_> for SilicaHierarchy {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        let coder = <&'_ Dictionary>::decode(nka, val)?;
        let class = nka.decode::<NsClass>(coder, "$class")?;

        match class.class_name.as_str() {
            "SilicaGroup" => Ok(Self::Group(SilicaGroup::decode(nka, val)?)),
            "SilicaLayer" => Ok(Self::Layer(SilicaLayer::decode(nka, val)?)),
            _ => Err(NsArchiveError::TypeMismatch),
        }
    }
}
