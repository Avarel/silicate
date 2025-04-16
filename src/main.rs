mod gui;

use clap::Parser;
use egui_wgpu::wgpu;
use egui_winit::winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::Window,
};
use gui::{
    app::{self, UserEvent},
    AppInstance,
};
use silicate_compositor::dev::GpuHandle;
use std::{error::Error, path::PathBuf, sync::Arc};
use tokio::runtime::Runtime;

pub use egui_winit::winit;

const INITIAL_SIZE: PhysicalSize<u32> = PhysicalSize {
    width: 1200,
    height: 700,
};

struct AppMultiplexer {
    rt: Arc<Runtime>,
    initial_file: Vec<PathBuf>,
    running: Option<AppInstance>,
    proxy: EventLoopProxy<UserEvent>,
}

impl AppMultiplexer {
    fn new(initial_file: Vec<PathBuf>, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            rt: Arc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime creation successful"),
            ),
            initial_file,
            running: None,
            proxy,
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
            .await?;
        GpuHandle::from_adapter(instance, adapter)
            .await
            .map(|dev| (dev, surface))
    }
}

impl ApplicationHandler<UserEvent> for AppMultiplexer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.running.is_none() {
            let taskbar_icon = egui_winit::winit::window::Icon::from_rgba(
                include_bytes!("../assets/icon.rgba").to_vec(),
                240,
                240,
            )
            .ok();

            let window_attributes = Window::default_attributes()
                .with_decorations(true)
                .with_resizable(true)
                .with_transparent(false)
                .with_title("Silicate")
                .with_inner_size(INITIAL_SIZE)
                .with_window_icon(taskbar_icon);

            let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
            let (dev, surface) = self
                .rt
                .block_on(Self::handle_with_window(window.clone()))
                .unwrap();

            let instance =
                AppInstance::new(dev, self.rt.clone(), surface, window, self.proxy.clone());

            for path in self.initial_file.drain(..) {
                let app = &instance.app;
                match app.load_file(path) {
                    Err(err) => {
                        app.toasts
                            .lock()
                            .error(format!("File from drag/drop failed to load. Reason: {err}"));
                    }
                    Ok(key) => {
                        app.toasts.lock().success("Loaded file from command line.");
                        app.new_instances
                            .blocking_send((
                                egui_dock::SurfaceIndex::main(),
                                egui_dock::NodeIndex::root(),
                                key,
                            ))
                            .unwrap();
                    }
                }
            }

            self.running = Some(instance);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        if let Some(app) = self.running.as_mut() {
            app.handle_event(event, event_loop);
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: UserEvent) {
        if let Some(app) = self.running.as_mut() {
            app.handle_user_event(event);
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

    let event_loop = EventLoop::<app::UserEvent>::with_user_event()
        .build()
        .unwrap();

    let proxy = event_loop.create_proxy();

    Ok(event_loop.run_app(&mut AppMultiplexer::new(args.files, proxy))?)
}
