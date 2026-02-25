use std::collections::HashMap;

use egui::load::SizedTexture;
use silica_gpu::{file::ProcreateFileGpu, hierarchy::SilicaHierarchyGpu};
use silicate_compositor::{Compositor, dev::GpuDispatch, pipeline::Pipeline, tex::TextureExt};

use crate::app::compositor::CompositorApp;

use super::compositor::CompositorHandle;

#[derive(Hash, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct InstanceKey(pub usize);

pub struct Instance {
    pub file: ProcreateFileGpu,
    pub output_texture: crate::wgpu::Texture,
    pub rotation: f32,
    pub preview_textures: Option<crate::wgpu::Texture>,
    pub compositor: CompositorHandle,

    pub previews: HashMap<u32, SizedTexture>,
    pub canvas: Option<SizedTexture>,
}

impl std::fmt::Debug for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Instance")
            .field("file", &self.file)
            .field("output_texture", &self.output_texture)
            .field("rotation", &self.rotation)
            .field("preview_textures", &self.preview_textures)
            .field("compositor", &"..")
            .finish()
    }
}

impl Instance {
    pub fn is_upright(&self) -> bool {
        !(45.0..135.0).contains(&self.rotation.to_degrees())
            && !(225.0..315.0).contains(&self.rotation.to_degrees())
    }

    pub fn submit_to_compositor(&mut self) {
        self.compositor.submit(&self.file);
    }

    pub fn generate_previews(
        &mut self,
        mut target: Compositor,
        dispatch: &GpuDispatch,
        pipeline: &Pipeline,
    ) {
        let file = &self.file;
        let aspect_ratio = file.info.size.width as f32 / file.info.size.height as f32;
        let scaled_height = (256.0 * aspect_ratio) as u32;

        let preview_textures = {
            fn generate_silica_layers_preview(
                pipeline: &Pipeline,
                target: &mut Compositor,
                preview_textures: &crate::wgpu::Texture,
                layers: &[SilicaHierarchyGpu],
            ) {
                fn inner(
                    pipeline: &Pipeline,
                    target: &mut Compositor,
                    preview_textures: &crate::wgpu::Texture,
                    layers: &[SilicaHierarchyGpu],
                ) {
                    for layer in layers.iter() {
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
                        match layer {
                            SilicaHierarchyGpu::Group(group) => {
                                target.render(
                                    pipeline,
                                    preview_textures.create_view_layer(group.addendum.id),
                                );
                                inner(pipeline, target, preview_textures, &group.children);
                            }

                            SilicaHierarchyGpu::Layer(layer) => {
                                target.render(
                                    pipeline,
                                    preview_textures.create_view_layer(layer.addendum.id),
                                );
                            }
                        }
                    }
                }

                inner(pipeline, target, preview_textures, layers);
            }

            let preview_textures = crate::wgpu::Texture::empty_layers(
                dispatch,
                256,
                scaled_height,
                file.layer_count(true) + 1,
                crate::wgpu::Texture::OUTPUT_USAGE,
            );

            generate_silica_layers_preview(pipeline, &mut target, &preview_textures, &file.layers);

            preview_textures
        };

        self.preview_textures = Some(preview_textures);
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        println!("Closing {:?}", self.file.info.name);
    }
}
