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
    pub info: silica::SilicaGroup,
    pub children: Vec<SilicaHierarchy>,
    pub addendum: Addendum,
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
        meta: &'a LoadParams<'a>,
    ) -> Result<SilicaGroup, SilicaError> {
        Ok(SilicaGroup {
            children: info
                .children
                .par_drain(..)
                .map(|ir| SilicaHierarchy::load(ir, queue, atlas_texture, meta))
                .collect::<Result<Vec<_>, _>>()?,
            info,
            addendum: Addendum {
                id: meta.addendum_id_counter.fetch_add(1, Ordering::AcqRel),
            },
        })
    }
}
