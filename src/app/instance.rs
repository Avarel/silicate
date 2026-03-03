use eframe::wgpu;

use std::collections::HashMap;
use egui::load::SizedTexture;
use silica_gpu::ProcreateFile;
use silicate_compositor::{pipeline::Pipeline, tex::TextureExt, Compositor};

use crate::app::compositor::CompositorApp;

use super::compositor::CompositorHandle;

#[derive(Hash, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct InstanceKey(usize);

impl InstanceKey {
    pub fn new(id: usize) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for InstanceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[I:{}]", self.0)
    }
}

pub struct Instance {
    pub id: InstanceKey,
    pub file: ProcreateFile,
    pub output_texture: wgpu::Texture,
    pub rotation: f32,
    pub preview_textures: Option<wgpu::Texture>,
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
        device: &wgpu::Device,
        pipeline: &Pipeline,
    ) {
        let file = &self.file;
        let aspect_ratio = file.size.width as f32 / file.size.height as f32;
        let scaled_height = (256.0 * aspect_ratio) as u32;

        let preview_textures = {
            let preview_textures = wgpu::Texture::empty_layers(
                device,
                256,
                scaled_height,
                file.layer_count(true) + 1,
                wgpu::Texture::OUTPUT_USAGE,
            );

            CompositorApp::generate_layers_preview(pipeline, &mut target, &preview_textures, &file.layers);

            preview_textures
        };

        self.preview_textures = Some(preview_textures);
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        eprintln!(
            "{} Closing instance for Procreate document \"{}\"",
            self.id,
            self.file.name.as_deref().unwrap_or("Untitled Artwork")
        );
    }
}
