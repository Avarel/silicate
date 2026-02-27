use plist::{Dictionary, Value};

use crate::{
    ns_archive::{NsArchive, NsDecode, NsObjects, error::NsArchiveError},
    types::hierarchy::SilicaHierarchy,
};

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaGroup {
    pub name: Option<String>,
    pub hidden: bool,
    pub children: Vec<SilicaHierarchy>,
}

impl<'a> NsDecode<'a> for SilicaGroup {
    fn decode(nka: &'a NsArchive, key: &'a str, val: &'a Value) -> Result<Self, NsArchiveError> {
        let refs = nka.bind(<&'_ Dictionary>::decode(nka, key, val)?);

        Ok(Self {
            hidden: refs.resolve::<bool>("isHidden")?,
            name: refs.resolve::<Option<String>>("name")?,
            children: refs
                .resolve::<NsObjects<SilicaHierarchy>>("children")?
                .objects,
        })
    }
}
