/// Represents a grouping of useful GPU resources.
#[derive(Debug)]
pub struct GpuHandle {
    /// WGPU instance.
    #[allow(dead_code)]
    pub instance: wgpu::Instance,
    /// Physical compute device.
    pub adapter: wgpu::Adapter,
    /// Logical compute device.
    pub device: wgpu::Device,
    /// Device command queue.
    pub queue: wgpu::Queue,
}

impl GpuHandle {
    pub fn instance_descriptor() -> wgpu::InstanceDescriptor {
        wgpu::InstanceDescriptor {
            backend_options: wgpu::BackendOptions {
                dx12: wgpu::Dx12BackendOptions {
                    presentation_system: wgpu::wgt::Dx12SwapchainKind::DxgiFromVisual,
                    ..Default::default()
                },
                gl: wgpu::GlBackendOptions::default(),
                noop: wgpu::NoopBackendOptions::default(),
            },
            backends: if !cfg!(target_os = "linux") {
                // Prefer native APIs... they're generally faster.
                wgpu::Backends::DX12 | wgpu::Backends::METAL
            } else {
                wgpu::Backends::PRIMARY
            },
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            flags: wgpu::InstanceFlags::default(),
        }
    }

    pub const ADAPTER_OPTIONS: wgpu::RequestAdapterOptions<'_, '_> = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    };

    #[allow(dead_code)]
    /// Create a bare GPU handle with no surface target.
    pub async fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(&Self::instance_descriptor());
        let adapter = instance
            .request_adapter(&Self::ADAPTER_OPTIONS)
            .await
            .ok()?;
        Self::from_adapter(instance, adapter).await
    }

    /// Request device.
    pub async fn from_adapter(instance: wgpu::Instance, adapter: wgpu::Adapter) -> Option<Self> {
        // Debugging information
        dbg!(adapter.get_info());
        dbg!(adapter.limits());

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features::default(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .ok()?;

        Some(Self {
            instance,
            adapter,
            queue,
            device
        })
    }
}
