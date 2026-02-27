use crate::data::BlendingMode;
use crate::ns_archive::{NsArchive, NsDecode, NsRefDictionary, error::NsArchiveError};
use plist::{Dictionary, Value};

impl<'a> NsDecode<'a> for BlendingMode {
    fn resolve(refs: &NsRefDictionary<'a>, key: &'a str) -> Result<Self, NsArchiveError> {
        assert!(key == "extendedBlend" || key == "blend");

        let val = refs
            .resolve_value_nullable("extendedBlend")
            .transpose()
            .unwrap_or_else(|| refs.resolve_value("blend"))?;
        Self::decode(refs.archive(), "extendedBlend", val)
    }

    fn decode(nka: &'a NsArchive, key: &'a str, val: &'a Value) -> Result<Self, NsArchiveError> {
        BlendingMode::from_u32(u32::decode(nka, key, val)?)
            .ok_or_else(|| NsArchiveError::TypeMismatch(String::from(key)))
    }
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
    pub uuid: String,
    pub version: u64,
}

impl<'a> NsDecode<'a> for SilicaLayer {
    fn decode(nka: &'a NsArchive, key: &'a str, val: &'a Value) -> Result<Self, NsArchiveError> {
        let refs = nka.bind(<&'_ Dictionary>::decode(nka, key, val)?);
        let uuid = refs.resolve::<String>("UUID")?;

        Ok(Self {
            blend: refs
                .resolve::<BlendingMode>("extendedBlend")
                .or_else(|_| refs.resolve::<BlendingMode>("blend"))?,
            clipped: refs.resolve::<bool>("clipped")?,
            hidden: refs.resolve::<bool>("hidden")?,
            mask: refs.resolve::<Option<Box<SilicaLayer>>>("mask")?,
            name: refs.resolve::<Option<String>>("name")?,
            opacity: refs.resolve::<f32>("opacity")?,
            uuid,
            version: refs.resolve::<u64>("version")?,
        })
    }
}
