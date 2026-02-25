use crate::{
    gpu::{hierarchy::SilicaHierarchyGpu, layer::Addendum},
    info::group::SilicaGroup,
};

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
