mod error;

use image::{imageops, Pixel, Rgba, RgbaImage};
use lzokay::decompress::decompress;
use once_cell::sync::OnceCell;
use plist::{Dictionary, Uid, Value};
use rayon::{
    iter::{IndexedParallelIterator, ParallelIterator},
    slice::{ParallelSlice, ParallelSliceMut},
};
use regex::Regex;
use serde::Deserialize;
use std::{
    error::Error,
    fs::{File, OpenOptions},
    io::{Cursor, Read},
};
use zip::read::ZipArchive;

use crate::error::NsArchiveError;

type Rgba8 = Rgba<u8>;

fn main() -> Result<(), Box<dyn Error>> {
    let mut archive = ZipArchive::new(
        OpenOptions::new()
            .read(true)
            .write(false)
            .open("./Gilvana.procreate")
            .unwrap(),
    )?;

    let mut document = archive.by_name("Document.archive")?;

    let mut buf = Vec::with_capacity(document.size() as usize);
    document.read_to_end(&mut buf)?;

    drop(document);

    let nka: NsKeyedArchive = plist::from_reader(Cursor::new(buf))?;

    let mut art = ProcreateFile::from_ns(archive, nka)?;

    let mut composite = RgbaImage::new(art.size.width, art.size.height);

    for (layer_i, layer) in art.layers.iter_mut().enumerate() {
        if layer.hidden {
            eprintln!("its hidden {layer_i} {:?}", layer.name);
            continue;
        }

        layer_blend(
            &mut composite,
            &layer.image.as_ref().unwrap(),
            layer.opacity,
            match layer.blend {
                1 => multiply,
                2 => screen,
                11 => overlay,
                0 | _ => normal
            },
        );
        eprintln!("Done {layer_i}: {:?} {}", layer.name, layer.blend);
    }

    composite.save("./out/final.png")?;

    art.composite.image.unwrap().save("./out/reference.png")?;
    Ok(())
}

pub fn layer_blend(
    bottom: &mut RgbaImage,
    top: &RgbaImage,
    top_opacity: f32,
    blender: BlendingFunction,
) {
    assert_eq!(bottom.dimensions(), top.dimensions());

    let bottom_iter = bottom
        .par_chunks_exact_mut(usize::from(Rgba8::CHANNEL_COUNT))
        .map(Rgba8::from_slice_mut);

    let top_iter = top
        .par_chunks_exact(usize::from(Rgba8::CHANNEL_COUNT))
        .map(Rgba8::from_slice);

    bottom_iter
        .zip_eq(top_iter)
        .for_each(|(bottom, top)| *bottom = blend_pixel(*bottom, *top, top_opacity, blender));
}

pub fn comp(cv: f32, alpha: f32) -> f32 {
    cv * (1.0 - alpha)
}

pub fn normal(c1: f32, c2: f32, _: f32, a2: f32) -> f32 {
    c2 + comp(c1, a2)
}

pub fn multiply(c1: f32, c2: f32, a1: f32, a2: f32) -> f32 {
    c2 * c1 + comp(c2, a1) + comp(c1, a2)
}

// works great!
pub fn screen(c1: f32, c2: f32, a1: f32, a2: f32) -> f32 {
    c2 + c1 - c2 * c1
}

// works great!
pub fn overlay(c1: f32, c2: f32, a1: f32, a2: f32) -> f32 {
    if c1 * 2.0 <= a1 {
        c2 * c1 * 2.0 + comp(c2, a1) + comp(c1, a2)
    } else {
        comp(c2, a1) + comp(c1, a2) - 2.0 * (a1 - c1) * (a2 - c2) +  a2 * a1
    }
}

type BlendingFunction = fn(f32, f32, f32, f32) -> f32;

pub fn blend_pixel(a: Rgba8, b: Rgba8, fa: f32, blender: BlendingFunction) -> Rgba8 {
    // http://stackoverflow.com/questions/7438263/alpha-compositing-algorithm-blend-modes#answer-11163848

    // First, as we don't know what type our pixel is, we have to convert to floats between 0.0 and 1.0
    let max_t = f32::from(u8::MAX);
    let [bg @ .., bg_a] = a.0.map(|v| f32::from(v) / max_t);
    let [fg @ .., mut fg_a] = b.0.map(|v| f32::from(v) / max_t);
    fg_a *= fa;

    // Work out what the final alpha level will be
    let alpha_final = bg_a + fg_a - bg_a * fg_a;
    if alpha_final == 0.0 {
        return a;
    }

    // We premultiply our channels by their alpha, as this makes it easier to calculate
    let bga = bg.map(|v| v * bg_a);
    let fga = fg.map(|v| v * fg_a);

    // Standard formula for src-over alpha compositing
    let outa = [
        blender(bga[0], fga[0], bg_a, fg_a),
        blender(bga[1], fga[1], bg_a, fg_a),
        blender(bga[2], fga[2], bg_a, fg_a),
    ];

    // Unmultiply the channels by our resultant alpha channel
    let out = outa.map(|v| v / alpha_final);

    // Cast back to our initial type on return
    Rgba([
        (max_t * out[0]) as u8,
        (max_t * out[1]) as u8,
        (max_t * out[2]) as u8,
        (max_t * alpha_final) as u8,
    ])
}

