use clap::Parser;
use silicate::AppMultiplexer;
use std::{path::PathBuf, sync::Arc};

const INITIAL_SIZE: [f32; 2] = [1200.0, 700.0];

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Files to open in the pager
    files: Vec<PathBuf>,
}

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "info"
        },
    ))
    .init();

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
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: eframe::egui_wgpu::WgpuSetupCreateNew {
                native_adapter_selector: Some(Arc::new(adapter_selector)),
                ..Default::default()
            }
            .into(),
            ..Default::default()
        },
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

fn adapter_selector(
    adapters: &[eframe::wgpu::Adapter],
    _surface: Option<&eframe::wgpu::Surface<'_>>,
) -> Result<eframe::wgpu::Adapter, String> {
    use eframe::egui_wgpu::wgpu;
    let mut adapters = adapters.iter().collect::<Vec<_>>();

    // Prefer DX12 and Metal, then Vulkan, then OpenGL
    adapters.sort_by_key(|a| match a.get_info().backend {
        wgpu::Backend::Dx12 | wgpu::Backend::Metal => 0,
        wgpu::Backend::Vulkan => 1,
        wgpu::Backend::Gl => 2,
        wgpu::Backend::BrowserWebGpu => 3,
        wgpu::Backend::Noop => 4,
    });

    // Prefer discrete GPU over integrated GPU, otherwise CPU.
    adapters.sort_by_key(|a| match a.get_info().device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::VirtualGpu => 1,
        wgpu::DeviceType::Cpu | wgpu::DeviceType::Other => 2,
    });

    adapters
        .first()
        .map(|a| (*a).clone())
        .ok_or_else(|| "No adapter found".to_owned())
}
