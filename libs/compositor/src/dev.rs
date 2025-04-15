/// Represents a grouping of useful GPU resources.
#[derive(Debug)]
pub struct GpuHandle {
    /// WGPU instance.
    #[allow(dead_code)]
    pub instance: wgpu::Instance,
    /// Physical compute device.
    pub adapter: wgpu::Adapter,
    pub dispatch: GpuDispatch,
}

#[derive(Debug, Clone)]
pub struct GpuDispatch {
    /// Logical compute device.
    device: wgpu::Device,
    /// Device command queue.
    queue: wgpu::Queue,
}

impl GpuDispatch {
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}

impl GpuHandle {
    pub fn instance_descriptor() -> wgpu::InstanceDescriptor {
        wgpu::InstanceDescriptor {
            backend_options: wgpu::BackendOptions {
                dx12: wgpu::Dx12BackendOptions::default(),
                gl: wgpu::GlBackendOptions::default(),
            },
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::default(),
        }
    }

    pub const ADAPTER_OPTIONS: wgpu::RequestAdapterOptions<'static, 'static> =
        wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        };

    #[allow(dead_code)]
    /// Create a bare GPU handle with no surface target.
    pub async fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(&Self::instance_descriptor());
        let adapter = instance.request_adapter(&Self::ADAPTER_OPTIONS).await?;
        Self::from_adapter(instance, adapter).await
    }

    /// Request device.
    pub async fn from_adapter(instance: wgpu::Instance, adapter: wgpu::Adapter) -> Option<Self> {
        // Debugging information
        dbg!(adapter.get_info());
        dbg!(adapter.limits());

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::STORAGE_RESOURCE_BINDING_ARRAY,
                    required_limits: wgpu::Limits {
                        max_buffer_size: 1024 << 20,
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
            adapter,
            dispatch: GpuDispatch { queue, device },
        })
    }
}
