use silica::info::group::SilicaGroup as SilicaGroupRaw;
use silicate_compositor::{dev::GpuDispatch, tex::GpuTexture};

use crate::{
    error::SilicaError,
    {hierarchy::SilicaHierarchyGpu, ir::IRData, layer::Addendum},
};

use std::sync::atomic::Ordering;

use rayon::iter::ParallelDrainRange;
use rayon::prelude::ParallelIterator;

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaGroupGpu {
    pub info: SilicaGroupRaw,
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

    pub(crate) fn load<'a>(
        mut info: SilicaGroupRaw,
        dispatch: &GpuDispatch,
        atlas_texture: &'a GpuTexture,
        meta: &'a IRData<'a>,
    ) -> Result<SilicaGroupGpu, SilicaError> {
        Ok(SilicaGroupGpu {
            children: info
                .children
                .par_drain(..)
                .map(|ir| SilicaHierarchyGpu::load(ir, dispatch, atlas_texture, meta))
                .collect::<Result<Vec<_>, _>>()?,
            info,
            addendum: Addendum {
                id: meta.addendum_id_counter.fetch_add(1, Ordering::AcqRel),
            },
        })
    }
}
