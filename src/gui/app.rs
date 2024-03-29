use crate::compositor::{dev::GpuHandle, tex::GpuTexture};
use crate::compositor::{BufferDimensions, CompositorTarget};
use crate::compositor::{CompositeLayer, CompositorPipeline};
use crate::silica::{ProcreateFile, SilicaError, SilicaHierarchy};
use egui_dock::{NodeIndex, SurfaceIndex};
use egui_notify::Toasts;
use egui_winit::winit::event_loop::EventLoopProxy;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::time::MissedTickBehavior;

pub struct App {
    pub dev: Arc<GpuHandle>,
    pub rt: Arc<Runtime>,
    pub compositor: CompositorHandle,
    pub toasts: Mutex<Toasts>,
    pub added_instances: Mutex<Vec<(SurfaceIndex, NodeIndex, InstanceKey)>>,
    pub event_loop: EventLoopProxy<UserEvent>,
}

#[derive(Debug, Clone, Copy)]
pub enum UserEvent {
    RebindTexture(InstanceKey),
    RemoveInstance(InstanceKey),
}

#[derive(Hash, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct InstanceKey(pub usize);

pub struct Instance {
    pub file: RwLock<ProcreateFile>,
    pub textures: GpuTexture,
    pub target: Mutex<CompositorTarget>,
    pub changed: AtomicBool,
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

pub struct CompositorHandle {
    pub instances: RwLock<HashMap<InstanceKey, Instance>>,
    pub curr_id: AtomicUsize,
    pub pipeline: CompositorPipeline,
}

impl App {
    pub fn new(dev: GpuHandle, rt: Arc<Runtime>, event_loop: EventLoopProxy<UserEvent>) -> Self {
        App {
            compositor: CompositorHandle {
                instances: RwLock::new(HashMap::new()),
                pipeline: CompositorPipeline::new(&dev),
                curr_id: AtomicUsize::new(0),
            },
            rt,
            dev: Arc::new(dev),
            toasts: Mutex::new(egui_notify::Toasts::default()),
            added_instances: Mutex::new(Vec::with_capacity(1)),
            event_loop,
        }
    }

    pub async fn load_file(&self, path: PathBuf) -> Result<InstanceKey, SilicaError> {
        let (file, textures) =
            tokio::task::block_in_place(|| ProcreateFile::open(path, &self.dev)).unwrap();
        let mut target = CompositorTarget::new(self.dev.clone());
        target
            .data
            .flip_vertices(file.flipped.horizontally, file.flipped.vertically);
        target.set_dimensions(file.size.width, file.size.height);

        for _ in 0..file.orientation {
            target.data.rotate_vertices(true);
            target.set_dimensions(target.dim.height, target.dim.width);
        }

        let id = self
            .compositor
            .curr_id
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let key = InstanceKey(id);
        self.compositor.instances.write().insert(
            key,
            Instance {
                file: RwLock::new(file),
                target: Mutex::new(target),
                textures,
                changed: AtomicBool::new(true),
            },
        );
        self.rebind_texture(key);
        Ok(key)
    }

    pub async fn load_dialog(self: Arc<Self>, surface_index: SurfaceIndex, node_index: NodeIndex) {
        if let Some(handle) = {
            let mut dialog = rfd::AsyncFileDialog::new();
            dialog = dialog.add_filter("All Files", &["*"]);
            dialog = dialog.add_filter("Procreate Files", &["procreate"]);
            dialog
        }
        .pick_file()
        .await
        {
            match self.clone().load_file(handle.path().to_path_buf()).await {
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
                    self.added_instances
                        .lock()
                        .push((surface_index, node_index, key));
                }
            }
        } else {
            self.toasts.lock().info("Load cancelled.");
        }
    }

    pub async fn save_dialog(self: Arc<Self>, copied_texture: GpuTexture) {
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
            if let Err(err) = copied_texture.export(&self.dev, dim, path).await {
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

    pub async fn rendering_thread(self: Arc<App>) {
        let mut limiter = tokio::time::interval(Duration::from_secs(1).div_f64(f64::from(60)));
        limiter.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            // Ensures that we are not generating frames faster than 60FPS
            // to avoid putting unnecessary computational pressure on the GPU.
            limiter.tick().await;

            for instance in self.compositor.instances.read().values() {
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

                    let resolved_layers = Self::linearize_silica_layers(&new_layer_config);

                    let mut lock = instance.target.lock();
                    lock.render(
                        &self.compositor.pipeline,
                        background,
                        &resolved_layers,
                        &instance.textures,
                    );
                    // ENABLE TO DEBUG: hold the lock to make sure the GUI is responsive
                    // std::thread::sleep(std::time::Duration::from_secs(1));
                    // Debugging notes: if the GPU is highly contended, the main
                    // GUI rendering can still be somewhat sluggish.
                    drop(lock);
                }
            }
        }
    }

    /// Transform tree structure of layers into a linear list of
    /// layers for rendering.
    fn linearize_silica_layers<'a>(layers: &'a crate::silica::SilicaGroup) -> Vec<CompositeLayer> {
        fn inner<'a>(
            layers: &'a crate::silica::SilicaGroup,
            composite_layers: &mut Vec<CompositeLayer>,
            mask_layer: &mut Option<(u32, &'a crate::silica::SilicaLayer)>,
        ) {
            for layer in layers.children.iter().rev() {
                match layer {
                    SilicaHierarchy::Group(group) if !group.hidden => {
                        inner(group, composite_layers, mask_layer);
                    }
                    SilicaHierarchy::Layer(layer) if !layer.hidden => {
                        if let Some((_, mask_layer)) = mask_layer {
                            if layer.clipped && mask_layer.hidden {
                                continue;
                            }
                        }

                        if !layer.clipped {
                            *mask_layer = Some((layer.image, layer));
                        }

                        composite_layers.push(CompositeLayer {
                            texture: layer.image,
                            clipped: layer.clipped.then(|| mask_layer.unwrap().0),
                            opacity: layer.opacity,
                            blend: layer.blend,
                        });
                    }
                    _ => continue,
                }
            }
        }

        let mut composite_layers = Vec::new();
        inner(layers, &mut composite_layers, &mut None);
        composite_layers
    }

    pub fn rebind_texture(&self, id: InstanceKey) {
        self.event_loop
            .send_event(UserEvent::RebindTexture(id))
            .unwrap();
    }
}
