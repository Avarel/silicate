use crate::error::SilicaError;
use crate::params::LoadParams;
use crate::types::{group::SilicaGroup, layer::SilicaLayer};

#[derive(Debug, Clone, PartialEq)]
pub enum SilicaHierarchy {
    Layer(SilicaLayer),
    Group(SilicaGroup),
}

impl SilicaHierarchy {
    pub fn layer_count(&self, include_groups: bool) -> u32 {
        match self {
            SilicaHierarchy::Layer(_) => 1,
            SilicaHierarchy::Group(silica_group) => silica_group.layer_count(include_groups),
        }
    }

    pub(crate) fn load<'a>(
        info: silica::SilicaHierarchy,
        queue: &wgpu::Queue,
        atlas_texture: &'a wgpu::Texture,
        meta: &'a LoadParams<'a>,
    ) -> Result<SilicaHierarchy, SilicaError> {
        Ok(match info {
            silica::SilicaHierarchy::Layer(layer) => SilicaHierarchy::Layer(SilicaLayer::load(
                layer,
                queue,
                atlas_texture,
                meta,
            )?),
            silica::SilicaHierarchy::Group(group) => SilicaHierarchy::Group(SilicaGroup::load(
                group,
                queue,
                atlas_texture,
                meta,
            )?),
        })
    }
}
