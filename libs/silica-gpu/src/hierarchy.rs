use crate::error::SilicaError;
use crate::ir::IRData;
use crate::{group::SilicaGroupGpu, layer::SilicaLayerGpu};
use silica::info::hierarchy::SilicaHierarchy as SilicaHierarchyRaw;
use silicate_compositor::dev::GpuDispatch;
use silicate_compositor::tex::GpuTexture;

#[derive(Debug, Clone, PartialEq)]
pub enum SilicaHierarchyGpu {
    Layer(SilicaLayerGpu),
    Group(SilicaGroupGpu),
}

impl SilicaHierarchyGpu {
    pub fn layer_count(&self, include_groups: bool) -> u32 {
        match self {
            SilicaHierarchyGpu::Layer(_) => 1,
            SilicaHierarchyGpu::Group(silica_group) => silica_group.layer_count(include_groups),
        }
    }

    pub(crate) fn load<'a>(
        info: SilicaHierarchyRaw,
        dispatch: &GpuDispatch,
        atlas_texture: &'a GpuTexture,
        meta: &'a IRData<'a>,
    ) -> Result<SilicaHierarchyGpu, SilicaError> {
        Ok(match info {
            SilicaHierarchyRaw::Layer(layer) => SilicaHierarchyGpu::Layer(SilicaLayerGpu::load(
                layer,
                dispatch,
                atlas_texture,
                meta,
            )?),
            SilicaHierarchyRaw::Group(group) => SilicaHierarchyGpu::Group(SilicaGroupGpu::load(
                group,
                dispatch,
                atlas_texture,
                meta,
            )?),
        })
    }
}
