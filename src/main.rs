mod app;
mod dev;
mod gui;
mod window;

use app::AppEvent;
use clap::Parser;
use dev::GpuHandle;
use egui_winit::winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event_loop::{ActiveEventLoop, EventLoop},
    window::Window,
};
use std::{
    error::Error,
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{Receiver, Sender},
    },
};
use tokio::runtime::Runtime;
use window::AppInstance;

pub use egui_winit::winit;

const INITIAL_SIZE: LogicalSize<u32> = LogicalSize {
    width: 1200,
    height: 700,
};

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

    /// Create a GPU handle with a surface target compatible with the window.
    pub async fn handle_with_window(
        window: Arc<egui_winit::winit::window::Window>,
    ) -> Option<(GpuHandle, wgpu::Surface<'static>)> {
        let instance = wgpu::Instance::new(&GpuHandle::instance_descriptor());
        let surface = instance.create_surface(window).ok()?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..GpuHandle::ADAPTER_OPTIONS
            })
            .await
            .ok()?;
        GpuHandle::from_adapter(instance, adapter)
            .await
            .map(|dev| (dev, surface))
    }
}

impl ApplicationHandler for AppMultiplexer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }

        let taskbar_icon = egui_winit::winit::window::Icon::from_rgba(
            include_bytes!("../assets/icon.rgba").to_vec(),
            240,
            240,
        )
        .ok();

        let window_attributes = Window::default_attributes()
            .with_decorations(true)
            .with_resizable(true)
            .with_transparent(true)
            .with_blur(true)
            .with_title("Silicate")
            .with_min_inner_size(INITIAL_SIZE)
            .with_window_icon(taskbar_icon);

        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        let (dev, surface) = self
            .rt
            .block_on(Self::handle_with_window(window.clone()))
            .unwrap();

        let mut instance = AppInstance::new(dev, surface, window, self.event_sender.clone());
        instance.load_files(std::mem::take(&mut self.initial_files));

        self.running = Some(instance);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(app) = self.running.as_mut() else {
            return;
        };
        app.handle_event(event, event_loop, &self.rt);

        // Poll for user events from the channel
        while let Ok(app_event) = self.event_receiver.try_recv() {
            app.handle_user_event(app_event, &self.rt);
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Files to open in the pager
    files: Vec<PathBuf>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let event_loop = EventLoop::new().unwrap();

    Ok(event_loop.run_app(&mut AppMultiplexer::new(args.files))?)
}
