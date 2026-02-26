use plist::{Dictionary, Value};

use crate::{
    ns_archive::{NsDecode, NsKeyedArchive, NsObjects, error::NsArchiveError},
    types::hierarchy::SilicaHierarchy,
};

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaGroup {
    pub name: Option<String>,
    pub hidden: bool,
    pub children: Vec<SilicaHierarchy>,
}

impl<'a> NsDecode<'a> for SilicaGroup {
    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        let coder = <&'a Dictionary>::decode(nka, key, val)?;
        Ok(Self {
            hidden: nka.fetch::<bool>(coder, "isHidden")?,
            name: nka.fetch::<Option<String>>(coder, "name")?,
            children: nka
                .fetch::<NsObjects<SilicaHierarchy>>(coder, "children")?
                .objects,
        })
    }
}
