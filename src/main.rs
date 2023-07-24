mod compositor;
mod error;
mod gui;
mod ns_archive;
mod silica;

use std::error::Error;
use winit::{dpi::PhysicalSize, event_loop::EventLoopBuilder, window::WindowBuilder};

const INITIAL_SIZE: PhysicalSize<u32> = PhysicalSize {
    width: 1200,
    height: 700,
};

fn main() -> Result<(), Box<dyn Error>> {
    let taskbar_icon =
        winit::window::Icon::from_rgba(include_bytes!("../assets/icon.rgba").to_vec(), 240, 240)
            .ok();

    let event_loop = EventLoopBuilder::with_user_event().build();
    let window = WindowBuilder::new()
        .with_decorations(true)
        .with_resizable(true)
        .with_transparent(false)
        .with_title("Silicate")
        .with_inner_size(INITIAL_SIZE)
        .with_window_icon(taskbar_icon)
        .build(&event_loop)?;

    gui::start_gui(window, event_loop)
}
