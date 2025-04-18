use crate::layers::{AtlasTextureTiling, CanvasTiling, Flipped, SilicaGroup, SilicaLayer};
use crate::{
    error::SilicaError,
    ir::{IRData, SilicaIRHierarchy, SilicaIRLayer},
    ns_archive::{NsKeyedArchive, NsObjects, Size, error::NsArchiveError},
};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use silicate_compositor::dev::GpuDispatch;
use silicate_compositor::tex::GpuTexture;
use std::{
    fs::OpenOptions,
    io::{Cursor, Read},
    path::Path,
    sync::atomic::AtomicU32,
};
use zip::read::ZipArchive;

pub(crate) type ZipArchiveMmap<'a> = ZipArchive<Cursor<&'a [u8]>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    NoRotation,
    Clockwise180,
    Clockwise270,
    Clockwise90,
    Unknown,
}

impl crate::ns_archive::NsDecode<'_> for Orientation {
    fn decode(nka: &NsKeyedArchive, key: &str, val: &plist::Value) -> Result<Self, NsArchiveError> {
        Ok(match u64::decode(nka, key, val)? {
            1 => Self::NoRotation,
            2 => Self::Clockwise180,
            3 => Self::Clockwise270,
            4 => Self::Clockwise90,
            v => Err(NsArchiveError::BadValue(key.to_string(), v.to_string()))?,
        })
    }
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
    pub orientation: Orientation,
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

    pub layer_count: u32,
}

pub struct ProcreateFileMetadata {
    pub atlas_texture: GpuTexture,
    pub canvas_tiling: CanvasTiling,
}

impl ProcreateFile {
    // Load a Procreate file asynchronously.
    pub fn open<P: AsRef<Path>>(
        p: P,
        dispatch: &GpuDispatch,
    ) -> Result<(Self, ProcreateFileMetadata), SilicaError> {
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

        Self::from_ns(archive, nka, dispatch)
    }

    pub(crate) fn from_ns(
        archive: ZipArchiveMmap<'_>,
        nka: NsKeyedArchive,
        dispatch: &GpuDispatch,
    ) -> Result<(Self, ProcreateFileMetadata), SilicaError> {
        let root = nka.root()?;

        let size = nka.fetch::<Size<u32>>(root, "size")?;
        let tile_size = nka.fetch::<u32>(root, "tileSize")?;
        let (cols, rows) = (
            size.width.div_ceil(tile_size),
            size.height.div_ceil(tile_size),
        );

        let file_names = archive.file_names().collect::<Vec<_>>();

        let ir_hierachy = nka
            .fetch::<NsObjects<SilicaIRHierarchy>>(root, "unwrappedLayers")?
            .objects;

        let chunk_count = file_names.len() as u32;

        let canvas_tiling = CanvasTiling {
            cols,
            rows,
            diff: Size {
                width: cols * tile_size - size.width,
                height: rows * tile_size - size.height,
            },
            size: tile_size,
            atlas: AtlasTextureTiling::compute_atlas_size(chunk_count, tile_size),
        };

        let layer_count = ir_hierachy.iter().map(|ir| ir.count_layer()).sum::<u32>() + 1;

        let atlas_texture = GpuTexture::empty_layers(
            &dispatch,
            canvas_tiling.size * canvas_tiling.atlas.cols,
            canvas_tiling.size * canvas_tiling.atlas.rows,
            canvas_tiling.atlas.layers, // Make it an array
            GpuTexture::ATLAS_USAGE,
        );

        let ir_data = IRData {
            tiling: canvas_tiling,
            archive: &archive,
            size,
            file_names: &file_names,
            dispatch,
            chunk_id_counter: AtomicU32::new(1),
            atlas_texture: &atlas_texture,
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
                orientation: nka.fetch::<Orientation>(root, "orientation")?,
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
                layer_count,
            },
            ProcreateFileMetadata {
                atlas_texture,
                canvas_tiling,
            },
        ))
    }
}