struct TilingMeta {
    columns: u32,
    rows: u32,
    diff: Size,
    tile_size: u32,
}

struct ProcreateFile {
    // animation:ValkyrieDocumentAnimation?
    // authorName: Option<String>
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
    //     layers:[SilicaLayer]?
    //     mask:SilicaLayer?
    //     name:String?
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
    composite: SilicaLayer,
    archive: ZipArchive<File>,
    size: Size,
    meta: TilingMeta,
    layers: Vec<SilicaLayer>,
}

impl ProcreateFile {
    pub fn from_ns(
        mut archive: ZipArchive<File>,
        nka: NsKeyedArchive,
    ) -> Result<Self, NsArchiveError> {
        let root = nka.decode::<&'_ Dictionary>(&nka.top, "root")?;

        println!("{root:#?}");
        let ul = nka.decode::<WrappedArray<&'_ Value>>(root, "unwrappedLayers")?.objects;

        // println!("UNWRAPPEDLAYERS {ul:#?}");
        for z in ul {
            println!("UNWRAP {:#?}", z)
        }

        let ns_layers = nka.decode::<&'_ Dictionary>(root, "layers")?;

        println!("LAYERS {ns_layers:#?}");

        let file_names = archive.file_names().map(str::to_owned).collect::<Vec<_>>();

        let size = nka.decode::<Size>(root, "size")?;
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

        let mut composite = SilicaLayer::from_ns(&nka, nka.decode(root, "composite")?)?;
        composite.load_image(&meta, &mut archive, &file_names);

        let mut layers = nka
            .decode::<WrappedArray<SilicaLayer>>(root, "layers")?
            .objects;

        for layer in layers.iter_mut() {
            layer.load_image(&meta, &mut archive, &file_names);
        }

        Ok(Self {
            archive,
            size,
            composite,
            meta,
            layers,
        })
    }
}

struct SilicaLayer {
    // animationHeldLength:Int?
    blend: u32,
    // bundledImagePath:String?
    // bundledMaskPath:String?
    // bundledVideoPath:String?
    clipped: bool,
    // contentsRect:Data?
    // contentsRectValid:Bool?
    // document:SilicaDocument?
    // extendedBlend:Int?
    hidden: bool,
    // locked:Bool?
    mask: Option<Box<SilicaLayer>>,
    name: Option<String>,
    opacity: f32,
    // perspectiveAssisted:Bool?
    // preserve:Bool?
    // private:Bool?
    // text:ValkyrieText?
    // textPDF:Data?
    // transform:Data?
    // type:Int?
    size_width: u32,
    size_height: u32,
    uuid: String,
    version: u64,
    image: Option<RgbaImage>,
}

impl SilicaLayer {
    fn from_ns(nka: &NsKeyedArchive, coder: &Dictionary) -> Result<Self, NsArchiveError> {
        // let transform = nka.decode_value(coder, "transform")?;
        // dbg!(transform);

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

    fn load_image(
        &mut self,
        meta: &TilingMeta,
        archive: &mut ZipArchive<File>,
        file_names: &[String],
    ) {
        let indexr = Regex::new("(\\d+)~(\\d+)").unwrap();

        let mut image_layer = RgbaImage::new(self.size_width, self.size_height);

        for path in file_names {
            if !path.starts_with(&self.uuid) {
                continue;
            }

            let chunk_str = &path[self.uuid.len()..path.find('.').unwrap_or(path.len())];
            let captures = indexr.captures(&chunk_str).unwrap();
            let col = u32::from_str_radix(captures.get(1).unwrap().as_str(), 10).unwrap();
            let row = u32::from_str_radix(captures.get(2).unwrap().as_str(), 10).unwrap();

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
            let mut dst =
                vec![0; (tile_width * tile_height * u32::from(Rgba8::CHANNEL_COUNT)) as usize];
            decompress(&buf, &mut dst).unwrap();
            let chunked_image = RgbaImage::from_vec(tile_width, tile_height, dst).unwrap();
            imageops::replace(
                &mut image_layer,
                &chunked_image,
                (col * meta.tile_size) as i64,
                (row * meta.tile_size) as i64,
            );
        }

        // image_layer
        //     .par_chunks_exact_mut(usize::from(Rgba8::CHANNEL_COUNT))
        //     .map(Rgba8::from_slice_mut)
        //     .for_each(|pixel| pixel[3] = (f32::from(pixel[3]) * self.opacity) as u8);

        self.image = Some(image_layer);
    }
}

#[derive(Deserialize)]
struct NsKeyedArchive {
    // #[serde(rename = "$version")]
    // version: usize,
    // #[serde(rename = "$archiver")]
    // archiver: String,
    #[serde(rename = "$top")]
    top: Dictionary,
    #[serde(rename = "$objects")]
    objects: Vec<Value>,
}

impl NsKeyedArchive {
    fn resolve_index<'a>(&'a self, idx: usize) -> Result<Option<&'a Value>, NsArchiveError> {
        if idx == 0 {
            Ok(None)
        } else {
            self.objects
                .get(idx)
                .ok_or(NsArchiveError::BadIndex)
                .map(Some)
        }
    }

    fn decode_value<'a>(
        &'a self,
        coder: &'a Dictionary,
        key: &str,
    ) -> Result<Option<&'a Value>, NsArchiveError> {
        return match coder.get(key) {
            Some(Value::Uid(uid)) => self.resolve_index(uid.get() as usize),
            value @ _ => Ok(value),
        };
    }

    fn decode<'a, T: NsCoding<'a>>(
        &'a self,
        coder: &'a Dictionary,
        key: &str,
    ) -> Result<T, NsArchiveError> {
        T::decode(self, self.decode_value(coder, key)?)
    }
}

