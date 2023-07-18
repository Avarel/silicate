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
}

impl GpuHandle {
    const INSTANCE_OPTIONS: wgpu::InstanceDescriptor = wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
    };

    const ADAPTER_OPTIONS: wgpu::RequestAdapterOptions<'static> = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    };

    #[allow(dead_code)]
    /// Create a bare GPU handle with no surface target.
    pub async fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(Self::INSTANCE_OPTIONS);
        let adapter = instance.request_adapter(&Self::ADAPTER_OPTIONS).await?;
        Self::from_adapter(instance, adapter).await
    }

    /// Create a GPU handle with a surface target compatible with the window.
    pub async fn with_window(window: &winit::window::Window) -> Option<(Self, wgpu::Surface)> {
        let instance = wgpu::Instance::new(Self::INSTANCE_OPTIONS);
        let surface = unsafe { instance.create_surface(window) }.ok()?;
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
        // Debugging information
        dbg!(adapter.get_info());
        dbg!(adapter.limits());

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::PUSH_CONSTANTS,
                    limits: wgpu::Limits {
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
            instance,
            device,
            adapter,
            queue,
        })
    }
}
