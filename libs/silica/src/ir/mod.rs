mod hierarchy;

use std::sync::atomic::AtomicU32;

use crate::data::{Flipped, Orientation};
use crate::file::{ProcreateFile, ProcreateFileMetadata};
use crate::layers::AtlasTextureTiling;
use crate::ns_archive::{NsKeyedArchive, NsObjects, Size, error::NsArchiveError};
use crate::{
    error::SilicaError,
    layers::{CanvasTiling, SilicaGroup},
};
use hierarchy::{SilicaIRHierarchy, SilicaIRLayer};
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use silicate_compositor::dev::GpuDispatch;
use silicate_compositor::tex::GpuTexture;

struct IRData<'a> {
    archive: &'a crate::file::ZipArchiveMmap<'a>,
    file_names: Vec<&'a str>,

    size: Size<u32>,
    tiling: CanvasTiling,
    chunk_id_counter: AtomicU32,
}

pub struct ProcreateUnloadedFile<'a> {
    pub author_name: Option<String>,
    pub background_hidden: bool,
    pub background_color: [f32; 4],
    //     closedCleanlyKey:Bool?
    //     colorProfile:ValkyrieColorProfile?

    // //  public var drawingguide
    //     faceBackgroundHidden:Bool?
    //     1 => BlendingMode::featureSet:Int?
    pub flipped: Flipped,
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
    pub tile_size: u32,

    pub layer_count: u32,

    info: IRData<'a>,

    layers: Vec<SilicaIRHierarchy<'a>>,
    composite: SilicaIRLayer<'a>,
}

impl<'a> ProcreateUnloadedFile<'a> {
    pub(super) fn from_ns(
        archive: &'a crate::file::ZipArchiveMmap<'a>,
        nka: &'a NsKeyedArchive,
    ) -> Result<Self, SilicaError> {
        let root = nka.root()?;

        let size = nka.fetch::<Size<u32>>(root, "size")?;
        let tile_size = nka.fetch::<u32>(root, "tileSize")?;
        let (cols, rows) = (
            size.width.div_ceil(tile_size),
            size.height.div_ceil(tile_size),
        );

        let file_names = archive.file_names().collect::<Vec<_>>();

        let layers = nka
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

        let layer_count = layers.iter().map(|ir| ir.count_layer()).sum::<u32>() + 1;

        Ok(Self {
            info: IRData {
                archive,
                file_names,
                size,
                tiling: canvas_tiling,
                chunk_id_counter: AtomicU32::new(1),
            },
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
            layer_count,
            composite: nka.fetch::<SilicaIRLayer>(root, "composite")?,
            layers,
        })
    }

    pub(super) fn load(
        self,
        dispatch: &GpuDispatch,
    ) -> Result<(ProcreateFile, ProcreateFileMetadata), SilicaError> {
        let canvas_tiling = self.info.tiling;
        let atlas_texture = GpuTexture::empty_layers(
            &dispatch,
            canvas_tiling.size * canvas_tiling.atlas.cols,
            canvas_tiling.size * canvas_tiling.atlas.rows,
            canvas_tiling.atlas.layers, // Make it an array
            GpuTexture::ATLAS_USAGE,
        );

        Ok((
            ProcreateFile {
                composite: self
                    .composite
                    .load(dispatch, &atlas_texture, &self.info)
                    .ok(),
                layers: SilicaGroup {
                    hidden: false,
                    name: Some(String::from("Root Layer")),
                    children: {
                        self.layers
                            .into_par_iter()
                            .map(|ir| ir.load(dispatch, &atlas_texture, &self.info))
                            .collect::<Result<_, _>>()?
                    },
                },
                author_name: self.author_name,
                background_hidden: self.background_hidden,
                stroke_count: self.stroke_count,
                background_color: self.background_color,
                name: self.name,
                orientation: self.orientation,
                flipped: self.flipped,
                tile_size: self.tile_size,
                size: self.info.size,
                layer_count: self.layer_count,
            },
            ProcreateFileMetadata {
                atlas_texture,
                canvas_tiling,
            },
        ))
    }
}
