use egui_dock::{NodeIndex, SurfaceIndex};
use egui_notify::Toasts;
use egui_wgpu::wgpu;
use egui_winit::winit::event_loop::EventLoopProxy;
use parking_lot::{Mutex, RwLock};
use silica::{
    error::SilicaError,
    file::ProcreateFile,
    layers::{SilicaGroup, SilicaHierarchy, SilicaLayer},
};
use silicate_compositor::{
    atlas::AtlasData, buffer::BufferDimensions, canvas::CanvasTiling, dev::GpuDispatch,
    pipeline::Pipeline, tex::GpuTexture, ChunkTile, CompositeLayer, Target,
};
use std::path::PathBuf;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::Arc;
use std::time::Duration;
use std::{collections::HashMap, num::NonZeroU32};
use tokio::time::MissedTickBehavior;
use tokio::{runtime::Runtime, sync::mpsc::Sender};

pub struct App {
    pub dispatch: GpuDispatch,
    pub rt: Arc<Runtime>,
    pub compositor: Arc<CompositorApp>,
    pub toasts: Mutex<Toasts>,
    pub new_instances: Sender<(SurfaceIndex, NodeIndex, InstanceKey)>,
    pub(crate) event_loop: EventLoopProxy<UserEvent>,
}

#[derive(Debug, Clone, Copy)]
pub enum UserEvent {
    RebindTexture(InstanceKey),
    RemoveInstance(InstanceKey),
}

#[derive(Hash, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct InstanceKey(pub usize);

pub struct Instance {
    key: InstanceKey,
    pub file: RwLock<ProcreateFile>,
    pub atlas_texture: GpuTexture,
    pub target: Mutex<Target>,
    pub changed: AtomicBool,
    tiling: silica::layers::TilingData,
}

impl Instance {
    pub fn store_change_or(&self, b: bool) {
        self.changed.fetch_or(b, Release);
    }

    pub fn change_untick(&self) -> bool {
        self.changed.swap(false, Acquire)
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        println!("Closing {:?}", self.file.get_mut().name);
    }
}

pub struct CompositorApp {
    pub instances: RwLock<HashMap<InstanceKey, Instance>>,
    pub curr_id: AtomicUsize,
    pub pipeline: Pipeline,
}

impl App {
    pub fn load_file(&self, path: PathBuf) -> Result<InstanceKey, SilicaError> {
        let (file, atlas_texture, tiling) =
            tokio::task::block_in_place(|| ProcreateFile::open(path, &self.dispatch)).unwrap();

        let canvas = CanvasTiling::new(
            (file.size.width, file.size.height),
            (tiling.cols, tiling.rows),
            tiling.size,
        );
        let target = Target::new(self.dispatch.clone(), canvas);
        // target
        //     .data
        //     .flip_vertices(file.flipped.horizontally, file.flipped.vertically);

        // for _ in 0..file.orientation {
        //     target.data.rotate_vertices(true);
        //     target.set_dimensions(target.dim.height, target.dim.width);
        // }

        let id = self
            .compositor
            .curr_id
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let key = InstanceKey(id);
        self.compositor.instances.write().insert(
            key,
            Instance {
                key,
                file: RwLock::new(file),
                target: Mutex::new(target),
                atlas_texture,
                tiling,
                changed: AtomicBool::new(true),
            },
        );
        self.rebind_texture(key);
        Ok(key)
    }

    pub async fn load_dialog(&self, surface_index: SurfaceIndex, node_index: NodeIndex) {
        if let Some(handle) = {
            let mut dialog = rfd::AsyncFileDialog::new();
            dialog = dialog.add_filter("All Files", &["*"]);
            dialog = dialog.add_filter("Procreate Files", &["procreate"]);
            dialog
        }
        .pick_file()
        .await
        {
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
        } else {
            self.toasts.lock().info("Load cancelled.");
        }
    }

    pub async fn save_dialog(&self, copied_texture: GpuTexture) {
        if let Some(handle) = rfd::AsyncFileDialog::new()
            .add_filter("png", image::ImageFormat::Png.extensions_str())
            .add_filter("jpeg", image::ImageFormat::Jpeg.extensions_str())
            .add_filter("tga", image::ImageFormat::Tga.extensions_str())
            .add_filter("tiff", image::ImageFormat::Tiff.extensions_str())
            .add_filter("webp", image::ImageFormat::WebP.extensions_str())
            .add_filter("bmp", image::ImageFormat::Bmp.extensions_str())
            .save_file()
            .await
        {
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
        } else {
            self.toasts.lock().info("Export cancelled.");
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
}

impl CompositorApp {
    /// Transform tree structure of layers into a linear list of
    /// layers for rendering.
    fn linearize_silica_layers<'a>(
        composite_layers: &mut Vec<CompositeLayer>,
        layers: &'a SilicaGroup,
    ) {
        composite_layers.clear();

        fn inner<'a>(layers: &'a SilicaGroup, composite_layers: &mut Vec<CompositeLayer>) {
            for layer in layers.children.iter().rev() {
                match layer {
                    SilicaHierarchy::Group(group) => {
                        inner(group, composite_layers);
                    }
                    SilicaHierarchy::Layer(layer) => {
                        composite_layers.push(CompositeLayer {
                            opacity: layer.opacity,
                            blend: layer.blend,
                            clipped: layer.clipped,
                            hidden: layer.hidden,
                        });
                    }
                }
            }
        }

        inner(layers, composite_layers);
    }

