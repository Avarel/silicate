use eframe::wgpu;

use silica_gpu::{ProcreateFile, SilicaHierarchy, SilicaLayer};
use silicate_compositor::tex::TextureExt;
use silicate_compositor::{ChunkTile, CompositeLayer, Compositor, pipeline::Pipeline};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::watch::{Receiver, Sender};

use crate::app::instance::InstanceKey;

pub struct CompositorApp {
    target: Compositor,
    needs_to_load_chunks: AtomicBool,
    pipeline: Pipeline,
    rx: Receiver<Arc<ProcreateFile>>,
    id: InstanceKey,
    composite_layers: Vec<CompositeLayer>,
    composite_chunks: Vec<ChunkTile>,
}

pub struct CompositorHandle {
    previously_sent_file: Arc<ProcreateFile>,
    compositor_sender: Sender<Arc<ProcreateFile>>,
}

impl CompositorHandle {
    pub fn submit(&mut self, file: &ProcreateFile) {
        if *self.previously_sent_file != *file {
            let file = Arc::new(file.clone());
            self.compositor_sender.send_replace(Arc::clone(&file));
            self.previously_sent_file = file;
        }
    }
}

impl CompositorApp {
    /// Transform tree structure of layers into a linear list of
    /// layers for rendering.
    fn linearize_silica_layers(
        composite_layers: &mut Vec<CompositeLayer>,
        layers: &[SilicaHierarchy],
    ) {
        composite_layers.clear();

        fn inner(
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
                            blend: super::blend::convert_blend(layer.blend),
                            clipped: layer.clipped,
                            hidden: layer.hidden | override_hidden,
                            mask_hidden: layer.mask.as_ref().map_or(true, |mask| mask.hidden),
                        });
                    }
                }
            }
        }

        inner(layers, composite_layers, false);
    }

    fn linearize_silica_chunks(
        composite_layers: &mut Vec<ChunkTile>,
        layers: &[SilicaHierarchy],
        render_masks: bool,
    ) {
        composite_layers.clear();

        let mut layer_counter = 0;

        fn inner<'a>(
            layers: &'a [SilicaHierarchy],
            chunks: &mut Vec<ChunkTile>,
            clip_layer: &mut Option<&'a SilicaLayer>,
            layer_counter: &mut u32,
            render_masks: bool,
        ) {
            for layer in layers.iter().rev() {
                match layer {
                    SilicaHierarchy::Group(group) => {
                        inner(&group.children, chunks, clip_layer, layer_counter, true);
                    }
                    SilicaHierarchy::Layer(layer) => {
                        for chunk in layer.image.chunks.iter() {
                            let clip_atlas_index = clip_layer.as_ref().and_then(|clip_layer| {
                                clip_layer
                                    .image
                                    .chunks
                                    .iter()
                                    .find(|clip_chunk| {
                                        clip_chunk.col == chunk.col && clip_chunk.row == chunk.row
                                    })
                                    .map(|clip_chunk| clip_chunk.atlas_index)
                            });

                            let mask_atlas_index = if render_masks {
                                layer.mask.as_ref().and_then(|mask| {
                                    mask.image
                                        .chunks
                                        .iter()
                                        .find(|mask_chunk| {
                                            mask_chunk.col == chunk.col
                                                && mask_chunk.row == chunk.row
                                        })
                                        .map(|mask_chunk| mask_chunk.atlas_index)
                                })
                            } else {
                                None
                            };

                            chunks.push(ChunkTile {
                                col: chunk.col,
                                row: chunk.row,
                                atlas_index: chunk.atlas_index,
                                mask_atlas_index,
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

        inner(
            layers,
            composite_layers,
            &mut None,
            &mut layer_counter,
            render_masks,
        );
    }

    pub fn generate_layers_preview(
        pipeline: &Pipeline,
        target: &mut Compositor,
        preview_textures: &wgpu::Texture,
        layers: &[SilicaHierarchy],
    ) {
        for layer in layers.iter() {
            {
                let layer = std::slice::from_ref(layer);
                let mut composite_layers = Vec::new();
                CompositorApp::linearize_silica_layers(&mut composite_layers, layer);

                target.load_layer_buffer(composite_layers.as_slice());

                let mut composite_chunks = Vec::new();
                CompositorApp::linearize_silica_chunks(&mut composite_chunks, layer, false);
                composite_chunks.sort_by_key(|v| (v.col, v.row));
                target.load_chunk_buffer(composite_chunks.as_slice());
            }
            match layer {
                SilicaHierarchy::Group(group) => {
                    target.render(pipeline, preview_textures.create_view_layer(group.id));
                    Self::generate_layers_preview(
                        pipeline,
                        target,
                        preview_textures,
                        &group.children,
                    );
                }

                SilicaHierarchy::Layer(layer) => {
                    target.render(pipeline, preview_textures.create_view_layer(layer.id));
                    if let Some(mask_layer) = &layer.mask {
                        Self::generate_layers_preview(
                            pipeline,
                            target,
                            preview_textures,
                            std::slice::from_ref(&SilicaHierarchy::Layer(*mask_layer.clone())),
                        );
                    }
                }
            }
        }
    }

    pub fn new(
        id: InstanceKey,
        pipeline: Pipeline,
        file: Arc<ProcreateFile>,
        target: Compositor,
    ) -> (Self, CompositorHandle) {
        let (tx, mut rx) = tokio::sync::watch::channel(file.clone());

        rx.mark_changed();

        let compositor = Self {
            id,
            rx,
            target,
            needs_to_load_chunks: AtomicBool::new(true),
            pipeline,
            composite_layers: Vec::new(),
            composite_chunks: Vec::new(),
        };

        let handle = CompositorHandle {
            previously_sent_file: file.clone(),
            compositor_sender: tx,
        };

        (compositor, handle)
    }

    pub async fn rendering_thread(mut self, output_texture: wgpu::Texture) {
        loop {
            let file = match self.rx.changed().await {
                Ok(()) => (*self.rx.borrow_and_update()).clone(),
                Err(_) => break,
            };

            self.render_inner(&file, &output_texture);
        }

        log::debug!("{} Done rendering", self.id)
    }

    pub fn rendering_tick_blocking(&mut self, output_texture: &wgpu::Texture) {
        match self.rx.has_changed() {
            Ok(true) => {
                let file = (*self.rx.borrow_and_update()).clone();
                self.render_inner(&file, output_texture);
            }
            Ok(false) => {}
            Err(_) => {
                panic!("{} Compositor channel closed unexpectedly", self.id);
            }
        }
    }

    fn render_inner(&mut self, file: &ProcreateFile, output_texture: &wgpu::Texture) {
        let new_layer_config = file.layers.clone();
        // TODO: add render by composite mode
        // let new_layer_config = [SilicaHierarchy::Layer(file.composite.clone().unwrap())];

        let background = (!file.background_hidden).then_some(file.background_color);

        let reload_chunks = self
            .needs_to_load_chunks
            .fetch_and(false, std::sync::atomic::Ordering::AcqRel);

        if reload_chunks {
            Self::linearize_silica_chunks(&mut self.composite_chunks, &new_layer_config, true);
            self.composite_chunks.sort_by_key(|v| (v.col, v.row));
            self.target
                .load_chunk_buffer(self.composite_chunks.as_slice());

            log::debug!(
                "{} Linearized {} chunks",
                self.id,
                self.composite_chunks.len()
            );
        }

        Self::linearize_silica_layers(&mut self.composite_layers, &new_layer_config);
        self.target
            .load_layer_buffer(self.composite_layers.as_slice());

        self.target.set_background(background);
        self.target
            .set_flipped(file.flipped.horizontally, file.flipped.vertically);
        self.target
            .render(&self.pipeline, output_texture.create_default_view());
        // ENABLE TO DEBUG: hold the lock to make sure the GUI is responsive
        // {
        //     const { assert!(cfg!(debug_assertions)); }
        //     std::thread::sleep(std::time::Duration::from_secs(1));
        // }
        // Debugging notes: if the GPU is highly contended, the main
        // GUI rendering can still be somewhat sluggish.
    }
}
