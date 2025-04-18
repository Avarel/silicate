use crate::ns_archive::{error::NsArchiveError, NsKeyedArchive};


#[derive(Debug, Clone, Copy)]
pub struct Flipped {
    pub horizontally: bool,
    pub vertically: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    NoRotation,
    Clockwise180,
    Clockwise270,
    Clockwise90,
    Unknown,
}

impl crate::ns_archive::NsDecode<'_> for Orientation {
    fn decode(nka: &NsKeyedArchive, key: &str, val: &plist::Value) -> Result<Self, NsArchiveError> {
        Ok(match u64::decode(nka, key, val)? {
            1 => Self::NoRotation,
            2 => Self::Clockwise180,
            3 => Self::Clockwise270,
            4 => Self::Clockwise90,
            v => Err(NsArchiveError::BadValue(key.to_string(), v.to_string()))?,
        })
    }
}
