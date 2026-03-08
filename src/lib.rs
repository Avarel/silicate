mod app;
mod gui;
mod window;

mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[cfg(target_arch = "wasm32")]
mod web;

use app::AppEvent;
use std::sync::mpsc::{Receiver, Sender};
use window::AppInstance;

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

pub struct UnifiedRuntime {
    // We use tokio on native and wasm_bindgen_futures on wasm
    #[cfg(not(target_arch = "wasm32"))]
    pub rt: tokio::runtime::Runtime,
}

impl UnifiedRuntime {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> Self {
        Self {
            rt: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime creation successful"),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new() -> Self {
        Self {}
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn<F>(&self, future: F)
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.rt.spawn(future);
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + 'static,
    {
        wasm_bindgen_futures::spawn_local(future);
    }
}

pub struct AppMultiplexer {
    rt: UnifiedRuntime,
    #[cfg(not(target_arch = "wasm32"))]
    initial_files: Vec<PathBuf>,
    running: Option<AppInstance>,
    event_sender: Sender<AppEvent>,
    event_receiver: Receiver<AppEvent>,
}

impl AppMultiplexer {
    pub fn new() -> Self {
        let (event_sender, event_receiver) = std::sync::mpsc::channel();
        Self {
            rt: UnifiedRuntime::new(),
            #[cfg(not(target_arch = "wasm32"))]
            initial_files: Vec::new(),
            running: None,
            event_sender,
            event_receiver,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_initial_files(&mut self, initial_files: Vec<PathBuf>) {
        self.initial_files = initial_files;
    }
}

impl eframe::App for AppMultiplexer {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Initialize the app instance if not already done
        if self.running.is_none() {
            if let Some(wgpu_render_state) = frame.wgpu_render_state() {
                let device = wgpu_render_state.device.clone();
                let queue = wgpu_render_state.queue.clone();

                #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
                let mut instance =
                    AppInstance::new_for_eframe(device, queue, ctx, &self.event_sender);

                #[cfg(not(target_arch = "wasm32"))]
                instance.load_files(std::mem::take(&mut self.initial_files));

                self.running = Some(instance);
            }
        }

        // Process user events from the channel
        while let Ok(app_event) = self.event_receiver.try_recv() {
            if let Some(app) = self.running.as_mut() {
                app.handle_user_event(app_event, &self.rt, ctx, frame);
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
