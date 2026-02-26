use crate::{
    data::{Flipped, Orientation},
    error::SilicaError,
    ns_archive::{NsKeyedArchive, NsObjects, Size, error::NsArchiveError},
    types::{hierarchy::SilicaHierarchy, layer::SilicaLayer},
};

#[derive(Debug, Clone, PartialEq)]
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

    pub size: Size<u32>,

    pub layers: Vec<SilicaHierarchy>,
    pub composite: Option<SilicaLayer>,
}

impl ProcreateFile {
    pub fn from_ns<'a>(nka: &'a NsKeyedArchive) -> Result<Self, SilicaError> {
        let root = nka.root()?;

        let size = nka.fetch::<Size<u32>>(root, "size")?;
        let tile_size = nka.fetch::<u32>(root, "tileSize")?;

        let layers = nka
            .fetch::<NsObjects<SilicaHierarchy>>(root, "unwrappedLayers")?
            .objects;

        Ok(Self {
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
            composite: Some(nka.fetch::<SilicaLayer>(root, "composite")?),
            layers,
            size,
        })
    }
}
