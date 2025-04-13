use std::sync::Arc;

/// This module computes how to bind the compositor layers to the shader
/// variables. It is configured specifically to serve the `shader.wgsl`
/// shader module and create bindings that match the shader's inputs.
use super::dev::GpuHandle;
use crate::CompositeLayer;

/// Shader buffers on the CPU side.
#[derive(Debug)]
pub struct CpuBuffers {
    /// Corresponds to how many layers there are in this buffer.
    /// All of the buffers are of this size.
    chunks: usize,
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
    pub fn new(size: usize) -> Self {
        Self {
            chunks: size,
            blends: vec![0; size].into_boxed_slice(),
            opacities: vec![0.0; size].into_boxed_slice(),
            masks: vec![0; size].into_boxed_slice(),
            layers: vec![0; size].into_boxed_slice(),
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
    pub fn map_composite_layers(&mut self, composite_layers: &[CompositeLayer]) {
        self.reset();
        for (index, layer) in composite_layers.iter().enumerate() {
            if index >= self.chunks {
                break;
            }

            self.masks[index] = layer.clipped.unwrap_or(CpuBuffers::MASK_NONE);
            self.layers[index] = layer.texture;

            self.blends[index] = layer.blend.to_u32();
            self.opacities[index] = layer.opacity;
        }
        self.count = composite_layers.len() as u32;
    }
}

/// Shader buffers on the GPU side.
pub(super) struct GpuBuffers {
    dev: Arc<GpuHandle>,
    size: usize,
    pub(super) blends: wgpu::Buffer,
    pub(super) opacities: wgpu::Buffer,
    pub(super) masks: wgpu::Buffer,
    pub(super) layers: wgpu::Buffer,
}

impl GpuBuffers {
    /// Create the buffers on the GPU.
    pub fn new(dev: Arc<GpuHandle>, size: usize) -> Self {
        let storage_desc: wgpu::BufferDescriptor = wgpu::BufferDescriptor {
            label: None,
            size: 4 * size as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        };
        GpuBuffers {
            blends: dev.device.create_buffer(&storage_desc),
            opacities: dev.device.create_buffer(&storage_desc),
            masks: dev.device.create_buffer(&storage_desc),
            layers: dev.device.create_buffer(&storage_desc),
            dev,
            size,
        }
    }

    /// Write the contents of the CPU buffers into the GPU buffers.
    pub fn load(&self, cpu: &CpuBuffers) {
        assert_eq!(self.size, cpu.chunks);
        assert!(self.size as u32 >= cpu.count);

        let q = &self.dev.queue;
        q.write_buffer(&self.blends, 0, bytemuck::cast_slice(&cpu.blends));
        q.write_buffer(&self.opacities, 0, bytemuck::cast_slice(&cpu.opacities));
        q.write_buffer(&self.masks, 0, bytemuck::cast_slice(&cpu.masks));
        q.write_buffer(&self.layers, 0, bytemuck::cast_slice(&cpu.layers));
    }
}
