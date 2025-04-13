mod compositor;
mod error;
mod gui;
mod ns_archive;
mod silica;

use compositor::dev::GpuHandle;
use egui_winit::winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::Window,
};
use gui::{app::{self, UserEvent}, AppInstance};
use std::{error::Error, sync::Arc};
use tokio::runtime::Runtime;

pub use egui_winit::winit;

const INITIAL_SIZE: PhysicalSize<u32> = PhysicalSize {
    width: 1200,
    height: 700,
};

struct AppMultiplexer {
    rt: Arc<Runtime>,
    running: Option<AppInstance>,
    proxy: EventLoopProxy<UserEvent>,
}

impl AppMultiplexer {
    fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            rt: Arc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime creation successful"),
            ),
            running: None,
            proxy,
        }
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
                .block_on(GpuHandle::with_window(window.clone()))
                .unwrap();

            let app = AppInstance::new(dev, self.rt.clone(), surface, window, self.proxy.clone());
            self.running = Some(app);
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

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::<app::UserEvent>::with_user_event()
        .build()
        .unwrap();

    let proxy = event_loop.create_proxy();

    Ok(event_loop.run_app(&mut AppMultiplexer::new(proxy))?)
}
