pub mod compositor;
pub mod instance;

use egui_dock::{NodeIndex, SurfaceIndex};
use egui_notify::Toasts;
use egui_wgpu::wgpu;
use egui_winit::winit::event_loop::EventLoopProxy;
use instance::{Instance, InstanceKey};
use parking_lot::{Mutex, RwLock};
use silica::{
    error::SilicaError,
    file::{ProcreateFile, ProcreateFileMetadata},
};
use silicate_compositor::{
    buffer::BufferDimensions,
    canvas::{CompositorAtlasTiling, CompositorCanvasTiling},
    dev::GpuDispatch,
    tex::GpuTexture,
    Compositor,
};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::{runtime::Runtime, sync::mpsc::Sender};

pub struct App {
    pub dispatch: GpuDispatch,
    pub rt: Arc<Runtime>,
    pub compositor: Arc<compositor::CompositorApp>,
    pub toasts: Mutex<Toasts>,
    pub new_instances: Sender<(SurfaceIndex, NodeIndex, InstanceKey)>,
    pub(crate) event_loop: EventLoopProxy<UserEvent>,
}

#[derive(Debug, Clone, Copy)]
pub enum UserEvent {
    RebindTexture(InstanceKey),
    RebindPreviews(InstanceKey),
    RemoveInstance(InstanceKey),
}

impl App {
    pub fn load_file(&self, path: PathBuf) -> Result<InstanceKey, SilicaError> {
        let (file, metadata) =
            tokio::task::block_in_place(|| ProcreateFile::open(&path, &self.dispatch)).unwrap();

        let ProcreateFileMetadata {
            atlas_texture,
            canvas_tiling,
        } = metadata;

        let canvas = CompositorCanvasTiling::new(
            (file.size.width, file.size.height),
            (canvas_tiling.cols, canvas_tiling.rows),
            canvas_tiling.size,
        );
        let mut composite_target = Compositor::new(
            self.dispatch.clone(),
            canvas,
            CompositorAtlasTiling::new(canvas_tiling.atlas.cols, canvas_tiling.atlas.rows),
            atlas_texture.clone(),
        );

        let output_texture = GpuTexture::empty(
            &self.dispatch,
            file.size.width,
            file.size.height,
            GpuTexture::OUTPUT_USAGE,
        );

        composite_target.set_flipped(file.flipped.horizontally, file.flipped.vertically);

        let rotation = match file.orientation {
            silica::data::Orientation::NoRotation => 0.0,
            silica::data::Orientation::Clockwise180 => 180.0,
            silica::data::Orientation::Clockwise270 => 270.0,
            silica::data::Orientation::Clockwise90 => 90.0,
            _ => 0f32,
        }
        .to_radians();

        let addendum = crate::addendum::build(&file.layers);

        let id = self
            .compositor
            .curr_id
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let key = InstanceKey(id);
        let mut instance = Instance {
            addendum,
            output_texture,
            flipped: file.flipped,
            file: RwLock::new(file),
            target: Mutex::new(composite_target),
            changed: AtomicBool::new(true),
            needs_to_load_chunks: AtomicBool::new(true),
            preview_textures: None,
            rotation,
        };
        instance.generate_previews(&self.dispatch, &self.compositor.pipeline);
        self.compositor.instances.write().insert(key, instance);
        self.rebind_texture(key);
        self.rebind_previews(key);
        Ok(key)
    }

    pub async fn load_dialog(&self, surface_index: SurfaceIndex, node_index: NodeIndex) {
        let dialog = rfd::AsyncFileDialog::new()
            .add_filter("All Files", &["*"])
            .add_filter("Procreate Files", &["procreate"])
            .pick_file();

        let Some(handle) = dialog.await else {
            self.toasts.lock().info("Load cancelled.");
            return;
        };

        match self.load_file(handle.path().to_path_buf()) {
            Err(err) => {
                self.toasts.lock().error(format!(
                    "File {} failed to load. Reason: {err}",
                    handle.file_name()
                ));
            }
            Ok(key) => {
                self.toasts
                    .lock()
                    .success(format!("File {} successfully opened.", handle.file_name()));
                self.new_instances
                    .send((surface_index, node_index, key))
                    .await
                    .unwrap();
            }
        }
    }

    pub async fn save_dialog(&self, copied_texture: GpuTexture) {
        let dialog = rfd::AsyncFileDialog::new()
            .add_filter("png", image::ImageFormat::Png.extensions_str())
            .add_filter("jpeg", image::ImageFormat::Jpeg.extensions_str())
            .add_filter("tga", image::ImageFormat::Tga.extensions_str())
            .add_filter("tiff", image::ImageFormat::Tiff.extensions_str())
            .add_filter("webp", image::ImageFormat::WebP.extensions_str())
            .add_filter("bmp", image::ImageFormat::Bmp.extensions_str())
            .save_file();

        let Some(handle) = dialog.await else {
            self.toasts.lock().info("Export cancelled.");
            return;
        };

        let dim = BufferDimensions::from_extent(copied_texture.size);
        let path = handle.path().to_path_buf();
        if let Err(err) = Self::export(&copied_texture, &self.dispatch, dim, path).await {
            self.toasts.lock().error(format!(
                "File {} failed to export. Reason: {err}.",
                handle.file_name()
            ));
        } else {
            self.toasts.lock().success(format!(
                "File {} successfully exported.",
                handle.file_name()
            ));
        }
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
        dispatch.device().poll(wgpu::MaintainBase::Wait);
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

    pub fn rebind_previews(&self, id: InstanceKey) {
        self.event_loop
            .send_event(UserEvent::RebindPreviews(id))
            .unwrap();
    }
}
