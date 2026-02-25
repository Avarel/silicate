use crate::gpu::{group::SilicaGroupGpu, layer::SilicaLayerGpu};

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
}
