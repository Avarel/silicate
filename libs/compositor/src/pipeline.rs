use crate::{ChunkInstance, VertexInput, dev::GpuDispatch};

pub struct Pipeline {
    pub sampler_bind_group: wgpu::BindGroup,
    pub blending_bind_group_layout: wgpu::BindGroupLayout,
    pub render_pipeline: wgpu::RenderPipeline,
    pub canvas_bind_group_layout: wgpu::BindGroupLayout,
}

impl Pipeline {
    /// Create a new compositor pipeline.
    pub fn new(dispatch: &GpuDispatch) -> Self {
        let device = dispatch.device();

        let canvas_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("canvas_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // This bind group only binds the sampler, which is a constant
        // through out all rendering passes.
        let (sampler_bind_group_layout, sampler_bind_group) = {
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                }],
            });

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("constant_bind_group"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                }],
            });

            (layout, bind_group)
        };

        // This bind group changes per composition run.
        let blending_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("blending_group_layout"),
                entries: &[
                    // atlas
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // textures
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        },
                        count: None,
                    },
                    // chunks
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // layers
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // segments
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        // Loads the shader and creates the render pipeline.
        let render_pipeline = {
            let shader = device.create_shader_module(shader_load());

            let render_pipeline_layout =
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("render_pipeline_layout"),
                    bind_group_layouts: &[
                        &canvas_bind_group_layout,
                        &sampler_bind_group_layout,
                        &blending_bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                });

            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                cache: None,
                label: Some("render_pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    entry_point: Some("vs_main"),
                    buffers: &[VertexInput::desc(), ChunkInstance::desc()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: crate::tex::TEX_FORMAT,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
            })
        };

        Self {
            canvas_bind_group_layout,
            sampler_bind_group,
            blending_bind_group_layout,
            render_pipeline,
        }
    }
}

#[cfg(not(debug_assertions))]
fn shader_load() -> wgpu::ShaderModuleDescriptor<'static> {
    // In release mode, the final binary includes the file directly so that
    // the binary does not rely on the shader file being at a specific location.
    wgpu::include_wgsl!("shader.wgsl")
}

#[cfg(debug_assertions)]
fn shader_load() -> wgpu::ShaderModuleDescriptor<'static> {
    // In debug mode, this reads directly from a file so that recompilation
    // will not be necessary in the event that only the shader file changes.
    wgpu::ShaderModuleDescriptor {
        label: Some("Dynamically loaded shader module"),
        source: wgpu::ShaderSource::Wgsl({
            use std::fs::OpenOptions;
            use std::io::Read;
            let mut file = OpenOptions::new()
                .read(true)
                .open("./libs/compositor/src/shader.wgsl")
                .unwrap();

            let mut buf = String::new();
            file.read_to_string(&mut buf).unwrap();
            buf.into()
        }),
    }
}
