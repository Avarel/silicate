use crate::error::SilicaError;
use crate::gpu::hierarchy::SilicaHierarchyGpu;
use crate::gpu::ir::IRData;
use crate::info::group::SilicaGroup;
use crate::info::layer::SilicaLayer;
use crate::ns_archive::{NsClass, NsDecode};
use crate::ns_archive::{NsKeyedArchive, error::NsArchiveError};
use plist::{Dictionary, Value};
use silicate_compositor::dev::GpuDispatch;
use silicate_compositor::tex::GpuTexture;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SilicaHierarchy {
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

impl<'a> SilicaHierarchy {
    pub(crate) fn load(
        self,
        dispatch: &GpuDispatch,
        atlas_texture: &'a GpuTexture,
        meta: &'a IRData<'a>,
    ) -> Result<SilicaHierarchyGpu, SilicaError> {
        Ok(match self {
            SilicaHierarchy::Layer(layer) => {
                SilicaHierarchyGpu::Layer(layer.load(dispatch, atlas_texture, meta)?)
            }
            SilicaHierarchy::Group(group) => {
                SilicaHierarchyGpu::Group(group.load(dispatch, atlas_texture, meta)?)
            }
        })
    }
}
