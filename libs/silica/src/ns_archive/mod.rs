pub mod error;

use error::NsArchiveError;
use plist::{Dictionary, Uid, Value};

pub struct NsKeyedArchive {
    #[allow(dead_code)]
    version: u64,
    #[allow(dead_code)]
    archiver: String,
    top: Dictionary,
    objects: Vec<Value>,
}

impl<'a> NsKeyedArchive {
    pub fn from_reader(reader: impl std::io::Read + std::io::Seek) -> Result<Self, NsArchiveError> {
        let mut value = plist::Value::from_reader(reader)?
            .into_dictionary()
            .ok_or(NsArchiveError::TypeMismatch(String::new()))?;

        Ok(Self {
            version: value
                .remove("$version")
                .ok_or_else(|| NsArchiveError::MissingKey("$version".to_string()))?
                .as_unsigned_integer()
                .ok_or_else(|| NsArchiveError::TypeMismatch("$version".to_string()))?,
            archiver: value
                .remove("$archiver")
                .ok_or_else(|| NsArchiveError::MissingKey("$archiver".to_string()))?
                .into_string()
                .ok_or_else(|| NsArchiveError::TypeMismatch("$archiver".to_string()))?,
            top: value
                .remove("$top")
                .ok_or_else(|| NsArchiveError::MissingKey("$top".to_string()))?
                .into_dictionary()
                .ok_or_else(|| NsArchiveError::TypeMismatch("$top".to_string()))?,
            objects: value
                .remove("$objects")
                .ok_or_else(|| NsArchiveError::MissingKey("$objects".to_string()))?
                .into_array()
                .ok_or_else(|| NsArchiveError::TypeMismatch("$objects".to_string()))?,
        })
    }

    fn resolve_index_nullable(&'a self, idx: usize) -> Result<Option<&'a Value>, NsArchiveError> {
        if idx == 0 {
            Ok(None)
        } else {
            self.objects
                .get(idx)
                .ok_or(NsArchiveError::BadIndex)
                .map(Some)
        }
    }

    fn resolve_index(&'a self, idx: usize) -> Result<&'a Value, NsArchiveError> {
        if idx == 0 {
            Err(NsArchiveError::BadIndex)
        } else {
            self.objects.get(idx).ok_or(NsArchiveError::BadIndex)
        }
    }

    pub fn fetch_value_nullable(
        &'a self,
        world: &'a Dictionary,
        key: &str,
    ) -> Result<Option<&'a Value>, NsArchiveError> {
        return match world.get(key) {
            Some(Value::Uid(uid)) => self.resolve_index_nullable(uid.get() as usize),
            value => Ok(value),
        };
    }

    pub fn fetch_value(
        &'a self,
        world: &'a Dictionary,
        key: &str,
    ) -> Result<&'a Value, NsArchiveError> {
        return match world.get(key) {
            Some(Value::Uid(uid)) => self.resolve_index(uid.get() as usize),
            Some(value) => Ok(value),
            None => Err(NsArchiveError::MissingKey(key.to_string())),
        };
    }

    pub fn fetch<T: NsDecode<'a>>(
        &'a self,
        world: &'a Dictionary,
        key: &'a str,
    ) -> Result<T, NsArchiveError> {
        T::fetch(self, world, key)
    }

    pub fn root(&self) -> Result<&'_ Dictionary, NsArchiveError> {
        self.fetch::<&'_ Dictionary>(&self.top, "root")
    }
}

pub trait NsDecode<'a>: Sized {
    fn fetch(
        nka: &'a NsKeyedArchive,
        world: &'a Dictionary,
        key: &'a str,
    ) -> Result<Self, NsArchiveError> {
        Self::decode(nka, key, nka.fetch_value(world, key)?)
    }

    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError>;
}

impl NsDecode<'_> for bool {
    fn decode(_: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        val.as_boolean()
            .ok_or_else(|| NsArchiveError::TypeMismatch(key.to_string()))
    }
}

