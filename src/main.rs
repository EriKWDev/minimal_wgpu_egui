#![allow(irrefutable_let_patterns)]

use winit::{
    event::{Event, WindowEvent},
    event_loop::{EventLoop, EventLoopBuilder},
    window::Window,
};

/// return true if quit
fn gui(ctx: &egui::Context) -> bool {
    let mut should_quit = false;

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Timings");
        ui.add_space(5.0);
        ui.label("draw 0.3ms");
        should_quit = ui.button("Quit").clicked();
    });

    should_quit
}

async fn run(event_loop: EventLoop<()>, window: Window, egui_ctx: egui::Context) {
    let mut size = window.inner_size();
    size.width = size.width.max(1);
    size.height = size.height.max(1);

    let instance = wgpu::Instance::default();

    let surface = instance.create_surface(&window).unwrap();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await
        .expect("Failed to find an appropriate adapter");

    let trace_dir = std::env::var("WGPU_TRACE");
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
            },
            trace_dir.ok().as_ref().map(std::path::Path::new),
        )
        .await
        .expect("Failed to create device");

    let swapchain_capabilities = surface.get_capabilities(&adapter);
    let swapchain_format = swapchain_capabilities.formats[0];

    let mut config = surface
        .get_default_config(&adapter, size.width, size.height)
        .unwrap();
    surface.configure(&device, &config);

    let ppp = egui_winit::pixels_per_point(&egui_ctx, &window);
    let mut egui_renderer = egui_wgpu::Renderer::new(&device, swapchain_format, None, 1);
    let mut egui_input = egui_winit::State::new(
        egui_ctx.clone(),
        egui::ViewportId::ROOT,
        &window,
        Some(ppp),
        Some(device.limits().max_texture_dimension_2d as usize),
    );

    let window = &window;
    event_loop
        .run(|event, target| {
            match event {
                Event::AboutToWait => {
                    window.request_redraw();
                }

                Event::WindowEvent {
                    window_id: _,
                    event,
                } => {
                    let resp = egui_input.on_window_event(&window, &event);
                    if resp.repaint {
                        window.request_redraw();
                    }

                    match event {
                        WindowEvent::Resized(new_size) => {
                            // Reconfigure the surface with the new size
                            config.width = new_size.width.max(1);
                            config.height = new_size.height.max(1);
                            surface.configure(&device, &config);
                            // On macos the window needs to be redrawn manually after resizing
                            window.request_redraw();
                        }

                        WindowEvent::RedrawRequested => {
                            let window_size = window.inner_size();
                            let ppp = egui_winit::pixels_per_point(&egui_ctx, &window);
                            let screen_desc = egui_wgpu::ScreenDescriptor {
                                size_in_pixels: [window_size.width, window_size.height],
                                pixels_per_point: ppp,
                            };

                            let egui_output = {
                                /*
                                    NOTE: Gui
                                */
                                egui_ctx.begin_frame(egui_input.take_egui_input(&window));
                                if gui(&egui_ctx) {
                                    target.exit();
                                }
                                egui_ctx.end_frame()
                            };
                            for (id, delta) in egui_output.textures_delta.set {
                                egui_renderer.update_texture(&device, &queue, id, &delta);
                            }
                            for id in egui_output.textures_delta.free {
                                egui_renderer.free_texture(&id);
                            }
                            let paint_jobs = egui_ctx.tessellate(egui_output.shapes, ppp);

                            let mut encoder = device.create_command_encoder(&Default::default());
                            egui_renderer.update_buffers(
                                &device,
                                &queue,
                                &mut encoder,
                                &paint_jobs,
                                &screen_desc,
                            );
                            queue.submit([encoder.finish()]);

                            let frame = surface
                                .get_current_texture()
                                .expect("Failed to acquire next swap chain texture");

                            let frame_view =
                                frame.texture.create_view(&wgpu::TextureViewDescriptor {
                                    format: Some(config.format),
                                    ..Default::default()
                                });

                            let mut encoder = device.create_command_encoder(&Default::default());
                            if let mut pass =
                                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                    label: None,
                                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                        view: &frame_view,
                                        resolve_target: None,
                                        ops: wgpu::Operations {
                                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                            store: wgpu::StoreOp::Store,
                                        },
                                    })],
                                    ..Default::default()
                                })
                            {
                                egui_renderer.render(&mut pass, &paint_jobs, &screen_desc);
                            };
                            queue.submit([encoder.finish()]);

                            window.pre_present_notify();
                            frame.present();
                        }

                        WindowEvent::CloseRequested => target.exit(),

                        _ => {}
                    };
                }

                _ => {}
            }
        })
        .unwrap();
}

pub fn main() {
    // NOTE: Force X11
    // use winit::platform::x11::EventLoopBuilderExtX11;
    // let event_loop = EventLoopBuilder::new().with_x11().build().unwrap();

    let event_loop = EventLoopBuilder::new().build().unwrap();
    let egui_ctx = egui::Context::default();

    let window =
        egui_winit::create_window(&egui_ctx, &event_loop, &egui::ViewportBuilder::default())
            .unwrap();
    pollster::block_on(run(event_loop, window, egui_ctx));
}
