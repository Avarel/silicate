use crate::ns_archive::{NsDecode, NsKeyedArchive, error::NsArchiveError};
use plist::{Dictionary, Value};
use crate::info::blend::BlendingMode;

impl<'a> NsDecode<'a> for BlendingMode {
    fn fetch(
        nka: &'a NsKeyedArchive,
        world: &'a Dictionary,
        key: &'a str,
    ) -> Result<Self, NsArchiveError> {
        assert!(key == "extendedBlend" || key == "blend");

        let val = nka
            .fetch_value_nullable(world, "extendedBlend")
            .transpose()
            .unwrap_or_else(|| nka.fetch_value(world, "blend"))?;
        Self::decode(nka, "extendedBlend", val)
    }

    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
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
    pub mask: Option<usize>,
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
    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        let world = <&'a Dictionary>::decode(nka, key, val)?;
        let uuid = nka.fetch::<String>(world, "UUID")?;

        Ok(Self {
            blend: nka
                .fetch::<BlendingMode>(world, "extendedBlend")
                .or_else(|_| nka.fetch::<BlendingMode>(world, "blend"))?,
            clipped: nka.fetch::<bool>(world, "clipped")?,
            hidden: nka.fetch::<bool>(world, "hidden")?,
            mask: None,
            name: nka.fetch::<Option<String>>(world, "name")?,
            opacity: nka.fetch::<f32>(world, "opacity")?,
            uuid,
            version: nka.fetch::<u64>(world, "version")?,
        })
    }
}
