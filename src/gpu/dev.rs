#[derive(Debug)]
pub struct LogicalDevice {
    pub instance: wgpu::Instance,
    pub device: wgpu::Device,
    pub adapter: wgpu::Adapter,
    pub queue: wgpu::Queue,
}

impl LogicalDevice {
    const ADAPTER_OPTIONS: wgpu::RequestAdapterOptions<'static> = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    };

    pub async fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);
        let adapter = instance.request_adapter(&Self::ADAPTER_OPTIONS).await?;
        Self::from_adapter(instance, adapter).await
    }

    pub async fn with_window(window: &winit::window::Window) -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Self::ADAPTER_OPTIONS
            })
            .await?;
        Self::from_adapter(instance, adapter).await
    }

    async fn from_adapter(instance: wgpu::Instance, adapter: wgpu::Adapter) -> Option<Self> {
        dbg!(adapter.get_info());
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::TEXTURE_BINDING_ARRAY
                        | wgpu::Features::STORAGE_RESOURCE_BINDING_ARRAY
                        | wgpu::Features::BUFFER_BINDING_ARRAY
                        | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
                    limits: wgpu::Limits {
                        max_sampled_textures_per_shader_stage: crate::gpu::CHUNKS * 2 + 1,
                        max_storage_buffers_per_shader_stage: crate::gpu::CHUNKS,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                None,
            )
            .await
            .ok()?;

        Some(Self {
            instance,
            device,
            adapter,
            queue,
        })
    }
}
