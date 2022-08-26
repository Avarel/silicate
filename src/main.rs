mod error;
mod gpu;
mod ns_archive;
mod silica;

use crate::{
    gpu::RenderState,
    silica::{BlendingMode, SilicaHierarchy},
};
use futures::executor::block_on;
use gpu::{CompositeLayer, LogicalDevice};
use image::{ImageBuffer, Rgba};
use silica::{ProcreateFile, SilicaGroup};
use std::{
    error::Error,
    fmt::format,
    num::NonZeroU32,
    sync::{atomic::AtomicBool, Arc, RwLock},
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 2 {
        return Ok(());
    }

    let event_loop = winit::event_loop::EventLoop::with_user_event();
    let window = winit::window::WindowBuilder::new()
        .with_decorations(true)
        .with_resizable(true)
        .with_transparent(false)
        .with_title("egui-wgpu_winit example")
        .with_inner_size(winit::dpi::PhysicalSize {
            width: INITIAL_WIDTH,
            height: INITIAL_HEIGHT,
        })
        .build(&event_loop)
        .unwrap();

    // let instance = wgpu::Instance::new(wgpu::Backends::PRIMARY);

    // // WGPU 0.11+ support force fallback (if HW implementation not supported), set it to true or false (optional).
    // let adapter = futures::executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
    //     power_preference: wgpu::PowerPreference::LowPower,
    //     compatible_surface: Some(&surface),
    //     force_fallback_adapter: false,
    // }))
    // .unwrap();

    let device = futures::executor::block_on(LogicalDevice::with_window(&window)).unwrap();

    let procreate = ProcreateFile::open(&args[1], &device)?;

    start_gui(procreate, device, window, event_loop);
    Ok(())
}

