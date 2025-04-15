use crate::layers::{AtlasData, Flipped, SilicaGroup, SilicaLayer, TilingData};
use crate::{
    error::SilicaError,
    ir::{IRData, SilicaIRHierarchy, SilicaIRLayer},
    ns_archive::{NsKeyedArchive, Size, WrappedArray, error::NsArchiveError},
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

impl ProcreateFile {
    // Load a Procreate file asynchronously.
    pub fn open<P: AsRef<Path>>(
        p: P,
        dispatch: &GpuDispatch,
    ) -> Result<(Self, GpuTexture, TilingData), SilicaError> {
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
    ) -> Result<(Self, GpuTexture, TilingData), SilicaError> {
        let root = nka.root()?;

        let size = nka.fetch::<Size<u32>>(root, "size")?;
        let tile_size = nka.fetch::<u32>(root, "tileSize")?;
        let columns = size.width.div_ceil(tile_size);
        let rows = size.height.div_ceil(tile_size);

        let file_names = archive.file_names().collect::<Vec<_>>();

        let ir_hierachy = nka
            .fetch::<WrappedArray<SilicaIRHierarchy>>(root, "unwrappedLayers")?
            .objects;

        let gpu_textures = GpuTexture::empty_layers(
            dispatch,
            size.width,
            size.height,
            ir_hierachy.iter().map(|ir| ir.count_layer()).sum::<u32>() + 1,
            GpuTexture::LAYER_USAGE,
        );

        let chunk_count = file_names.len() as u32;

        let tile = TilingData {
            columns,
            rows,
            diff: Size {
                width: columns * tile_size - size.width,
                height: rows * tile_size - size.height,
            },
            size: tile_size,
            atlas: AtlasData::compute_atlas_size(chunk_count, tile_size),
        };

        dbg!(chunk_count);
        dbg!(&tile);

        let texture_chunks = GpuTexture::empty_layers(
            &dispatch,
            tile.size * tile.atlas.columns,
            tile.size * tile.atlas.rows,
            tile.atlas.layers,
            GpuTexture::ATLAS_USAGE,
        );

        let ir_data = IRData {
            tile: &tile,
            archive: &archive,
            size,
            file_names: &file_names,
            dispatch,
            texture_chunks: &texture_chunks,
            gpu_textures: &gpu_textures,
            combined_counter: &AtomicU32::new(0),
            chunk_counter: &AtomicU32::new(0),
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
            tile,
        ))
    }
}
