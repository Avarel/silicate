use std::num::NonZeroU32;

use crate::{
    info::hierarchy::{SilicaGroup, SilicaLayer},
};

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

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaGroupGpu {
    pub info: SilicaGroup,
    pub children: Vec<SilicaHierarchyGpu>,
    pub addendum: Addendum,
}

impl SilicaGroupGpu {
    pub fn layer_count(&self, include_groups: bool) -> u32 {
        self.children
            .iter()
            .map(|hier| hier.layer_count(include_groups))
            .sum::<u32>()
            + u32::from(include_groups)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaChunk {
    pub col: u32,
    pub row: u32,
    pub atlas_index: NonZeroU32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaImageData {
    pub chunks: Vec<SilicaChunk>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaLayerGpu {
    pub info: SilicaLayer,
    pub image: SilicaImageData,
    pub addendum: Addendum,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Addendum {
    pub id: u32,
}
