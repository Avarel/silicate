/// Represents a grouping of useful GPU resources.
#[derive(Debug)]
pub struct GpuHandle {
    /// WGPU instance.
    pub instance: wgpu::Instance,
    /// Physical compute device.
    pub adapter: wgpu::Adapter,
    /// Logical compute device.
    pub device: wgpu::Device,
    /// Device command queue.
    pub queue: wgpu::Queue,
    /// How many textures to be binded at once in a shader render pass.
    pub chunks: u32,
}

const CHUNKS_LIMIT: u32 = 32;

impl GpuHandle {
    const ADAPTER_OPTIONS: wgpu::RequestAdapterOptions<'static> = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    };

    #[allow(dead_code)]
    /// Create a bare GPU handle with no surface target.
    pub async fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);
        let adapter = instance.request_adapter(&Self::ADAPTER_OPTIONS).await?;
        Self::from_adapter(instance, adapter).await
    }

    /// Create a GPU handle with a surface target compatible with the window.
    pub async fn with_window(window: &winit::window::Window) -> Option<(Self, wgpu::Surface)> {
        let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Self::ADAPTER_OPTIONS
            })
            .await?;
        Self::from_adapter(instance, adapter)
            .await
            .map(|dev| (dev, surface))
    }

    /// Request device.
    async fn from_adapter(instance: wgpu::Instance, adapter: wgpu::Adapter) -> Option<Self> {
        let chunks = (adapter.limits().max_sampled_textures_per_shader_stage - 1).min(CHUNKS_LIMIT);

        // Debugging information
        dbg!(adapter.get_info());
        dbg!(adapter.limits());
        dbg!(chunks);

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::TEXTURE_BINDING_ARRAY
                        | wgpu::Features::PUSH_CONSTANTS
                        | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
                    limits: wgpu::Limits {
                        max_sampled_textures_per_shader_stage: chunks + 1,
                        max_push_constant_size: 4,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                None,
            )
            .await
            .ok()?;

        Some(Self {
            chunks,
            instance,
            device,
            adapter,
            queue,
        })
    }
}