impl NsDecode<'_> for usize {
    fn decode(_: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        val.as_unsigned_integer()
            .ok_or_else(|| NsArchiveError::TypeMismatch(key.to_string()))
            .map(|n| n as Self)
    }
}

impl NsDecode<'_> for isize {
    fn decode(_: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        val.as_signed_integer()
            .ok_or_else(|| NsArchiveError::TypeMismatch(key.to_string()))
            .map(|n| n as Self)
    }
}

impl NsDecode<'_> for u64 {
    fn decode(_: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        val.as_unsigned_integer()
            .ok_or_else(|| NsArchiveError::TypeMismatch(key.to_string()))
    }
}

impl NsDecode<'_> for i64 {
    fn decode(_: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        val.as_signed_integer()
            .ok_or_else(|| NsArchiveError::TypeMismatch(key.to_string()))
    }
}

impl NsDecode<'_> for f64 {
    fn decode(_: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        val.as_real()
            .ok_or_else(|| NsArchiveError::TypeMismatch(key.to_string()))
    }
}

impl NsDecode<'_> for u32 {
    fn decode(nka: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        u32::try_from(u64::decode(nka, key, val)?)
            .map_err(|_| NsArchiveError::TypeMismatch(key.to_string()))
    }
}

impl NsDecode<'_> for i32 {
    fn decode(nka: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        i32::try_from(i64::decode(nka, key, val)?)
            .map_err(|_| NsArchiveError::TypeMismatch(key.to_string()))
    }
}

impl NsDecode<'_> for f32 {
    fn decode(nka: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        f64::decode(nka, key, val).map(|v| v as Self)
    }
}

impl<'a> NsDecode<'a> for &'a Dictionary {
    fn decode(_: &NsKeyedArchive, key: &str, val: &'a Value) -> Result<Self, NsArchiveError> {
        val.as_dictionary()
            .ok_or_else(|| NsArchiveError::TypeMismatch(key.to_string()))
    }
}

impl<'a> NsDecode<'a> for &'a Value {
    fn decode(_: &NsKeyedArchive, _: &str, val: &'a Value) -> Result<Self, NsArchiveError> {
        Ok(val)
    }
}

impl<'a> NsDecode<'a> for &'a [u8] {
    fn decode(_: &NsKeyedArchive, key: &str, val: &'a Value) -> Result<Self, NsArchiveError> {
        val.as_data()
            .ok_or_else(|| NsArchiveError::TypeMismatch(key.to_string()))
    }
}

impl NsDecode<'_> for Uid {
    fn decode(_: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        val.as_uid()
            .copied()
            .ok_or_else(|| NsArchiveError::TypeMismatch(key.to_string()))
    }
}

impl<'a> NsDecode<'a> for &'a str {
    fn decode(_: &NsKeyedArchive, key: &str, val: &'a Value) -> Result<Self, NsArchiveError> {
        val.as_string()
            .ok_or_else(|| NsArchiveError::TypeMismatch(key.to_string()))
    }
}

