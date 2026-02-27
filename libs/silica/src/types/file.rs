use crate::{
    data::{Flipped, Orientation},
    error::SilicaError,
    ns_archive::{NsArchive, NsObjects, Size, error::NsArchiveError},
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
    pub fn from_ns<'a>(nka: &'a NsArchive) -> Result<Self, SilicaError> {
        let refs = nka.bind(nka.root()?);

        let size = refs.resolve::<Size<u32>>("size")?;
        let tile_size = refs.resolve::<u32>("tileSize")?;

        let layers = refs
            .resolve::<NsObjects<SilicaHierarchy>>("unwrappedLayers")?
            .objects;

        Ok(Self {
            author_name: refs.resolve::<Option<String>>("authorName")?,
            background_hidden: refs.resolve::<bool>("backgroundHidden")?,
            stroke_count: refs.resolve::<usize>("strokeCount")?,
            background_color: <[f32; 4]>::try_from(
                refs.resolve::<&[u8]>("backgroundColor")?
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
            name: refs.resolve::<Option<String>>("name")?,
            orientation: refs.resolve::<Orientation>("orientation")?,
            flipped: Flipped {
                horizontally: refs.resolve::<bool>("flippedHorizontally")?,
                vertically: refs.resolve::<bool>("flippedVertically")?,
            },
            tile_size,
            composite: Some(refs.resolve::<SilicaLayer>("composite")?),
            layers,
            size,
        })
    }
}
