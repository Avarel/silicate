use silica::layers::{SilicaGroup, SilicaHierarchy, SilicaLayer};

struct AddendumData {
    id_counter: u32,
}

impl AddendumData {
    pub fn new() -> Self {
        Self { id_counter: 0 }
    }

    pub fn next_id(&mut self) -> u32 {
        let id = self.id_counter;
        self.id_counter += 1;
        id
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SilicaHierarchyAddendum {
    Layer(SilicaLayerAddendum),
    Group(SilicaGroupAddendum),
}

impl SilicaHierarchyAddendum {
    fn build_addendum(hier: &SilicaHierarchy, data: &mut AddendumData) -> Self {
        match hier {
            SilicaHierarchy::Layer(layer) => {
                Self::Layer(SilicaLayerAddendum::build_addendum(layer, data))
            }
            SilicaHierarchy::Group(group) => {
                Self::Group(SilicaGroupAddendum::build_addendum(group, data))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaGroupAddendum {
    pub children: Vec<SilicaHierarchyAddendum>,
    pub id: u32,
}

impl SilicaGroupAddendum {
    fn build_addendum(group: &SilicaGroup, data: &mut AddendumData) -> Self {
        Self {
            id: data.next_id(),
            children: build_addendum(&group.children, data),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SilicaLayerAddendum {
    pub id: u32,
}

impl SilicaLayerAddendum {
    fn build_addendum(_: &SilicaLayer, data: &mut AddendumData) -> Self {
        Self { id: data.next_id() }
    }
}

fn build_addendum(
    layers: &[SilicaHierarchy],
    data: &mut AddendumData,
) -> Vec<SilicaHierarchyAddendum> {
    layers
        .iter()
        .map(|child| SilicaHierarchyAddendum::build_addendum(child, data))
        .collect()
}

pub fn build(layers: &[SilicaHierarchy]) -> Vec<SilicaHierarchyAddendum> {
    let mut data = AddendumData::new();
    build_addendum(layers, &mut data)
}
