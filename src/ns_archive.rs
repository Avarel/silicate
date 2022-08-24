use once_cell::sync::OnceCell;
use plist::{Dictionary, Uid, Value};
use regex::Regex;
use serde::Deserialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NsArchiveError {
    #[error("i/o error")]
    Io(#[from] std::io::Error),
    #[error("plist error")]
    PlistError(#[from] plist::Error),
    #[error("zip error")]
    ZipError(#[from] zip::result::ZipError),
    #[error("type mismatch")]
    TypeMismatch,
    #[error("missing key")]
    MissingKey,
    #[error("bad index")]
    BadIndex,
}

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct NsKeyedArchive {
    #[serde(rename = "$version")]
    version: usize,
    #[serde(rename = "$archiver")]
    archiver: String,
    #[serde(rename = "$top")]
    top: Dictionary,
    #[serde(rename = "$objects")]
    objects: Vec<Value>,
}

impl<'a> NsKeyedArchive {
    pub fn resolve_index(&'a self, idx: usize) -> Result<Option<&'a Value>, NsArchiveError> {
        if idx == 0 {
            Ok(None)
        } else {
            self.objects
                .get(idx)
                .ok_or(NsArchiveError::BadIndex)
                .map(Some)
        }
    }

    pub fn decode_value(
        &'a self,
        coder: &'a Dictionary,
        key: &str,
    ) -> Result<Option<&'a Value>, NsArchiveError> {
        return match coder.get(key) {
            Some(Value::Uid(uid)) => self.resolve_index(uid.get() as usize),
            value @ _ => Ok(value),
        };
    }

    pub fn decode<T: NsDecode<'a>>(
        &'a self,
        coder: &'a Dictionary,
        key: &str,
    ) -> Result<T, NsArchiveError> {
        T::decode(self, self.decode_value(coder, key)?)
    }

    pub fn root(&self) -> Result<&'_ Dictionary, NsArchiveError> {
        self.decode::<&'_ Dictionary>(&self.top, "root")
    }
}

pub trait NsDecode<'a>: Sized {
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError>;
}

impl NsDecode<'_> for bool {
    fn decode(_: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_boolean()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl NsDecode<'_> for usize {
    fn decode(_: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_unsigned_integer()
            .ok_or(NsArchiveError::TypeMismatch)
            .map(|n| n as Self)
    }
}

impl NsDecode<'_> for isize {
    fn decode(_: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_signed_integer()
            .ok_or(NsArchiveError::TypeMismatch)
            .map(|n| n as Self)
    }
}

impl NsDecode<'_> for u64 {
    fn decode(_: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_unsigned_integer()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl NsDecode<'_> for i64 {
    fn decode(_: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_signed_integer()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl NsDecode<'_> for f64 {
    fn decode(_: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_real()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl NsDecode<'_> for u32 {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        u32::try_from(u64::decode(nka, val)?).map_err(|_| NsArchiveError::TypeMismatch)
    }
}

impl NsDecode<'_> for i32 {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        i32::try_from(i64::decode(nka, val)?).map_err(|_| NsArchiveError::TypeMismatch)
    }
}

impl NsDecode<'_> for f32 {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        f64::decode(nka, val).map(|v| v as Self)
    }
}

impl<'a> NsDecode<'a> for &'a Dictionary {
    fn decode(_: &NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_dictionary()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl<'a> NsDecode<'a> for &'a Value {
    fn decode(_: &NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)
    }
}

impl<'a> NsDecode<'a> for &'a [u8] {
    fn decode(_: &NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_data()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl NsDecode<'_> for Uid {
    fn decode(_: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_uid()
            .copied()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl<'a> NsDecode<'a> for &'a str {
    fn decode(_: &NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        val.ok_or(NsArchiveError::MissingKey)?
            .as_string()
            .ok_or(NsArchiveError::TypeMismatch)
    }
}

impl<'a> NsDecode<'a> for String {
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        Ok(<&'_ str>::decode(nka, val)?.to_owned())
    }
}
impl<'a, T> NsDecode<'a> for Option<T>
where
    T: NsDecode<'a>,
{
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        val.map_or(Ok(None), |a| Some(T::decode(nka, Some(a))).transpose())
    }
}

impl<'a, T> NsDecode<'a> for Box<T>
where
    T: NsDecode<'a>,
{
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        Ok(Box::new(T::decode(nka, val)?))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}
use std::str::FromStr;
impl<T: FromStr> NsDecode<'_> for Size<T> {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        let string = <&'_ str>::decode(nka, val)?;

        static INSTANCE: OnceCell<Regex> = OnceCell::new();
        let size_regex = INSTANCE.get_or_init(|| Regex::new("\\{(\\d+), ?(\\d+)\\}").unwrap());
        let captures = size_regex
            .captures(string)
            .ok_or(NsArchiveError::TypeMismatch)?;

        let width = captures[1]
            .parse::<T>()
            .map_err(|_| NsArchiveError::TypeMismatch)?;
        let height = captures[2]
            .parse::<T>()
            .map_err(|_| NsArchiveError::TypeMismatch)?;
        Ok(Size { width, height })
    }
}

impl<'a, T> NsDecode<'a> for Vec<T>
where
    T: NsDecode<'a>,
{
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        Ok(val
            .ok_or(NsArchiveError::MissingKey)?
            .as_array()
            .ok_or(NsArchiveError::TypeMismatch)?
            .iter()
            .map(|val| T::decode(nka, Some(val)))
            .collect::<Result<Vec<_>, _>>()?)
    }
}

#[derive(Debug)]
pub struct WrappedArray<T> {
    pub objects: Vec<T>,
}

impl<'a, T> NsDecode<'a> for WrappedArray<T>
where
    T: NsDecode<'a>,
{
    fn decode(nka: &'a NsKeyedArchive, val: Option<&'a Value>) -> Result<Self, NsArchiveError> {
        Ok(Self {
            objects: WrappedRawArray::decode(nka, val)?
                .inner
                .iter()
                .map(|uid| {
                    let val = nka
                        .resolve_index(uid.get() as usize)?
                        .ok_or(NsArchiveError::BadIndex)?;
                    T::decode(nka, Some(val))
                })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

#[derive(Debug)]
pub struct WrappedRawArray {
    pub inner: Vec<Uid>,
}

impl NsDecode<'_> for WrappedRawArray {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        let coder = <&'_ Dictionary>::decode(nka, val)?;
        Ok(Self {
            inner: nka.decode::<Vec<Uid>>(coder, "NS.objects")?,
        })
    }
}

#[derive(Debug)]
pub struct NsClass {
    pub class_name: String,
    pub classes: Vec<String>,
}

impl NsDecode<'_> for NsClass {
    fn decode(nka: &NsKeyedArchive, val: Option<&Value>) -> Result<Self, NsArchiveError> {
        let coder = <&'_ Dictionary>::decode(nka, val)?;
        Ok(Self {
            class_name: nka.decode::<String>(coder, "$classname")?,
            classes: nka.decode::<Vec<String>>(coder, "$classes")?,
        })
    }
}
