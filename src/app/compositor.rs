use silica::{
    file::ProcreateFile,
    layers::{SilicaHierarchy, SilicaLayer},
};
use silicate_compositor::{
    pipeline::Pipeline, tex::GpuTexture, ChunkTile, CompositeLayer, Compositor,
};
use std::{num::NonZeroU32, sync::Arc};
use std::sync::atomic::AtomicBool;
use tokio::sync::{watch::{Receiver, Sender}, Notify};

pub struct CompositorApp {
    pub target: Compositor,
    pub needs_to_load_chunks: AtomicBool,
    pub pipeline: Pipeline,
    rx: Receiver<Arc<ProcreateFile>>,
    notify: Arc<Notify>,
}

impl CompositorApp {
    /// Transform tree structure of layers into a linear list of
    /// layers for rendering.
    pub(crate) fn linearize_silica_layers(
        composite_layers: &mut Vec<CompositeLayer>,
        layers: &[SilicaHierarchy],
    ) {
        composite_layers.clear();

        pub(crate) fn inner(
            layers: &[SilicaHierarchy],
            composite_layers: &mut Vec<CompositeLayer>,
            override_hidden: bool,
        ) {
            for layer in layers.iter().rev() {
                match layer {
                    SilicaHierarchy::Group(group) => {
                        inner(
                            &group.children,
                            composite_layers,
                            group.hidden | override_hidden,
                        );
                    }
                    SilicaHierarchy::Layer(layer) => {
                        composite_layers.push(CompositeLayer {
                            opacity: layer.opacity,
                            blend: layer.blend,
                            clipped: layer.clipped,
                            hidden: layer.hidden | override_hidden,
                        });
                    }
                }
            }
        }

        inner(layers, composite_layers, false);
    }

    pub(crate) fn linearize_silica_chunks(
        composite_layers: &mut Vec<ChunkTile>,
        layers: &[SilicaHierarchy],
    ) {
        composite_layers.clear();

        let mut layer_counter = 0;

        pub(crate) fn inner<'a>(
            layers: &'a [SilicaHierarchy],
            chunks: &mut Vec<ChunkTile>,
            clip_layer: &mut Option<&'a SilicaLayer>,
            layer_counter: &mut u32,
        ) {
            for layer in layers.iter().rev() {
                match layer {
                    SilicaHierarchy::Group(group) => {
                        inner(&group.children, chunks, clip_layer, layer_counter);
                    }
                    SilicaHierarchy::Layer(layer) => {
                        for chunk in layer.image.chunks.iter() {
                            let mut clip_atlas_index: Option<NonZeroU32> = None;

                            if let Some(clip_layer) = clip_layer.as_ref() {
                                for clip_chunk in clip_layer.image.chunks.iter() {
                                    if clip_chunk.col == chunk.col && clip_chunk.row == chunk.row {
                                        clip_atlas_index = Some(clip_chunk.atlas_index);
                                    }
                                }
                            }

                            chunks.push(ChunkTile {
                                col: chunk.col,
                                row: chunk.row,
                                atlas_index: chunk.atlas_index,
                                clip_atlas_index,
                                layer_index: *layer_counter,
                            });
                        }
                        *clip_layer = Some(layer);
                        *layer_counter += 1;
                    }
                }
            }
        }

        inner(layers, composite_layers, &mut None, &mut layer_counter);
    }

    pub fn new(
        pipeline: Pipeline,
        file: Arc<ProcreateFile>,
        target: Compositor,
    ) -> (Self, Arc<Notify>, Sender<Arc<ProcreateFile>>) {
        let (tx, mut rx) = tokio::sync::watch::channel(file);

        rx.mark_changed();

        let notify = Arc::new(Notify::new());

        let compositor = Self {
            notify: notify.clone(),
            rx,
            target,
            needs_to_load_chunks: AtomicBool::new(true),
            pipeline,
        };

        (compositor, notify, tx)
    }

    pub async fn rendering_thread(mut self, output_texture: GpuTexture) {
        let mut composite_layers = Vec::new();
        let mut composite_chunks: Vec<ChunkTile> = Vec::new();

        loop {
            tokio::select! {
                _ = self.notify.notified() => { break }
                _ = self.rx.changed() => {}
            };

            let file = (*self.rx.borrow()).clone();

            let new_layer_config = file.layers.clone();
            let background = (!file.background_hidden).then_some(file.background_color);

            let reload_chunks = self
                .needs_to_load_chunks
                .fetch_and(false, std::sync::atomic::Ordering::AcqRel);

            if reload_chunks {
                Self::linearize_silica_chunks(&mut composite_chunks, &new_layer_config);
                composite_chunks.sort_by_key(|v| (v.col, v.row));
            }

            Self::linearize_silica_layers(&mut composite_layers, &new_layer_config);

            self.target.load_layer_buffer(composite_layers.as_slice());
            if reload_chunks {
                eprintln!("Reloading chunks");
                self.target.load_chunk_buffer(composite_chunks.as_slice());
            }
            self.target.set_background(background);
            self.target
                .set_flipped(file.flipped.horizontally, file.flipped.vertically);
            self.target
                .render(&self.pipeline, output_texture.create_default_view());
            // ENABLE TO DEBUG: hold the lock to make sure the GUI is responsive
            // {
            //     if !cfg!(debug_assertions) {
            //         panic!("FORGOT TO DISABLE DEBUG CODE")
            //     }
            //     std::thread::sleep(std::time::Duration::from_secs(1));
            // }
            // Debugging notes: if the GPU is highly contended, the main
            // GUI rendering can still be somewhat sluggish.
        }

        eprintln!("Done rendering")
    }
}
