use crate::{
    ns_archive::{NsClass, NsDecode, NsKeyedArchive, error::NsArchiveError},
    types::{group::SilicaGroup, layer::SilicaLayer},
};
use plist::{Dictionary, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum SilicaHierarchy {
    Layer(SilicaLayer),
    Group(SilicaGroup),
}

impl<'a> NsDecode<'a> for SilicaHierarchy {
    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        let coder = <&'a Dictionary>::decode(nka, key, val)?;
        let class = nka.fetch::<NsClass>(coder, "$class")?;

        match class.class_name.as_str() {
            "SilicaGroup" => Ok(SilicaGroup::decode(nka, key, val).map(Self::Group)?),
            "SilicaLayer" => Ok(SilicaLayer::decode(nka, key, val).map(Self::Layer)?),
            _ => Err(NsArchiveError::TypeMismatch("$class".to_string())),
        }
    }
}
