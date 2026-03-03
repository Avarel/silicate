use eframe::wgpu;

use std::sync::{mpsc::Sender, Arc};

use egui_dock::{NodeIndex, SurfaceIndex};
use egui_notify::Toast;
use silicate_compositor::buffer::BufferDimensions;

use crate::app::{App, AppEvent};

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

    pub async fn load_dialog(
        self,
        app: Arc<App>,
        surface_index: SurfaceIndex,
        node_index: NodeIndex,
    ) {
        let dialog = rfd::AsyncFileDialog::new()
            .add_filter("All Files", &["*"])
            .add_filter("Procreate Files", &["procreate"])
            .pick_file();

        let Some(handle) = dialog.await else {
            self.send_toast(Toast::info("Load cancelled."));
            return;
        };

        match app.load_file(handle.path().to_path_buf()) {
            Err(err) => {
                self.send_toast(Toast::error(format!(
                    "File {} failed to load. Reason: {err}",
                    handle.file_name()
                )));
            }
            Ok(key) => {
                self.send_toast(Toast::success(format!(
                    "File {} successfully opened.",
                    handle.file_name()
                )));
                self.event_sender
                    .send(AppEvent::NewView(surface_index, node_index, key))
                    .unwrap();
            }
        }
    }

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

        let dim = BufferDimensions::from_extent(copied_texture.size());
        let path = handle.path().to_path_buf();
        if let Err(err) = App::export(&copied_texture, &device, &queue, dim, path).await {
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