pub fn gpu_render(
    pc: &ProcreateFile,
    composite_reference: bool,
    state: &LogicalDevice,
) -> wgpu::TextureView {
    let mut state = RenderState::new(
        pc.size.width,
        pc.size.height,
        (pc.flipped.horizontally, pc.flipped.vertically),
        (!pc.background_hidden).then_some(pc.background_color),
        state,
    );

    // let output_buffer = state.handle.device.create_buffer(&wgpu::BufferDescriptor {
    //     label: None,
    //     size: (state.buffer_dimensions.padded_bytes_per_row * state.buffer_dimensions.height)
    //         as u64,
    //     usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    //     mapped_at_creation: false,
    // });

    if composite_reference {
        state.render(&[CompositeLayer {
            texture: &pc.composite.image,
            clipped: None,
            opacity: 1.0,
            blend: BlendingMode::Normal,
            name: Some("Composite"),
        }]);
    } else {
        state.render(&resolve(&state, &pc.layers));
    }

    // state.handle.queue.submit(Some({
    //     let mut encoder = state
    //         .handle
    //         .device
    //         .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    //     // Copy the data from the texture to the buffer
    //     encoder.copy_texture_to_buffer(
    //         state.composite_texture.as_image_copy(),
    //         wgpu::ImageCopyBuffer {
    //             buffer: &output_buffer,
    //             layout: wgpu::ImageDataLayout {
    //                 offset: 0,
    //                 bytes_per_row: NonZeroU32::new(state.buffer_dimensions.padded_bytes_per_row),
    //                 rows_per_image: None,
    //             },
    //         },
    //         state.texture_extent,
    //     );

    //     encoder.finish()
    // }));

    // let buffer_slice = output_buffer.slice(..);

    // // NOTE: We have to create the mapping THEN device.poll() before await
    // // the future. Otherwise the application will freeze.
    // let (tx, rx) = futures::channel::oneshot::channel();
    // buffer_slice.map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    // state.handle.device.poll(wgpu::Maintain::Wait);
    // block_on(rx).unwrap().unwrap();

    // let data = buffer_slice.get_mapped_range();

    // // eprintln!("Loading data to CPU");
    // // let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(
    // //     state.buffer_dimensions.padded_bytes_per_row as u32 / 4,
    // //     state.buffer_dimensions.height as u32,
    // //     data,
    // // )
    // // .unwrap();
    // // eprintln!("Writing image");

    // eprintln!("Loading data to CPU");
    // let mut buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(
    //     state.buffer_dimensions.padded_bytes_per_row as u32 / 4,
    //     state.buffer_dimensions.height as u32,
    //     data.to_vec(),
    // )
    // .unwrap();
    // eprintln!("Rotating image");

    // buffer = image::imageops::crop_imm(&buffer, 0, 0, pc.size.width, pc.size.height).to_image();
    // match pc.orientation {
    //     0 => {}
    //     1 | 4 => buffer = image::imageops::rotate90(&buffer),
    //     2 => buffer = image::imageops::rotate180(&buffer),
    //     3 => buffer = image::imageops::rotate270(&buffer),
    //     _ => println!("Unknown orientation!"),
    // };
    // eprintln!("Writing image");

    // buffer.save(out_path).unwrap();

    // eprintln!("Finished");
    // drop(buffer);
    // drop(buffer_slice);

    // output_buffer.unmap();
    state
        // .composite_to_srgb()
        .composite_texture
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn resolve<'a>(
    state: &RenderState,
    layers: &'a crate::silica::SilicaGroup,
) -> Vec<CompositeLayer<'a>> {
    fn inner<'a>(
        state: &RenderState,
        layers: &'a crate::silica::SilicaGroup,
        composite_layers: &mut Vec<CompositeLayer<'a>>,
    ) {
        let mut mask_layer: Option<(usize, &crate::silica::SilicaLayer)> = None;

        for (index, layer) in layers.children.iter().rev().enumerate() {
            match layer {
                SilicaHierarchy::Group(group) if !group.hidden => {
                    inner(state, group, composite_layers);
                }
                SilicaHierarchy::Layer(layer) if !layer.hidden => {
                    if let Some((_, mask_layer)) = mask_layer {
                        if layer.clipped && mask_layer.hidden {
                            // eprintln!("Hidden layer {:?} due to clip to hidden", layer.name);
                            continue;
                        }
                    }

                    let gpu_texture = &layer.image;

                    composite_layers.push(CompositeLayer {
                        texture: gpu_texture,
                        clipped: layer.clipped.then(|| mask_layer.unwrap().0),
                        opacity: layer.opacity,
                        blend: layer.blend,
                        name: layer.name.as_deref(),
                    });

                    if !layer.clipped {
                        mask_layer = Some((index, layer));
                    }

                    // eprintln!("Resolved layer {:?}: {}", layer.name, layer.blend);
                }
                _ => continue,
            }
        }
    }

    let mut composite_layers = Vec::new();
    inner(&state, layers, &mut composite_layers);
    composite_layers
}

use std::iter;
use std::time::Instant;

use egui::FontDefinitions;
use egui_wgpu_backend::{RenderPass, ScreenDescriptor};
use egui_winit_platform::{Platform, PlatformDescriptor};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::{event::Event::*, event_loop};
const INITIAL_WIDTH: u32 = 600;
const INITIAL_HEIGHT: u32 = 600;

fn layout_layers(ui: &mut egui::Ui, layers: &mut SilicaGroup, i: &mut usize) {
    for layer in &mut layers.children {
        *i += 1;
        match layer {
            SilicaHierarchy::Layer(l) => {
                ui.push_id(*i, |ui| {
                    *i += 1;
                    ui.collapsing(l.name.as_deref().unwrap_or(""), |ui| {
                        ui.checkbox(&mut l.hidden, "Hidden").changed();
                        ui.add(egui::Slider::new(&mut l.opacity, 0.0..=1.0));
                    });
                });
            }
            SilicaHierarchy::Group(h) => {
                ui.push_id(*i, |ui| {
                    *i += 1;
                    ui.collapsing(h.name.to_string().as_str(), |ui| {
                        ui.checkbox(&mut h.hidden, "Hidden").changed();
                        layout_layers(ui, h, i);
                    })
                });
            }
        }
    }
}