impl<'a> NsDecode<'a> for String {
    fn decode(a: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        if let Ok(s) = NsString::decode(a, key, val) {
            return Ok(s.string);
        }
        Ok(<&'_ str>::decode(a, key, val)?.to_owned())
    }
}

impl<'a, T> NsDecode<'a> for Box<T>
where
    T: NsDecode<'a>,
{
    fn fetch(
        nka: &'a NsKeyedArchive,
        world: &'a Dictionary,
        key: &'a str,
    ) -> Result<Self, NsArchiveError> {
        Ok(Box::new(T::fetch(nka, world, key)?))
    }

    fn decode(a: &'a NsKeyedArchive, key: &'a str, val: &'a Value) -> Result<Self, NsArchiveError> {
        Ok(Box::new(T::decode(a, key, val)?))
    }
}

impl<'a, T> NsDecode<'a> for Option<T>
where
    T: NsDecode<'a>,
{
    fn fetch(
        nka: &'a NsKeyedArchive,
        world: &'a Dictionary,
        key: &'a str,
    ) -> Result<Self, NsArchiveError> {
        nka.fetch_value_nullable(world, key)?
            .map(|z| T::decode(nka, key, z))
            .transpose()
    }

    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        Ok(Some(T::decode(nka, key, val)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}

impl<T: std::str::FromStr> Size<T> {
    fn parse_size_str(size_str: &str, key: &str) -> Result<(T, T), NsArchiveError> {
        if !size_str.starts_with('{') || !size_str.ends_with('}') {
            Err(NsArchiveError::TypeMismatch(key.to_string()))
        } else {
            let separator_index = size_str
                .find(',')
                .ok_or_else(|| NsArchiveError::TypeMismatch(key.to_string()))?;
            let width = size_str[1..separator_index]
                .trim()
                .parse::<T>()
                .map_err(|_| NsArchiveError::TypeMismatch(key.to_string()))?;
            let height = size_str[separator_index + 1..size_str.len() - 1]
                .trim()
                .parse::<T>()
                .map_err(|_| NsArchiveError::TypeMismatch(key.to_string()))?;

            Ok((width, height))
        }
    }
}

impl<T: std::str::FromStr> NsDecode<'_> for Size<T> {
    fn decode(nka: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        let size_str = <&'_ str>::decode(nka, key, val)?;
        let (width, height) = Self::parse_size_str(size_str, key)?;
        Ok(Size { width, height })
    }
}

impl<'a, T> NsDecode<'a> for Vec<T>
where
    T: NsDecode<'a>,
{
    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        val.as_array()
            .ok_or_else(|| NsArchiveError::TypeMismatch(key.to_string()))?
            .iter()
            .map(|val| T::decode(nka, key, val))
            .collect::<Result<Vec<_>, _>>()
    }
}

#[derive(Debug)]
pub struct NsObjects<T> {
    pub objects: Vec<T>,
}

impl<'a, T> NsDecode<'a> for NsObjects<T>
where
    T: NsDecode<'a>,
{
    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        struct NsObjectIds {
            pub inner: Vec<Uid>,
        }

        impl NsDecode<'_> for NsObjectIds {
            fn decode(
                nka: &NsKeyedArchive,
                key: &str,
                val: &Value,
            ) -> Result<Self, NsArchiveError> {
                let world = <&'_ Dictionary>::decode(nka, key, val)?;
                Ok(Self {
                    inner: nka.fetch::<Vec<Uid>>(world, "NS.objects")?,
                })
            }
        }

        Ok(Self {
            objects: NsObjectIds::decode(nka, key, val)?
                .inner
                .iter()
                .map(|uid| T::decode(nka, key, nka.resolve_index(uid.get() as usize)?))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

#[derive(Debug)]
pub struct NsClass {
    pub class_name: String,
    #[allow(dead_code)]
    pub classes: Vec<String>,
}

impl NsDecode<'_> for NsClass {
    fn decode(nka: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        let coder = <&'_ Dictionary>::decode(nka, key, val)?;
        Ok(Self {
            class_name: nka.fetch::<String>(coder, "$classname")?,
            classes: nka.fetch::<Vec<String>>(coder, "$classes")?,
        })
    }
}

#[derive(Debug)]
pub struct NsString {
    #[allow(dead_code)]
    pub class: NsClass,
    pub string: String,
}

impl NsDecode<'_> for NsString {
    fn decode(nka: &NsKeyedArchive, key: &str, val: &Value) -> Result<Self, NsArchiveError> {
        let coder = <&'_ Dictionary>::decode(nka, key, val)?;
        Ok(Self {
            class: nka.fetch::<NsClass>(coder, "$class")?,
            string: nka.fetch::<String>(coder, "NS.string")?,
        })
    }
}
