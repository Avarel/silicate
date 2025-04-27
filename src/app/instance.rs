use std::sync::Arc;

use silica::file::ProcreateFile;
use silica::layers::SilicaHierarchy;
use silicate_compositor::{
    dev::GpuDispatch,
    pipeline::Pipeline,
    tex::GpuTexture,
    Compositor,
};
use tokio::sync::watch::Sender;
use tokio::sync::Notify;

use crate::addendum::SilicaHierarchyAddendum;
use crate::app::compositor::CompositorApp;

#[derive(Hash, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct InstanceKey(pub usize);

pub struct Instance {
    pub file: ProcreateFile,
    pub addendum: Vec<SilicaHierarchyAddendum>,
    pub output_texture: GpuTexture,
    pub rotation: f32,
    pub preview_textures: Option<GpuTexture>,

    pub(crate) previously_sent_file: Arc<ProcreateFile>,
    pub(crate) compositor_sender: Sender<Arc<ProcreateFile>>,
    pub(crate) notify: Arc<Notify>,
}

impl Instance {
    pub fn is_upright(&self) -> bool {
        !(45.0..135.0).contains(&self.rotation.to_degrees())
            && !(225.0..315.0).contains(&self.rotation.to_degrees())
    }

    pub fn submit(&mut self) {
        if *self.previously_sent_file != self.file {
            let file = Arc::new(self.file.clone());
            self.compositor_sender.send_replace(Arc::clone(&file));
            self.previously_sent_file = file;
        }
    }

    pub fn generate_previews(&mut self, target: &mut Compositor, dispatch: &GpuDispatch, pipeline: &Pipeline) {
        let file = &self.file;
        let aspect_ratio = file.size.width as f32 / file.size.height as f32;
        let scaled_height = (256.0 * aspect_ratio) as u32;

        let preview_textures = {
            fn generate_silica_layers_preview(
                pipeline: &Pipeline,
                target: &mut Compositor,
                preview_textures: &GpuTexture,
                layers: &[SilicaHierarchy],
                addendum: &[SilicaHierarchyAddendum],
            ) {
                fn inner(
                    pipeline: &Pipeline,
                    target: &mut Compositor,
                    preview_textures: &GpuTexture,
                    layers: &[SilicaHierarchy],
                    addendums: &[SilicaHierarchyAddendum],
                ) {
                    for (layer, addendum) in layers.iter().zip(addendums.iter()) {
                        {
                            let layer = std::slice::from_ref(layer);
                            let mut composite_layers = Vec::new();
                            CompositorApp::linearize_silica_layers(&mut composite_layers, layer);

                            target.load_layer_buffer(composite_layers.as_slice());

                            let mut composite_chunks = Vec::new();
                            CompositorApp::linearize_silica_chunks(&mut composite_chunks, layer);
                            composite_chunks.sort_by_key(|v| (v.col, v.row));
                            target.load_chunk_buffer(composite_chunks.as_slice());
                        }
                        match (layer, addendum) {
                            (
                                SilicaHierarchy::Group(group),
                                SilicaHierarchyAddendum::Group(addendum),
                            ) => {
                                target.render(
                                    pipeline,
                                    preview_textures.create_view_layer(addendum.id),
                                );
                                inner(
                                    pipeline,
                                    target,
                                    preview_textures,
                                    &group.children,
                                    &addendum.children,
                                );
                            }
                            (
                                SilicaHierarchy::Layer(_),
                                SilicaHierarchyAddendum::Layer(addendum),
                            ) => {
                                target.render(
                                    pipeline,
                                    preview_textures.create_view_layer(addendum.id),
                                );
                            }
                            _ => unreachable!(),
                        }
                    }
                }

                inner(pipeline, target, preview_textures, layers, addendum);
            }

            let preview_textures = GpuTexture::empty_layers(
                dispatch,
                256,
                scaled_height,
                file.layer_count(true),
                GpuTexture::OUTPUT_USAGE,
            );

            generate_silica_layers_preview(
                pipeline,
                target,
                &preview_textures,
                &file.layers,
                &self.addendum,
            );

            preview_textures
        };

        self.preview_textures = Some(preview_textures);
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.notify.notify_waiters();
        println!("Closing {:?}", self.file.name);
    }
}
