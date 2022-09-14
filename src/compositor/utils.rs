use std::num::NonZeroU32;

pub fn shader_load() -> wgpu::ShaderModuleDescriptor<'static> {
    #[cfg(not(debug_assertions))]
    {
        wgpu::include_wgsl!("../shader.wgsl")
    }
    #[cfg(debug_assertions)]
    {
        wgpu::ShaderModuleDescriptor {
            label: Some("Dynamically loaded shader module"),
            source: wgpu::ShaderSource::Wgsl({
                use std::fs::OpenOptions;
                use std::io::Read;
                let mut file = OpenOptions::new()
                    .read(true)
                    .open("./src/shader.wgsl")
                    .unwrap();

                let mut buf = String::new();
                file.read_to_string(&mut buf).unwrap();
                buf.into()
            }),
        }
    }
}

pub fn blending_group_layout(device: &wgpu::Device, chunks: u32) -> wgpu::BindGroupLayout {
    const fn fragment_bgl_buffer_ro_entry(
        binding: u32,
        count: Option<NonZeroU32>,
    ) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count,
        }
    }

    const fn fragment_bgl_uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }
    }

    const fn fragment_bgl_tex_entry(
        binding: u32,
        count: Option<NonZeroU32>,
    ) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                multisampled: false,
                view_dimension: wgpu::TextureViewDimension::D2,
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
            },
            count,
        }
    }

    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("blending_group_layout"),
        entries: &[
            fragment_bgl_tex_entry(0, None),
            fragment_bgl_tex_entry(1, NonZeroU32::new(chunks)),
            fragment_bgl_buffer_ro_entry(2, None),
            fragment_bgl_buffer_ro_entry(3, None),
            fragment_bgl_buffer_ro_entry(4, None),
            fragment_bgl_buffer_ro_entry(5, None),
            fragment_bgl_uniform_entry(6),
        ],
    })
}
