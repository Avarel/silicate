// use egui_wgpu::renderer::{RenderPass, ScreenDescriptor};
// use wgpu;
// use winit::{
//     event::{Event, WindowEvent},
//     event_loop::{ControlFlow, EventLoop},
//     window::Window,
// };

// use crate::compositor::dev::LogicalDevice;

// pub fn run(dev: LogicalDevice, event_loop: EventLoop<()>, window: Window) {
//     let window_size = window.inner_size();

//     let LogicalDevice {
//         instance,
//         device,
//         adapter,
//         queue,
//         // chunks,
//         ..
//     } = dev;

//     let surface = unsafe { instance.create_surface(&window) };

//     let surface_format = surface.get_supported_formats(&adapter)[0];

//     let swap_chain_format = wgpu::TextureFormat::Bgra8UnormSrgb;

//     let mut surface_config = wgpu::SurfaceConfiguration {
//         usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
//         format: swap_chain_format,
//         width: window_size.width,
//         height: window_size.height,
//         present_mode: wgpu::PresentMode::Fifo,
//     };

//     surface.configure(&device, &surface_config);

//     let mut state = egui_winit::State::new(&event_loop);
//     state.set_pixels_per_point(window.scale_factor() as f32);
//     let context = egui::Context::default();
//     context.set_pixels_per_point(window.scale_factor() as f32);
    
//     let mut egui_rpass = RenderPass::new(&device, surface_format, 1);

//     event_loop.run(move |event, _, control_flow| {
//         *control_flow = ControlFlow::Poll;

//         match event {
//             Event::WindowEvent { event, .. } => {
//                 match event {
//                     WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
//                     WindowEvent::Resized(size) => {
//                         // Resize with 0 width and height is used by winit to signal a minimize event on Windows.
//                         // See: https://github.com/rust-windowing/winit/issues/208
//                         // This solves an issue where the app would panic when minimizing on Windows.
//                         if size.width > 0 && size.height > 0 {
//                             surface_config.width = size.width;
//                             surface_config.height = size.height;
//                             surface.configure(&device, &surface_config);
//                         }
//                     }
//                     WindowEvent::DroppedFile(file) => {
//                         println!("File dropped: {:?}", file.as_path().display().to_string());
//                     }
//                     _ => {
//                         state.on_event(&context, &event);
//                     }
//                 }
//             }
//             Event::MainEventsCleared => window.request_redraw(),
//             Event::RedrawRequested(..) => {
//                 let output_frame = surface
//                     .get_current_texture()
//                     .expect("Failed to get surface output texture");
//                 let output_view = output_frame
//                     .texture
//                     .create_view(&wgpu::TextureViewDescriptor::default());

//                 let input = state.take_egui_input(&window);

//                 context.begin_frame(input);
//                 egui::Window::new("K4 Kahlberg").show(&context, |ui| {
//                     ui.heading("Objects");
//                     ui.label("Currently there are no objects here.");
//                     ui.separator();
//                     ui.heading("Settings");
//                     ui.label("Show fog");
//                     ui.label("Show crosshair");
//                     ui.label("See https://github.com/emilk/egui for how to make other UI elements");
//                     if ui.button("Switch to light mode").clicked() {
//                         //egui::widgets::global_dark_light_mode_switch(ui);
//                         context.set_visuals(egui::Visuals::light());
//                     }
//                 });
//                 let output = context.end_frame();

//                 let paint_jobs = context.tessellate(output.shapes);

//                 // Upload all resources for the GPU.
//                 let screen_descriptor = ScreenDescriptor {
//                     size_in_pixels: [surface_config.width, surface_config.height],
//                     pixels_per_point: window.scale_factor() as f32,
//                 };

//                 for (id, image_delta) in &output.textures_delta.set {
//                     egui_rpass.update_texture(&device, &queue, *id, image_delta);
//                 }
//                 for id in &output.textures_delta.free {
//                     egui_rpass.free_texture(id);
//                 }
//                 egui_rpass.update_buffers(&device, &queue, &paint_jobs, &screen_descriptor);

//                 {
//                     let mut encoder = device
//                         .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

//                     egui_rpass.execute(
//                         &mut encoder,
//                         &output_view,
//                         &paint_jobs,
//                         &screen_descriptor,
//                         Some(wgpu::Color::BLACK),
//                     );
//                     queue.submit(Some(encoder.finish()));
//                 }
//                 output_frame.present();
//             }
//             _ => (),
//         }
//     });
// }
