use crate::{error::SilicaError, params::LoadParams, types::hierarchy::SilicaHierarchy};
#[cfg(not(target_arch = "wasm32"))]
use rayon::{iter::ParallelDrainRange, prelude::ParallelIterator};

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaGroup {
    info: silica::SilicaGroup,
    pub children: Vec<SilicaHierarchy>,
    pub id: u32,
}

impl std::ops::Deref for SilicaGroup {
    type Target = silica::SilicaGroup;

    fn deref(&self) -> &Self::Target {
        &self.info
    }
}

impl std::ops::DerefMut for SilicaGroup {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.info
    }
}

impl SilicaGroup {
    pub fn layer_count(&self, include_groups: bool) -> u32 {
        self.children
            .iter()
            .map(|hier| hier.layer_count(include_groups))
            .sum::<u32>()
            + u32::from(include_groups)
    }

    pub(crate) fn load<'a>(
        mut info: silica::SilicaGroup,
        params: &'a LoadParams<'a>,
    ) -> Result<SilicaGroup, SilicaError> {
        Ok(SilicaGroup {
            children: {
                #[cfg(not(target_arch = "wasm32"))]
                let iter = info.children.par_drain(..);
                #[cfg(target_arch = "wasm32")]
                let iter = info.children.drain(..);
                iter
            }
            .map(|ir| SilicaHierarchy::load(ir, params))
            .collect::<Result<Vec<_>, _>>()?,
            info,
            id: params.allocate_layer_id(),
        })
    }
}