    fn linearize_silica_chunks<'a>(composite_layers: &mut Vec<ChunkTile>, layers: &'a SilicaGroup) {
        composite_layers.clear();

        let mut layer_counter = 0;

        fn inner<'a>(
            layers: &'a SilicaGroup,
            chunks: &mut Vec<ChunkTile>,
            mask_layer: &mut Option<&'a SilicaLayer>,
            layer_counter: &mut u32,
        ) {
            for layer in layers.children.iter().rev() {
                match layer {
                    SilicaHierarchy::Group(group) => {
                        inner(group, chunks, mask_layer, layer_counter);
                    }
                    SilicaHierarchy::Layer(layer) => {
                        for chunk in layer.image.chunks.iter() {
                            let mut mask_atlas_index: Option<NonZeroU32> = None;

                            if let Some(mask_layer) = mask_layer.as_ref() {
                                for mask_chunk in mask_layer.image.chunks.iter() {
                                    if mask_chunk.col == chunk.col && mask_chunk.row == chunk.row {
                                        mask_atlas_index = Some(mask_chunk.atlas_index);
                                    }
                                }
                            }

                            chunks.push(ChunkTile {
                                col: chunk.col,
                                row: chunk.row,
                                atlas_index: chunk.atlas_index,
                                mask_atlas_index,
                                layer_index: *layer_counter,
                            });
                        }
                        *mask_layer = Some(layer);
                        *layer_counter += 1;
                    }
                }
            }
        }

        inner(layers, composite_layers, &mut None, &mut layer_counter);
    }

    pub async fn rendering_thread(self: Arc<Self>) {
        let mut composite_layers = Vec::new();
        let mut composite_chunks: Vec<ChunkTile> = Vec::new();
        let mut limiter = tokio::time::interval(Duration::from_secs(1).div_f64(f64::from(60)));
        limiter.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut last_loaded_instance_chunks_index = None;

        loop {
            // Ensures that we are not generating frames faster than 60FPS
            // to avoid putting unnecessary computational pressure on the GPU.
            limiter.tick().await;

            for instance in self.instances.read().values() {
                // If the file is contended then it might be edited by the GUI.
                // Might as well not render a soon to be outdated result.
                if let Some(file) = instance.file.try_read() {
                    // Only force a recompute if we need to.
                    if !instance.change_untick() {
                        continue;
                    }

                    let new_layer_config = file.layers.clone();
                    let background = (!file.background_hidden).then_some(file.background_color);
                    // Drop the guard here, we no longer need it.
                    drop(file);

                    let start_time = std::time::Instant::now();

                    let reload_chunks = if last_loaded_instance_chunks_index
                        .map(|v| v != instance.key)
                        .unwrap_or(true)
                    {
                        Self::linearize_silica_chunks(&mut composite_chunks, &new_layer_config);
                        composite_chunks.sort_by_key(|v| (v.col, v.row));
                        true
                    } else {
                        false
                    };
                    last_loaded_instance_chunks_index = Some(instance.key);

                    let chunk_linear_time = start_time.elapsed();

                    let start_time = std::time::Instant::now();

                    Self::linearize_silica_layers(&mut composite_layers, &new_layer_config);

                    let layer_linear_time = start_time.elapsed();

                    eprintln!("Chunk linearization: {:?}", chunk_linear_time);
                    eprintln!("Layer linearization: {:?}", layer_linear_time);

                    let mut target = instance.target.lock();
                    if reload_chunks {
                        eprintln!("Reloading chunks");
                        target.load_chunk_buffer(composite_chunks.as_slice());
                    }
                    target.render(
                        &self.pipeline,
                        background,
                        &composite_layers,
                        &AtlasData::new(instance.tiling.atlas.cols, instance.tiling.atlas.rows),
                        &instance.atlas_texture,
                    );
                    // ENABLE TO DEBUG: hold the lock to make sure the GUI is responsive
                    // std::thread::sleep(std::time::Duration::from_secs(1));
                    // Debugging notes: if the GPU is highly contended, the main
                    // GUI rendering can still be somewhat sluggish.
                    drop(target);
                }
            }
        }
    }
}
