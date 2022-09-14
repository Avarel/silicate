/// This module computes how to bind the compositor layers to the shader
/// variables.

use super::{tex::GpuTexture, dev::LogicalDevice};
use crate::compositor::CompositeLayer;
use std::collections::HashMap;

#[derive(Debug)]
pub struct CpuBindings {
    chunks: u32,
    blends: Box<[u32]>,
    opacities: Box<[f32]>,
    masks: Box<[u32]>,
    layers: Box<[u32]>,
    pub(super) count: u32,
}

impl CpuBindings {
    const MASK_NONE: u32 = u32::MAX;

    pub fn new(chunks: u32) -> Self {
        let chunks = chunks as usize;

        Self {
            chunks: chunks as u32,
            blends: vec![0; chunks].into_boxed_slice(),
            opacities: vec![0.0f32; chunks].into_boxed_slice(),
            masks: vec![0; chunks].into_boxed_slice(),
            layers: vec![0; chunks].into_boxed_slice(),
            count: 0u32,
        }
    }

    fn reset(&mut self) {
        self.blends.fill(0);
        self.opacities.fill(0.0);
        self.masks.fill(Self::MASK_NONE);
        self.layers.fill(0);
        self.count = 0;
    }

    pub fn map_composite_layers(
        &mut self,
        composite_layers: &[CompositeLayer],
        textures: &[GpuTexture],
    ) -> Box<[wgpu::TextureView]> {
        self.reset();
        BindingMapper::new(self.chunks, composite_layers, textures, self).map()
    }
}

struct BindingMapper<'dev> {
    chunks: u32,
    output: Box<[wgpu::TextureView]>,
    map: HashMap<usize, u32>,
    composite_layers: &'dev [CompositeLayer],
    textures: &'dev [GpuTexture],
    bindings: &'dev mut CpuBindings,
}

impl<'dev> BindingMapper<'dev> {
    fn new(
        chunks: u32,
        composite_layers: &'dev [CompositeLayer],
        textures: &'dev [GpuTexture],
        bindings: &'dev mut CpuBindings,
    ) -> Self {
        Self {
            chunks,
            output: {
                let mut views = Vec::new();
                views.resize_with(chunks as usize, || textures[0].make_view());
                views.into_boxed_slice()
            },
            map: HashMap::new(),
            composite_layers,
            textures,
            bindings,
        }
    }

    fn map_texture(&mut self, texture_index: usize) -> u32 {
        let mlen = self.map.len() as u32;
        *self.map.entry(texture_index).or_insert_with(|| {
            self.output[mlen as usize] = self.textures[texture_index].make_view();
            mlen
        })
    }

    fn map(mut self) -> Box<[wgpu::TextureView]> {
        for (index, layer) in self.composite_layers.into_iter().enumerate() {
            debug_assert_eq!(index, self.bindings.count as usize);

            if let Some(clip_layer) = layer.clipped {
                if (self.map.len() as u32) + 2 > self.chunks {
                    break;
                }
                self.bindings.masks[index] = self.map_texture(clip_layer);
                self.bindings.layers[index] = self.map_texture(layer.texture);
            } else {
                if (self.map.len() as u32) + 1 > self.chunks {
                    break;
                }
                self.bindings.masks[index] = CpuBindings::MASK_NONE;
                self.bindings.layers[index] = self.map_texture(layer.texture);

            }

            self.bindings.blends[index] = layer.blend.to_u32();
            self.bindings.opacities[index] = layer.opacity;
            self.bindings.count += 1;
        }

        self.output
    }
}


pub(super) struct GpuBuffers<'dev> {
    dev: &'dev LogicalDevice,
    pub(super) blends: wgpu::Buffer,
    pub(super) opacities: wgpu::Buffer,
    pub(super) masks: wgpu::Buffer,
    pub(super) layers: wgpu::Buffer,
    pub(super) count: wgpu::Buffer,
}

impl<'dev> GpuBuffers<'dev> {
    pub fn new(dev: &'dev LogicalDevice) -> Self {
        let chunks = u64::from(dev.chunks);
        let storage_desc: wgpu::BufferDescriptor = wgpu::BufferDescriptor {
            label: None,
            size: 4 * chunks,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        };
        GpuBuffers {
            dev,
            blends: dev.device.create_buffer(&storage_desc),
            opacities: dev.device.create_buffer(&storage_desc),
            masks: dev.device.create_buffer(&storage_desc),
            layers: dev.device.create_buffer(&storage_desc),
            count: dev.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                size: 4,
                mapped_at_creation: false,
            }),
        }
    }

    pub fn load(&self, cpu: &CpuBindings) {
        let q = &self.dev.queue;
        q.write_buffer(&self.blends, 0, bytemuck::cast_slice(&cpu.blends));
        q.write_buffer(&self.opacities, 0, bytemuck::cast_slice(&cpu.opacities));
        q.write_buffer(&self.masks, 0, bytemuck::cast_slice(&cpu.masks));
        q.write_buffer(&self.layers, 0, bytemuck::cast_slice(&cpu.layers));
        q.write_buffer(&self.count, 0, &cpu.count.to_ne_bytes());
    }
}
