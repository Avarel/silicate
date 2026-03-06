mod blend;
pub mod compositor;
pub mod instance;

use compositor::CompositorApp;
use eframe::egui_wgpu::wgpu;
use egui_dock::{NodeIndex, SurfaceIndex};
use egui_notify::Toast;
use instance::{Instance, InstanceKey};
use silica_gpu::{ProcreateFile, ProcreateFileAtlas, error::SilicaError};
use silicate_compositor::{
    Compositor,
    buffer::BufferDimensions,
    canvas::{CompositorAtlasTiling, CompositorCanvasTiling},
    pipeline::Pipeline,
    tex::TextureExt,
};
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::{fs::OpenOptions, path::Path};
use std::{sync::atomic::AtomicUsize, sync::mpsc::Sender, time::Duration};

pub enum AppEvent {
    NewInstance(InstanceKey, Instance, CompositorApp),
    NewView(SurfaceIndex, NodeIndex, InstanceKey),
    RebindTexture(InstanceKey),
    RebindPreviews(InstanceKey),
    RemoveInstance(InstanceKey),
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    LoadFilePath {
        path: PathBuf,
        surface_index: Option<SurfaceIndex>,
        node_index: Option<NodeIndex>,
    },
    LoadFileBytes {
        bytes: Arc<[u8]>,
        surface_index: Option<SurfaceIndex>,
        node_index: Option<NodeIndex>,
    },
    LoadDialog(SurfaceIndex, NodeIndex),
    SaveDialog(wgpu::Texture),
    Toast(Toast),
    #[cfg(target_arch = "wasm32")]
    LoadDemoFile,
}

impl std::fmt::Debug for AppEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppEvent::NewInstance(arg0, arg1, _) => f
                .debug_tuple("NewInstance")
                .field(arg0)
                .field(arg1)
                .finish(),
            AppEvent::NewView(surface_index, node_index, instance_key) => f
                .debug_tuple("NewView")
                .field(surface_index)
                .field(node_index)
                .field(instance_key)
                .finish(),
            AppEvent::RebindTexture(arg0) => f.debug_tuple("RebindTexture").field(arg0).finish(),
            AppEvent::RebindPreviews(arg0) => f.debug_tuple("RebindPreviews").field(arg0).finish(),
            AppEvent::RemoveInstance(arg0) => f.debug_tuple("RemoveInstance").field(arg0).finish(),
            AppEvent::Toast(_) => f.debug_tuple("Toast").field(&"...").finish(),
            AppEvent::LoadFilePath { .. } => f.debug_tuple("LoadFilePath").field(&"...").finish(),
            AppEvent::LoadFileBytes { .. } => f.debug_tuple("LoadFilebytes").field(&"...").finish(),
            AppEvent::LoadDialog(_, _) => f.debug_tuple("LoadDialog").field(&"...").finish(),
            AppEvent::SaveDialog(_) => f.debug_tuple("SaveDialog").field(&"...").finish(),
            #[cfg(target_arch = "wasm32")]
            AppEvent::LoadDemoFile => f.debug_tuple("LoadDemoFile").finish(),
        }
    }
}

pub struct App {
    device: wgpu::Device,
    queue: wgpu::Queue,
    event_sender: Sender<AppEvent>,
    pipeline: Pipeline,
    curr_id: AtomicUsize,
}

