mod app;
mod gui;
mod window;

use app::AppEvent;
use clap::Parser;
use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
};
use tokio::runtime::Runtime;
use window::AppInstance;

const INITIAL_SIZE: [f32; 2] = [1200.0, 700.0];

struct AppMultiplexer {
    rt: Runtime,
    initial_files: Vec<PathBuf>,
    running: Option<AppInstance>,
    event_sender: Sender<AppEvent>,
    event_receiver: Receiver<AppEvent>,
}

impl AppMultiplexer {
    fn new(initial_files: Vec<PathBuf>) -> Self {
        let (event_sender, event_receiver) = std::sync::mpsc::channel();
        Self {
            rt: tokio::runtime::Builder::new_multi_thread()
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

                let mut instance = AppInstance::new_for_eframe(
                    device,
                    queue,
                    self.event_sender.clone(),
                );

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
            app.render_gui(ctx);
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Files to open in the pager
    files: Vec<PathBuf>,
}

fn main() -> eframe::Result {
    let args = Args::parse();

    let icon_data = include_bytes!("../assets/icon.rgba").to_vec();
    let taskbar_icon = egui::IconData {
        rgba: icon_data,
        width: 240,
        height: 240,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(INITIAL_SIZE)
            .with_min_inner_size(INITIAL_SIZE)
            .with_decorations(true)
            .with_resizable(true)
            .with_transparent(true)
            .with_title("Silicate")
            .with_icon(std::sync::Arc::new(taskbar_icon)),
        renderer: eframe::Renderer::Wgpu,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "Silicate",
        options,
        Box::new(|cc| {
            if let Some(eframe::egui_wgpu::RenderState { adapter, .. }) = cc.wgpu_render_state.as_ref() {
                dbg!(adapter.get_info());
                dbg!(adapter.limits());
            }
            Ok(Box::new(AppMultiplexer::new(args.files)))
        }),
    )
}
