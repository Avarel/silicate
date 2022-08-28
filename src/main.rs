mod compositor;
mod error;
mod gui;
mod ns_archive;
mod silica;

use compositor::dev::LogicalDevice;
use silica::ProcreateFile;
use std::error::Error;
use winit::{
    dpi::PhysicalSize, event_loop::EventLoopBuilder, platform::windows::WindowBuilderExtWindows,
    window::WindowBuilder,
};

const INITIAL_SIZE: PhysicalSize<u32> = PhysicalSize {
    width: 1200,
    height: 700,
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 2 {
        return Ok(());
    }

    let taskbar_icon =
        winit::window::Icon::from_rgba(include_bytes!("../procreate-240.rgba").to_vec(), 240, 240)
            .ok();

    let event_loop = EventLoopBuilder::new().build();

    let window = WindowBuilder::new()
        .with_decorations(true)
        .with_resizable(true)
        .with_transparent(false)
        .with_title("Procreate Viewer")
        .with_inner_size(INITIAL_SIZE)
        .with_window_icon(taskbar_icon)
        .build(&event_loop)
        .unwrap();

    let device = futures::executor::block_on(LogicalDevice::with_window(&window)).unwrap();

    let procreate = ProcreateFile::open(&args[1], &device)?;

    gui::start_gui(procreate, device, window, event_loop);
    Ok(())
}
