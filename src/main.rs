mod compositor;
mod error;
mod gui;
mod ns_archive;
mod silica;

use compositor::dev::GpuHandle;
use egui_winit::winit::{dpi::PhysicalSize, event_loop::EventLoopBuilder, window::WindowBuilder};
use gui::app::App;
use std::{error::Error, sync::Arc};

pub use egui_winit::winit;

const INITIAL_SIZE: PhysicalSize<u32> = PhysicalSize {
    width: 1200,
    height: 700,
};

fn main() -> Result<(), Box<dyn Error>> {
    let taskbar_icon = egui_winit::winit::window::Icon::from_rgba(
        include_bytes!("../assets/icon.rgba").to_vec(),
        240,
        240,
    )
    .ok();

    let event_loop = EventLoopBuilder::with_user_event().build()?;
    let window = WindowBuilder::new()
        .with_decorations(true)
        .with_resizable(true)
        .with_transparent(false)
        .with_title("Silicate")
        .with_inner_size(INITIAL_SIZE)
        .with_window_icon(taskbar_icon)
        .build(&event_loop)?;

    let rt = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("tokio runtime creation successful")
    );

    let (dev, surface) = rt.block_on(GpuHandle::with_window(&window)).unwrap();
    let app = Arc::new(App::new(dev, rt, event_loop.create_proxy()));
    Ok(app.run(&window, surface, event_loop)?)
}
