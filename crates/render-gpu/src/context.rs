use crate::GpuError;

/// A GPU device and queue. Cheap to clone; share one between renderers.
#[derive(Clone)]
pub struct GpuContext {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    adapter_name: String,
    backend: String,
}

impl GpuContext {
    /// Open the best available adapter. `WGPU_BACKEND` (for example `vulkan` or `dx12`)
    /// narrows the choice, which is how CI selects a software driver.
    pub fn new() -> Result<Self, GpuError> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::from_env().unwrap_or_default(),
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .ok_or(GpuError::NoAdapter)?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("inbetween"),
                    required_features: wgpu::Features::empty(),
                    required_limits: adapter.limits(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .map_err(|e| GpuError::Device(e.to_string()))?;
        Ok(Self {
            device,
            queue,
            adapter_name: info.name,
            backend: format!("{:?}", info.backend),
        })
    }

    /// Wrap a device and queue that something else already opened, such as the ones eframe
    /// creates. Textures made by a renderer built on it can then be shown by that program
    /// directly, with no copy.
    pub fn from_parts(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter_name: impl Into<String>,
        backend: impl Into<String>,
    ) -> Self {
        Self {
            device,
            queue,
            adapter_name: adapter_name.into(),
            backend: backend.into(),
        }
    }

    /// The device, for code that needs to register textures with another library.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The adapter's name, e.g. "NVIDIA GeForce RTX 5080".
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// The graphics API in use, e.g. "Vulkan" or "Dx12".
    pub fn backend(&self) -> &str {
        &self.backend
    }

    /// The largest image side the device accepts.
    pub fn max_texture_side(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }
}
