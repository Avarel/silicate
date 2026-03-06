mod app;
mod gui;
mod window;

#[cfg(target_arch = "wasm32")]
mod web;

use app::AppEvent;
use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
};
use tokio::runtime::Runtime;
use window::AppInstance;

pub struct AppMultiplexer {
    rt: Runtime,
    initial_files: Vec<PathBuf>,
    running: Option<AppInstance>,
    event_sender: Sender<AppEvent>,
    event_receiver: Receiver<AppEvent>,
}

impl AppMultiplexer {
    pub fn new(initial_files: Vec<PathBuf>) -> Self {
        let (event_sender, event_receiver) = std::sync::mpsc::channel();
        Self {
            rt: {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    tokio::runtime::Builder::new_multi_thread()
                }
                #[cfg(target_arch = "wasm32")]
                {
                    tokio::runtime::Builder::new_current_thread()
                }
            }
            .enable_all()
            .build()
            .expect("tokio runtime creation successful"),
            initial_files,
            running: None,
            event_sender,
            event_receiver,
        }
    }
}

impl eframe::App for AppMultiplexer {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Initialize the app instance if not already done
        if self.running.is_none() {
            if let Some(wgpu_render_state) = frame.wgpu_render_state() {
                let device = wgpu_render_state.device.clone();
                let queue = wgpu_render_state.queue.clone();

                let mut instance =
                    AppInstance::new_for_eframe(device, queue, self.event_sender.clone());

                instance.load_files(std::mem::take(&mut self.initial_files));
                self.running = Some(instance);
            }
        }

        // Process user events from the channel
        while let Ok(app_event) = self.event_receiver.try_recv() {
            if let Some(app) = self.running.as_mut() {
                app.handle_user_event(app_event, &self.rt, frame);
            }
        }

        // Render the GUI
        if let Some(app) = self.running.as_mut() {
            for (_, (compositor, texture)) in app.compositors_mut() {
                compositor.rendering_tick_blocking(texture);
            }
            app.render_gui(ctx);
        }
    }
}
