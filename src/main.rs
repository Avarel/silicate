use clap::Parser;
use silicate::AppMultiplexer;
use std::path::PathBuf;

const INITIAL_SIZE: [f32; 2] = [1200.0, 700.0];

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
            if let Some(eframe::egui_wgpu::RenderState { adapter, .. }) =
                cc.wgpu_render_state.as_ref()
            {
                dbg!(adapter.get_info());
                dbg!(adapter.limits());
            }
            Ok(Box::new(AppMultiplexer::new(args.files)))
        }),
    )
}
