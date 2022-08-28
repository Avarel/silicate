mod compositor;
mod error;
mod gui;
mod ns_archive;
mod silica;

use compositor::dev::LogicalDevice;
use std::error::Error;
use winit::{dpi::PhysicalSize, event_loop::EventLoopBuilder, window::WindowBuilder};

const INITIAL_SIZE: PhysicalSize<u32> = PhysicalSize {
    width: 1200,
    height: 700,
};

fn main() -> Result<(), Box<dyn Error>> {
    // let taskbar_icon =
    //     winit::window::Icon::from_rgba(include_bytes!("../procreate-240.rgba").to_vec(), 240, 240)
    //         .ok();

    let event_loop = EventLoopBuilder::new().build();

    let window = WindowBuilder::new()
        .with_decorations(true)
        .with_resizable(true)
        .with_transparent(false)
        .with_title("Procreate Viewer")
        .with_inner_size(INITIAL_SIZE)
        // .with_window_icon(taskbar_icon)
        .build(&event_loop)
        .unwrap();

    let (dev, surface) = futures::executor::block_on(LogicalDevice::with_window(&window)).unwrap();

    gui::start_gui(dev, surface, window, event_loop);
    Ok(())
}
