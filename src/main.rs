mod error;
mod compositor;
mod gui;
mod ns_archive;
mod silica;

use compositor::dev::LogicalDevice;
use silica::ProcreateFile;
use std::error::Error;
use winit::event_loop::EventLoopBuilder;

const INITIAL_WIDTH: u32 = 1200;
const INITIAL_HEIGHT: u32 = 700;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 2 {
        return Ok(());
    }

    let event_loop = EventLoopBuilder::new().build();
    let window = winit::window::WindowBuilder::new()
        .with_decorations(true)
        .with_resizable(true)
        .with_transparent(false)
        .with_title("Procreate Viewer")
        .with_inner_size(winit::dpi::PhysicalSize {
            width: INITIAL_WIDTH,
            height: INITIAL_HEIGHT,
        })
        .build(&event_loop)
        .unwrap();

    let device = futures::executor::block_on(LogicalDevice::with_window(&window)).unwrap();

    let procreate = ProcreateFile::open(&args[1], &device)?;

    gui::start_gui(procreate, device, window, event_loop);
    Ok(())
}