fn layout_layers_ro(ui: &mut egui::Ui, layers: &SilicaGroup, i: &mut usize) {
    for layer in &layers.children {
        *i += 1;
        match layer {
            SilicaHierarchy::Layer(l) => {
                ui.push_id(*i, |ui| {
                    *i += 1;
                    ui.collapsing(l.name.as_deref().unwrap_or(""), |ui| {
                        let mut z = l.hidden;
                        ui.checkbox(&mut z, "Hidden");
                        let mut z = l.opacity;
                        ui.add(egui::Slider::new(&mut z, 0.0..=1.0));
                    });
                });
            }
            SilicaHierarchy::Group(h) => {
                ui.push_id(*i, |ui| {
                    *i += 1;
                    ui.collapsing(h.name.to_string().as_str(), |ui| {
                        let mut z = h.hidden;
                        ui.checkbox(&mut z, "Hidden");
                        layout_layers_ro(ui, h, i);
                    })
                });
            }
        }
    }
}

/// A simple egui + wgpu + winit based example.
fn start_gui(
    pc: ProcreateFile,
    dev: LogicalDevice,
    window: winit::window::Window,
    event_loop: winit::event_loop::EventLoop<()>,
) {
    let surface = unsafe { dev.instance.create_surface(&window) };

    let tex = Arc::new(RwLock::new(gpu_render(&pc, false, &dev)));

    let size = window.inner_size();
    let surface_format = surface.get_supported_formats(&dev.adapter)[0];
    let mut surface_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: size.width as u32,
        height: size.height as u32,
        present_mode: wgpu::PresentMode::Fifo,
    };
    surface.configure(&dev.device, &surface_config);

    // We use the egui_winit_platform crate as the platform.
    let mut platform = Platform::new(PlatformDescriptor {
        physical_width: size.width as u32,
        physical_height: size.height as u32,
        scale_factor: window.scale_factor(),
        font_definitions: FontDefinitions::default(),
        style: Default::default(),
    });

    let dev = Arc::new(dev);
    let pc = Arc::new(RwLock::new(pc));
    let running = Arc::new(AtomicBool::new(true));

    // We use the egui_wgpu_backend crate as the render backend.
    let mut egui_rpass = RenderPass::new(&dev.device, surface_format, 1);

    let egui_tex = egui_rpass.egui_texture_from_wgpu_texture(
        &dev.device,
        &tex.read().unwrap(),
        wgpu::FilterMode::Linear,
    );

    {
        let tex = tex.clone();
        let running = running.clone();
        let dev = dev.clone();
        let pc = pc.clone();
        // let egui_rpass = egui_rpass.clone();
        std::thread::spawn(move || {
            while running.load(std::sync::atomic::Ordering::SeqCst) {
                if let Ok(pc) = pc.try_read() {
                    *tex.write().unwrap() = gpu_render(&pc, false, &dev);
                }
                
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        });
    }

    // egui_rpass.update_egui_texture_from_wgpu_texture(device, texture, texture_filter, id)

    // // Display the demo application that ships with egui.
    // let mut demo_app = egui_demo_lib::DemoWindows::default();

    let mut show = true;

    let start_time = Instant::now();
    event_loop.run(move |event, _, control_flow| {
        // Pass the winit events to the platform integration.
        platform.handle_event(&event);

        match event {
            RedrawRequested(..) => {
                let mut need_redraw = false;

                platform.update_time(start_time.elapsed().as_secs_f64());

                let output_frame = match surface.get_current_texture() {
                    Ok(frame) => frame,
                    Err(wgpu::SurfaceError::Outdated) => {
                        // This error occurs when the app is minimized on Windows.
                        // Silently return here to prevent spamming the console with:
                        // "The underlying surface has changed, and therefore the swap chain must be updated"
                        return;
                    }
                    Err(e) => {
                        eprintln!("Dropped frame with error: {}", e);
                        return;
                    }
                };
                let output_view = output_frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                // Begin to draw the UI frame.
                platform.begin_frame();

                egui::CentralPanel::default().show(&platform.context(), |ui| {
                    egui::Area::new("image")
                        .default_pos(egui::pos2(0.0, 0.0))
                        .drag_bounds(egui::Rect::EVERYTHING)
                        .show(ui.ctx(), |ui| {
                            if show {
                                ui.image(egui_tex, (1000.0, 1000.0));
                            }
                        });
                });

                if let Ok(mut z) = pc.try_write() {
                    egui::Window::new("Layers").show(&platform.context(), |ui| {
                        if ui.button("Toggle Canvas").clicked() {
                            show = !show;
                            need_redraw = true;
                        }
                        let mut i = 0;
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            layout_layers(ui, &mut z.layers, &mut i);
                        })
                    });
                } else {
                    egui::Window::new("Layers").enabled(false).show(&platform.context(), |ui| {
                        if ui.button("Toggle Canvas").clicked() {
                            show = !show;
                            need_redraw = true;
                        }
                        let mut i = 0;
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            layout_layers_ro(ui, &pc.read().unwrap().layers, &mut i);
                        })
                    });
                }

                if let Ok(z) = tex.try_read() {
                    egui_rpass
                        .update_egui_texture_from_wgpu_texture(
                            &dev.device,
                            &z,
                            wgpu::FilterMode::Linear,
                            egui_tex,
                        )
                        .unwrap();
                }

                // End the UI frame. We could now handle the output and draw the UI with the backend.
                let full_output = platform.end_frame(Some(&window));
                let paint_jobs = platform.context().tessellate(full_output.shapes);

                let mut encoder =
                    dev.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("encoder"),
                        });

                // Upload all resources for the GPU.
                let screen_descriptor = ScreenDescriptor {
                    physical_width: surface_config.width,
                    physical_height: surface_config.height,
                    scale_factor: window.scale_factor() as f32,
                };
                let tdelta: egui::TexturesDelta = full_output.textures_delta;
                egui_rpass
                    .add_textures(&dev.device, &dev.queue, &tdelta)
                    .expect("add texture ok");
                egui_rpass.update_buffers(&dev.device, &dev.queue, &paint_jobs, &screen_descriptor);

                // Record all render passes.
                egui_rpass
                    .execute(
                        &mut encoder,
                        &output_view,
                        &paint_jobs,
                        &screen_descriptor,
                        Some(wgpu::Color::BLACK),
                    )
                    .unwrap();
                // Submit the commands.
                dev.queue.submit(iter::once(encoder.finish()));

                // Redraw egui
                output_frame.present();

                egui_rpass
                    .remove_textures(tdelta)
                    .expect("remove texture ok");

                // if need_redraw {
                //     *control_flow = ControlFlow::Poll;
                // } else {
                //     *control_flow = ControlFlow::Wait;
                // }
            }
            MainEventsCleared => {
                window.request_redraw();
            }
            WindowEvent { event, .. } => match event {
                winit::event::WindowEvent::Resized(size) => {
                    // Resize with 0 width and height is used by winit to signal a minimize event on Windows.
                    // See: https://github.com/rust-windowing/winit/issues/208
                    // This solves an issue where the app would panic when minimizing on Windows.
                    if size.width > 0 && size.height > 0 {
                        surface_config.width = size.width;
                        surface_config.height = size.height;
                        surface.configure(&dev.device, &surface_config);
                    }
                }
                winit::event::WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                    running.store(false, std::sync::atomic::Ordering::SeqCst);
                }
                _ => {}
            },
            _ => (),
        }
    });
}
