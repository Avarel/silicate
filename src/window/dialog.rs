use eframe::wgpu;

use std::sync::mpsc::Sender;

use egui_dock::{NodeIndex, SurfaceIndex};
use egui_notify::Toast;

use crate::app::AppEvent;

pub struct Dialog {
    event_sender: Sender<AppEvent>,
}

impl Dialog {
    pub fn new(event_sender: Sender<AppEvent>) -> Self {
        Self { event_sender }
    }

    fn send_toast(&self, toast: Toast) {
        self.event_sender.send(AppEvent::Toast(toast)).ok();
    }

    pub async fn load_dialog(self, surface_index: SurfaceIndex, node_index: NodeIndex) {
        let dialog = rfd::AsyncFileDialog::new()
            .add_filter("All Files", &["*"])
            .add_filter("Procreate Files", &["procreate"])
            .pick_file();

        let Some(handle) = dialog.await else {
            self.send_toast(Toast::info("Load cancelled."));
            return;
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.event_sender
                .send(AppEvent::LoadFilePath {
                    path: handle.path().to_path_buf(),
                    surface_index: Some(surface_index),
                    node_index: Some(node_index),
                })
                .unwrap();
        }
        #[cfg(target_arch = "wasm32")]
        {
            use std::sync::Arc;

            let data = handle.read().await;
            log::info!("File read complete, loading file...");
            self.event_sender
                .send(AppEvent::LoadFileBytes {
                    bytes: Arc::from(data),
                    surface_index: Some(surface_index),
                    node_index: Some(node_index),
                })
                .unwrap();
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code, unused))]
    pub async fn save_dialog(
        self,
        device: wgpu::Device,
        queue: wgpu::Queue,
        copied_texture: wgpu::Texture,
    ) {
        let dialog = rfd::AsyncFileDialog::new()
            .add_filter("png", image::ImageFormat::Png.extensions_str())
            .add_filter("jpeg", image::ImageFormat::Jpeg.extensions_str())
            .add_filter("tga", image::ImageFormat::Tga.extensions_str())
            .add_filter("tiff", image::ImageFormat::Tiff.extensions_str())
            .add_filter("webp", image::ImageFormat::WebP.extensions_str())
            .add_filter("bmp", image::ImageFormat::Bmp.extensions_str())
            .save_file();

        let Some(handle) = dialog.await else {
            self.send_toast(Toast::info("Export cancelled."));
            return;
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            use silicate_compositor::buffer::BufferDimensions;

            let dim = BufferDimensions::from_extent(copied_texture.size());
            let path = handle.path().to_path_buf();

            let image = crate::app::App::export(&copied_texture, &device, &queue, dim)
                .await
                .unwrap();

            log::info!("Saving the file to {}", path.display());
            let save_result = tokio::task::spawn_blocking(move || image.save(path))
                .await
                .unwrap();

            if let Err(err) = save_result {
                self.send_toast(Toast::error(format!(
                    "File {} failed to export. Reason: {err}.",
                    handle.file_name()
                )));
            } else {
                self.send_toast(Toast::success(format!(
                    "File {} successfully exported.",
                    handle.file_name()
                )));
            }
        }
    }
}
