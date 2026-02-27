use crate::{
    ns_archive::{NsClass, NsDecode, NsArchive, error::NsArchiveError},
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
        nka: &'a NsArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        let refs = nka.bind(<&'_ Dictionary>::decode(nka, key, val)?);
        let class = refs.resolve::<NsClass>("$class")?;

        match class.class_name.as_str() {
            "SilicaGroup" => Ok(SilicaGroup::decode(nka, key, val).map(Self::Group)?),
            "SilicaLayer" => Ok(SilicaLayer::decode(nka, key, val).map(Self::Layer)?),
            _ => Err(NsArchiveError::TypeMismatch("$class".to_string())),
        }
    }
}