impl App {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, event_sender: Sender<AppEvent>) -> Self {
        Self {
            pipeline: Pipeline::new(&device),
            device,
            queue,
            event_sender,
            curr_id: AtomicUsize::new(0),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_file(&self, path: &Path) -> Result<InstanceKey, SilicaError> {
        let file = OpenOptions::new().read(true).write(false).open(path)?;

        let mapping = unsafe { memmap2::Mmap::map(&file)? };

        self.load_bytes(&mapping)
    }

    pub fn load_bytes(&self, bytes: &[u8]) -> Result<InstanceKey, SilicaError> {
        let id = InstanceKey::new(
            self.curr_id
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        );
        log::info!("{id} Loading file");

        let open_file = || ProcreateFile::open(bytes, &self.device, &self.queue);

        #[cfg(not(target_arch = "wasm32"))]
        let (file, metadata) = tokio::task::block_in_place(open_file).unwrap();
        #[cfg(target_arch = "wasm32")]
        let (file, metadata) = open_file().unwrap();

        log::info!(
            "{id} Loaded Procreate document \"{}\" with {} layers",
            file.name.as_deref().unwrap_or("Untitled Artwork"),
            file.layer_count(true)
        );

        let ProcreateFileAtlas {
            atlas_texture,
            canvas_tiling,
        } = metadata;

        let canvas = CompositorCanvasTiling::new(
            (file.size.width, file.size.height),
            (canvas_tiling.cols, canvas_tiling.rows),
            canvas_tiling.size,
        );
        let composite_target = Compositor::new(
            &self.device,
            &self.queue,
            canvas,
            CompositorAtlasTiling::new(canvas_tiling.atlas.cols, canvas_tiling.atlas.rows),
            atlas_texture.clone(),
        );

        let output_texture = wgpu::Texture::empty(
            &self.device,
            file.size.width,
            file.size.height,
            wgpu::Texture::OUTPUT_USAGE,
        );

        let rotation = match file.orientation {
            silica_gpu::Orientation::NoRotation => 0.0,
            silica_gpu::Orientation::Clockwise180 => 180.0,
            silica_gpu::Orientation::Clockwise270 => 270.0,
            silica_gpu::Orientation::Clockwise90 => 90.0,
            _ => 0f32,
        }
        .to_radians();

        let initial_compositor_file = Arc::new(file.clone());
        let (compositor, handle) = CompositorApp::new(
            id,
            self.pipeline.clone(),
            initial_compositor_file.clone(),
            composite_target,
        );

        let mut instance = Instance {
            id,
            file: file.clone(),
            output_texture: output_texture.clone(),
            preview_textures: None,
            compositor: handle,
            rotation,
            previews: HashMap::new(),
            canvas: None,
        };

        log::debug!(
            "{id} Generating previews for Procreate document \"{}\"",
            file.name.as_deref().unwrap_or("Untitled Artwork")
        );

        instance.generate_previews(
            Compositor::new(
                &self.device,
                &self.queue,
                canvas,
                CompositorAtlasTiling::new(canvas_tiling.atlas.cols, canvas_tiling.atlas.rows),
                atlas_texture,
            ),
            &self.device,
            &self.pipeline,
        );

        log::info!(
            "{id} Instance created for Procreate document \"{}\"",
            file.name.as_deref().unwrap_or("Untitled Artwork")
        );

        self.event_sender
            .send(AppEvent::NewInstance(id, instance, compositor))
            .unwrap();
        Ok(id)
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    /// Export the texture to the given path.
    pub async fn export(
        texture: &wgpu::Texture,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        dim: BufferDimensions,
    ) -> image::ImageResult<image::ImageBuffer<image::Rgba<u8>, Vec<u8>>> {
        let output_buffer = texture.export_buffer(device, queue, dim);

        let buffer_slice = output_buffer.slice(..);

        // NOTE: We have to create the mapping THEN device.poll() before await
        // the future. Otherwise the application will freeze.
        let (tx, rx) = tokio::sync::oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
        rx.await.unwrap().expect("Buffer mapping failed");

        let data = buffer_slice.get_mapped_range().to_vec();
        output_buffer.unmap();

        log::debug!("Loading data to CPU");
        let buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
            dim.padded_bytes_per_row() / 4,
            dim.height(),
            data,
        )
        .unwrap();

        Ok(image::imageops::crop_imm(&buffer, 0, 0, dim.width(), dim.height()).to_image())
    }

    pub fn rebind_texture(&self, id: InstanceKey) {
        self.event_sender.send(AppEvent::RebindTexture(id)).unwrap();
    }
}
