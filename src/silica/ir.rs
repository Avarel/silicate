use crate::ns_archive::{NsArchiveError, NsClass, WrappedArray};
use crate::ns_archive::{NsDecode, NsKeyedArchive};
use plist::{Dictionary, Value};

pub(super) enum SilicaIRHierarchy<'a> {
    Layer(SilicaIRLayer<'a>),
    Group(SilicaIRGroup<'a>),
}

pub(super) struct SilicaIRLayer<'a> {
    pub(super) nka: &'a NsKeyedArchive,
    pub(super) coder: &'a Dictionary
}

impl<'a> NsDecode<'a> for SilicaIRLayer<'a> {
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        Ok(Self {
            nka,
            coder: <&'a Dictionary>::decode(nka, val)?,
        })
    }
}

pub(super) struct SilicaIRGroup<'a> {
    pub(super) nka: &'a NsKeyedArchive,
    pub(super) coder: &'a Dictionary,
    pub(super) children: Vec<SilicaIRHierarchy<'a>>,
}

impl<'a> NsDecode<'a> for SilicaIRGroup<'a> {
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        let coder = <&'a Dictionary>::decode(nka, val)?;
        Ok(Self {
            nka,
            coder, 
            children: nka
                .decode::<WrappedArray<SilicaIRHierarchy<'a>>>(coder, "children")?
                .objects,
        })
    }
}


impl<'a> NsDecode<'a> for SilicaIRHierarchy<'a> {
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        let coder = <&'a Dictionary>::decode(nka, val)?;
        let class = nka.decode::<NsClass>(coder, "$class")?;

        match class.class_name.as_str() {
            "SilicaGroup" => Ok(SilicaIRGroup::<'a>::decode(nka, val).map(Self::Group)?),
            "SilicaLayer" => Ok(SilicaIRLayer::<'a>::decode(nka, val).map(Self::Layer)?),
            _ => Err(NsArchiveError::TypeMismatch),
        }
    }
}
