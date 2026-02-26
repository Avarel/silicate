use crate::{
    error::SilicaError,
    params::LoadParams,
    types::{hierarchy::SilicaHierarchy, layer::Addendum},
};

use std::sync::atomic::Ordering;

use rayon::iter::ParallelDrainRange;
use rayon::prelude::ParallelIterator;

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaGroup {
    info: silica::SilicaGroup,
    pub children: Vec<SilicaHierarchy>,
    pub addendum: Addendum,
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
        queue: &wgpu::Queue,
        atlas_texture: &wgpu::Texture,
        params: &'a LoadParams<'a>,
    ) -> Result<SilicaGroup, SilicaError> {
        Ok(SilicaGroup {
            children: info
                .children
                .par_drain(..)
                .map(|ir| SilicaHierarchy::load(ir, queue, atlas_texture, params))
                .collect::<Result<Vec<_>, _>>()?,
            info,
            addendum: Addendum {
                id: params.addendum_id_counter.fetch_add(1, Ordering::AcqRel),
            },
        })
    }
}