trait NsCoding<'a>: Sized {
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError>;
}

impl NsCoding<'_> for bool {
    fn decode(_: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_boolean()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl NsCoding<'_> for u64 {
    fn decode(_: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_unsigned_integer()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl NsCoding<'_> for i64 {
    fn decode(_: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_signed_integer()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl NsCoding<'_> for f64 {
    fn decode(_: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_real()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl NsCoding<'_> for u32 {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        u32::try_from(u64::decode(nka, val)?).map_err(|_| NsArchiveError::TypeMismatch)
    }
}

impl NsCoding<'_> for i32 {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        i32::try_from(i64::decode(nka, val)?).map_err(|_| NsArchiveError::TypeMismatch)
    }
}

impl NsCoding<'_> for f32 {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        f64::decode(nka, val).map(|v| v as f32)
    }
}

impl<'a> NsCoding<'a> for &'a Dictionary {
    fn decode(_: &NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_dictionary()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl<'a> NsCoding<'a> for &'a Value {
    fn decode(_: &NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)
    }
}

impl NsCoding<'_> for Uid {
    fn decode(_: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_uid()
            .copied()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl<'a> NsCoding<'a> for &'a str {
    fn decode(_: &NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_string()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl<'a> NsCoding<'a> for String {
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        Ok(<&'_ str>::decode(nka, val)?.to_owned())
    }
}
impl<'a, T> NsCoding<'a> for Option<T>
where
    T: NsCoding<'a>,
{
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        val.map_or(Ok(None), |a| Some(T::decode(nka, Some(a))).transpose())
    }
}

#[derive(Debug, Clone, Copy)]
struct Size {
    width: u32,
    height: u32,
}

impl NsCoding<'_> for Size {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        let string = <&'_ str>::decode(nka, val)?;

        static INSTANCE: OnceCell<Regex> = OnceCell::new();
        let size_regex = INSTANCE.get_or_init(|| Regex::new("\\{(\\d+), ?(\\d+)\\}").unwrap());
        let captures = size_regex
            .captures(string)
            .ok_or(NsArchiveError::TypeMismatch)?;

        let width = u32::from_str_radix(captures.get(1).unwrap().as_str(), 10).unwrap();
        let height = u32::from_str_radix(captures.get(2).unwrap().as_str(), 10).unwrap();
        Ok(Size { width, height })
    }
}

impl<'a, T> NsCoding<'a> for Vec<T>
where
    T: NsCoding<'a>,
{
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        let array = val
            .ok_or(NsArchiveError::MissingKey)?
            .as_array()
            .ok_or(NsArchiveError::TypeMismatch)?;

        let mut vec = Vec::with_capacity(array.len());

        for val in array {
            vec.push(T::decode(nka, Some(val))?);
        }

        Ok(vec)
    }
}

#[derive(Debug)]
struct WrappedArray<T> {
    objects: Vec<T>,
}

impl<'a, T> NsCoding<'a> for WrappedArray<T>
where
    T: NsCoding<'a>,
{
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        let array = WrappedRawArray::decode(nka, val)?.inner;

        let mut objects = Vec::with_capacity(array.len());
        for uid in array.iter().rev() {
            let val = nka
                .resolve_index(uid.get() as usize)?
                .ok_or(NsArchiveError::BadIndex)?;

            objects.push(T::decode(nka, Some(val))?);
        }
        Ok(WrappedArray { objects })
    }
}

#[derive(Debug)]
struct WrappedRawArray {
    inner: Vec<Uid>,
}

impl NsCoding<'_> for WrappedRawArray {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        let coder = <&'_ Dictionary>::decode(nka, val)?;
        let objects = nka.decode::<Vec<Uid>>(coder, "NS.objects")?;
        Ok(WrappedRawArray { inner: objects })
    }
}

impl NsCoding<'_> for SilicaLayer {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        let coder = <&'_ Dictionary>::decode(nka, val)?;
        SilicaLayer::from_ns(nka, coder)
    }
}
