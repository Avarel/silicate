/// This module computes how to bind the compositor layers to the shader
/// variables. It is configured specifically to serve the `shader.wgsl`
/// shader module and create bindings that match the shader's inputs.
use super::{dev::GpuHandle, tex::GpuTexture};
use crate::compositor::CompositeLayer;
use std::collections::HashMap;

/// Shader buffers on the CPU side.
#[derive(Debug)]
pub struct CpuBuffers {
    /// Corresponds to how many layers there are in this buffer.
    /// All of the buffers are of this size.
    chunks: u32,
    /// Blending mode buffers. See also [BlendingMode]
    blends: Box<[u32]>,
    /// Opacity buffer. Each element is the corresponding layer's
    opacities: Box<[f32]>,
    /// Mask buffer. Each element is an index into a texture view array, and
    /// corresponds to the layer's maximum alpha value.
    masks: Box<[u32]>,
    /// Layer buffer. Each element is an index into a texture view array, and
    /// corresponds to the layer's RGBA value.
    layers: Box<[u32]>,
    /// Corresponds to the how many layers are in this render pass.
    pub(super) count: u32,
}

impl CpuBuffers {
    const MASK_NONE: u32 = u32::MAX;

    /// Create shader buffers on the CPU side.
    pub fn new(chunks: u32) -> Self {
        let chunks = chunks as usize;

        Self {
            chunks: chunks as u32,
            blends: vec![0; chunks].into_boxed_slice(),
            opacities: vec![0.0; chunks].into_boxed_slice(),
            masks: vec![0; chunks].into_boxed_slice(),
            layers: vec![0; chunks].into_boxed_slice(),
            count: 0,
        }
    }

    /// Reset all of the buffers to its initial state.
    fn reset(&mut self) {
        self.blends.fill(0);
        self.opacities.fill(0.0);
        self.masks.fill(Self::MASK_NONE);
        self.layers.fill(0);
        self.count = 0;
    }

    /// Resolves the given composite layers and fill the CPU buffer.
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
    /// Corresponds to how many layers maximum can be bounded to the buffer.
    chunks: u32,
    /// Texture view array. This is different from the texture array.
    /// This array's final destination is to be mapped to the GPU.
    output: Box<[wgpu::TextureView]>,
    /// Maps the composite layer's texture index to the index in the
    /// texture view array. This is so that clipping masks and layers
    /// can reuse the same layer data if necessary.
    map: HashMap<usize, u32>,
    /// Stores the array of composite layers to resolve.
    composite_layers: &'dev [CompositeLayer],
    /// Texture array that the composite layers texture index references.
    textures: &'dev [GpuTexture],
    /// Target CPU buffer to resolve data to.
    bindings: &'dev mut CpuBuffers,
}

impl<'dev> BindingMapper<'dev> {
    /// Create the binding mapper.
    fn new(
        chunks: u32,
        composite_layers: &'dev [CompositeLayer],
        textures: &'dev [GpuTexture],
        bindings: &'dev mut CpuBuffers,
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

    /// Obtain an index in the texture view array, given the index in
    /// the texture array. This may reuse a mapped index in the texture view
    /// array.
    fn map_texture(&mut self, texture_index: usize) -> u32 {
        let mlen = self.map.len() as u32;
        *self.map.entry(texture_index).or_insert_with(|| {
            self.output[mlen as usize] = self.textures[texture_index].make_view();
            mlen
        })
    }

    /// Maps the CPU buffer and return the texture view array, to be used
    /// in the GPU buffer.
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
                self.bindings.masks[index] = CpuBuffers::MASK_NONE;
                self.bindings.layers[index] = self.map_texture(layer.texture);
            }

            self.bindings.blends[index] = layer.blend.to_u32();
            self.bindings.opacities[index] = layer.opacity;
            self.bindings.count += 1;
        }

        self.output
    }
}

/// Shader buffers on the GPU side.
pub(super) struct GpuBuffers<'dev> {
    dev: &'dev GpuHandle,
    pub(super) blends: wgpu::Buffer,
    pub(super) opacities: wgpu::Buffer,
    pub(super) masks: wgpu::Buffer,
    pub(super) layers: wgpu::Buffer,
    pub(super) count: wgpu::Buffer,
}

impl<'dev> GpuBuffers<'dev> {
    /// Create the buffers on the GPU.
    pub fn new(dev: &'dev GpuHandle) -> Self {
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

    /// Write the contents of the CPU buffers into the GPU buffers.
    pub fn load(&self, cpu: &CpuBuffers) {
        let q = &self.dev.queue;
        q.write_buffer(&self.blends, 0, bytemuck::cast_slice(&cpu.blends));
        q.write_buffer(&self.opacities, 0, bytemuck::cast_slice(&cpu.opacities));
        q.write_buffer(&self.masks, 0, bytemuck::cast_slice(&cpu.masks));
        q.write_buffer(&self.layers, 0, bytemuck::cast_slice(&cpu.layers));
        q.write_buffer(&self.count, 0, &cpu.count.to_ne_bytes());
    }
}
