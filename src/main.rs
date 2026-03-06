use silicate::AppMultiplexer;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    use clap::Parser;
    use std::path::PathBuf;
    const INITIAL_SIZE: [f32; 2] = [1200.0, 700.0];

    #[derive(Parser, Debug)]
    #[command(author, version, about)]
    struct Args {
        /// Files to open in the pager
        files: Vec<PathBuf>,
    }

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "info"
        },
    ))
    .init();

    let args = Args::parse();

    let icon_data = include_bytes!("../assets/favicon.rgba").to_vec();
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
        wgpu_options: wgpu_config(),
        ..Default::default()
    };

    eframe::run_native(
        "Silicate",
        options,
        Box::new(|cc| {
            if let Some(eframe::egui_wgpu::RenderState { adapter, .. }) =
                cc.wgpu_render_state.as_ref()
            {
                log::debug!("{:?}", adapter.get_info());
                log::debug!("{:?}", adapter.limits());
            }
            Ok(Box::new(AppMultiplexer::new(args.files)))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id was not a HtmlCanvasElement");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions {
                    wgpu_options: wgpu_config(),
                    ..Default::default()
                },
                Box::new(|_cc| Ok(Box::new(AppMultiplexer::new(Vec::new())))),
            )
            .await;

        // Remove the loading text and spinner:
        if let Some(loading_text) = document.get_element_by_id("loading_text") {
            match start_result {
                Ok(_) => {
                    loading_text.remove();
                }
                Err(e) => {
                    loading_text.set_inner_html(
                        "<p> The app has crashed. See the developer console for details. </p>",
                    );
                    panic!("Failed to start eframe: {e:?}");
                }
            }
        }
    });
}

fn wgpu_config() -> eframe::egui_wgpu::WgpuConfiguration {
    eframe::egui_wgpu::WgpuConfiguration {
        wgpu_setup: eframe::egui_wgpu::WgpuSetupCreateNew {
            native_adapter_selector: Some(Arc::new(wgpu_adapter_selector)),
            ..Default::default()
        }
        .into(),
        ..Default::default()
    }
}

fn wgpu_adapter_selector(
    adapters: &[eframe::wgpu::Adapter],
    _surface: Option<&eframe::wgpu::Surface<'_>>,
) -> Result<eframe::wgpu::Adapter, String> {
    use eframe::egui_wgpu::wgpu;
    let mut adapters = adapters.iter().collect::<Vec<_>>();

    for adapter in &adapters {
        log::debug!("Found adapter: {:?}", adapter.get_info());
    }

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
