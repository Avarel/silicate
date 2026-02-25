mod blend;
pub mod compositor;
pub mod instance;

use compositor::CompositorApp;
use egui_dock::{NodeIndex, SurfaceIndex};
use egui_notify::Toast;
use egui_wgpu::wgpu;
use egui_winit::winit::event_loop::EventLoopProxy;
use instance::{Instance, InstanceKey};
use silica_gpu::{
    error::SilicaError,
    file::{ProcreateFileCanvas, ProcreateFileGpu},
};
use silicate_compositor::{
    buffer::BufferDimensions,
    canvas::{CompositorAtlasTiling, CompositorCanvasTiling},
    dev::GpuDispatch,
    pipeline::Pipeline,
    tex::GpuTexture,
    Compositor,
};
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};
use std::{sync::atomic::AtomicUsize, time::Duration};

pub enum UserEvent {
    NewInstance(InstanceKey, Instance, CompositorApp),
    NewView(SurfaceIndex, NodeIndex, InstanceKey),
    RebindTexture(InstanceKey),
    RebindPreviews(InstanceKey),
    RemoveInstance(InstanceKey),
    LoadDialog(SurfaceIndex, NodeIndex),
    SaveDialog(GpuTexture),
    Toast(Toast),
}

impl std::fmt::Debug for UserEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserEvent::NewInstance(arg0, arg1, _) => f
                .debug_tuple("NewInstance")
                .field(arg0)
                .field(arg1)
                .finish(),
            UserEvent::NewView(surface_index, node_index, instance_key) => f
                .debug_tuple("NewView")
                .field(surface_index)
                .field(node_index)
                .field(instance_key)
                .finish(),
            UserEvent::RebindTexture(arg0) => f.debug_tuple("RebindTexture").field(arg0).finish(),
            UserEvent::RebindPreviews(arg0) => f.debug_tuple("RebindPreviews").field(arg0).finish(),
            UserEvent::RemoveInstance(arg0) => f.debug_tuple("RemoveInstance").field(arg0).finish(),
            UserEvent::Toast(_) => f.debug_tuple("Toast").field(&"...").finish(),
            UserEvent::LoadDialog(_, _) => f.debug_tuple("LoadDialog").field(&"...").finish(),
            UserEvent::SaveDialog(_) => f.debug_tuple("SaveDialog").field(&"...").finish(),
        }
    }
}

pub struct App {
    dispatch: GpuDispatch,
    event_loop: EventLoopProxy<UserEvent>,
    pipeline: Pipeline,
    curr_id: AtomicUsize,
}

impl App {
    pub fn new(dispatch: GpuDispatch, event_loop: EventLoopProxy<UserEvent>) -> Self {
        Self {
            pipeline: Pipeline::new(&dispatch),
            dispatch,
            event_loop,
            curr_id: AtomicUsize::new(0),
        }
    }

    pub fn send_toast(&self, toast: Toast) {
        self.event_loop.send_event(UserEvent::Toast(toast)).ok();
    }

    pub fn load_file(&self, path: PathBuf) -> Result<InstanceKey, SilicaError> {
        let (file, metadata) = tokio::task::block_in_place(|| {
            ProcreateFileGpu::open(&path, self.dispatch.device(), self.dispatch.queue())
        })
        .unwrap();

        let ProcreateFileCanvas {
            atlas_texture,
            canvas_tiling,
        } = metadata;

        let canvas = CompositorCanvasTiling::new(
            (file.info.size.width, file.info.size.height),
            (canvas_tiling.cols, canvas_tiling.rows),
            canvas_tiling.size,
        );
        let composite_target = Compositor::new(
            self.dispatch.clone(),
            canvas,
            CompositorAtlasTiling::new(canvas_tiling.atlas.cols, canvas_tiling.atlas.rows),
            GpuTexture::from_texture(atlas_texture.clone()),
        );

        let output_texture = GpuTexture::empty(
            &self.dispatch,
            file.info.size.width,
            file.info.size.height,
            GpuTexture::OUTPUT_USAGE,
        );

        let rotation = match file.info.orientation {
            silica_gpu::raw::data::Orientation::NoRotation => 0.0,
            silica_gpu::raw::data::Orientation::Clockwise180 => 180.0,
            silica_gpu::raw::data::Orientation::Clockwise270 => 270.0,
            silica_gpu::raw::data::Orientation::Clockwise90 => 90.0,
            _ => 0f32,
        }
        .to_radians();

        let initial_compositor_file = Arc::new(file.clone());
        let (compositor, handle) = CompositorApp::new(
            self.pipeline.clone(),
            initial_compositor_file.clone(),
            composite_target,
        );

        let id = self
            .curr_id
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let key = InstanceKey(id);
        let mut instance = Instance {
            file: file.clone(),
            output_texture: output_texture.clone(),
            preview_textures: None,
            compositor: handle,
            rotation,
            previews: HashMap::new(),
            canvas: None,
        };
        instance.generate_previews(
            Compositor::new(
                self.dispatch.clone(),
                canvas,
                CompositorAtlasTiling::new(canvas_tiling.atlas.cols, canvas_tiling.atlas.rows),
                GpuTexture::from_texture(atlas_texture),
            ),
            &self.dispatch,
            &self.pipeline,
        );

        self.event_loop
            .send_event(UserEvent::NewInstance(key, instance, compositor))
            .unwrap();
        Ok(key)
    }

    /// Export the texture to the given path.
    pub async fn export(
        texture: &GpuTexture,
        dispatch: &GpuDispatch,
        dim: BufferDimensions,
        path: std::path::PathBuf,
    ) -> image::ImageResult<()> {
        let output_buffer = texture.export_buffer(dispatch, dim);

        let buffer_slice = output_buffer.slice(..);

        // NOTE: We have to create the mapping THEN device.poll() before await
        // the future. Otherwise the application will freeze.
        let (tx, rx) = tokio::sync::oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        dispatch
            .device()
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
        rx.await.unwrap().expect("Buffer mapping failed");

        let data = buffer_slice.get_mapped_range().to_vec();
        output_buffer.unmap();

        eprintln!("Loading data to CPU");
        let buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
            dim.padded_bytes_per_row() / 4,
            dim.height(),
            data,
        )
        .unwrap();

        let buffer = image::imageops::crop_imm(&buffer, 0, 0, dim.width(), dim.height()).to_image();

        eprintln!("Saving the file to {}", path.display());
        tokio::task::spawn_blocking(move || buffer.save(path))
            .await
            .unwrap()
    }

    pub fn rebind_texture(&self, id: InstanceKey) {
        self.event_loop
            .send_event(UserEvent::RebindTexture(id))
            .unwrap();
    }
}
