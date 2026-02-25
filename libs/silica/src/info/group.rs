use std::sync::atomic::Ordering;

use plist::{Dictionary, Value};
use rayon::iter::ParallelDrainRange;
use rayon::prelude::ParallelIterator;
use silicate_compositor::{dev::GpuDispatch, tex::GpuTexture};

use crate::{
    error::SilicaError,
    gpu::{group::SilicaGroupGpu, ir::IRData, layer::Addendum},
    info::hierarchy::SilicaHierarchy,
    ns_archive::{NsDecode, NsKeyedArchive, NsObjects, error::NsArchiveError},
};

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaGroup {
    pub name: Option<String>,
    pub hidden: bool,
    pub(crate) children: Vec<SilicaHierarchy>,
}

impl<'a> NsDecode<'a> for SilicaGroup {
    fn decode(
        nka: &'a NsKeyedArchive,
        key: &'a str,
        val: &'a Value,
    ) -> Result<Self, NsArchiveError> {
        let coder = <&'a Dictionary>::decode(nka, key, val)?;
        Ok(Self {
            hidden: nka.fetch::<bool>(coder, "isHidden")?,
            name: nka.fetch::<Option<String>>(coder, "name")?,
            children: nka
                .fetch::<NsObjects<SilicaHierarchy>>(coder, "children")?
                .objects,
        })
    }
}

impl<'a> SilicaGroup {
    pub(crate) fn load(
        mut self,
        dispatch: &GpuDispatch,
        atlas_texture: &'a GpuTexture,
        meta: &'a IRData<'a>,
    ) -> Result<SilicaGroupGpu, SilicaError> {
        Ok(SilicaGroupGpu {
            children: self
                .children
                .par_drain(..)
                .map(|ir| ir.load(dispatch, atlas_texture, meta))
                .collect::<Result<Vec<_>, _>>()?,
            info: self,
            addendum: Addendum {
                id: meta.addendum_id_counter.fetch_add(1, Ordering::AcqRel),
            },
        })
    }
}
