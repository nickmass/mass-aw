use glium::{
    backend::glutin,
    glutin::{Api, GlRequest},
};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
};

use engine::video::Page;
use engine::Executor;
use engine::Input;

mod directory;
mod gfx;
mod input;
mod shaders;

use directory::DirectoryIo;
use gfx::GlGfx;
use input::WinitInput;

const BYPASS_COPY_PROTECTION: bool = true;

pub enum UserEvent {
    Blit(Page),
    Copy(Page, Page, i16),
    Fill(Page, u8),
    Select(Page),
    String(&'static str, u8, i16, i16),
}

fn main() {
    let mut args = std::env::args();
    let _ = args.next();

    let mut game_path = None;
    let mut scale = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-d" | "--data-path" => game_path = args.next(),
            "-s" | "--scale" => scale = args.next().and_then(|s| s.parse().ok()),
            _ => (),
        }
    }

    let event_loop: EventLoop<UserEvent> = EventLoop::with_user_event();
    let window_builder = winit::window::WindowBuilder::new()
        .with_title("Another World")
        .with_inner_size(winit::dpi::PhysicalSize {
            width: 320 * scale.unwrap_or(1),
            height: 200 * scale.unwrap_or(1),
        });
    let context_builder = glutin::glutin::ContextBuilder::new()
        .with_srgb(true)
        .with_depth_buffer(16)
        .with_gl(GlRequest::Specific(Api::OpenGl, (4, 2)))
        .with_vsync(false);
    let display = glium::Display::new(window_builder, context_builder, &event_loop)
        .expect("unable to create OpenGL window");

    let io = DirectoryIo::new(game_path.expect("--data-path is required"));

    let mut gfx = GlGfx::new(display, &event_loop);
    let gfx_handle = gfx.handle();

    let input = WinitInput::new();
    let input_handle = input.handle();
    let turbo_handle = input.handle();

    let mut executor = Executor::new(io, gfx_handle, input_handle, BYPASS_COPY_PROTECTION);
    let mut last_timestamp = std::time::Instant::now();

    std::thread::spawn(move || loop {
        let input = turbo_handle;
        loop {
            let input = input.get_input();
            let sleep_ms = executor.run();
            if sleep_ms > 0 {
                let ms = if input.turbo {
                    sleep_ms.min(1)
                } else {
                    sleep_ms
                };
                let elapsed = last_timestamp.elapsed();
                let duration = std::time::Duration::from_millis(ms);
                if duration > elapsed {
                    std::thread::sleep(duration - elapsed);
                } else if !input.turbo {
                    eprintln!(
                        "slow frame: {}ms {}ms",
                        elapsed.as_millis(),
                        duration.as_millis()
                    )
                }
                last_timestamp = std::time::Instant::now();
            }
        }
    });

    event_loop.run(move |event, _window, control_flow| match event {
        Event::UserEvent(UserEvent::Blit(page)) => {
            gfx.blit(page);
            gfx.request_redraw();
        }
        Event::UserEvent(UserEvent::Fill(page, color)) => {
            gfx.fill(page, color);
        }
        Event::UserEvent(UserEvent::Copy(src, dest, scroll)) => {
            gfx.copy(src, dest, scroll);
        }
        Event::UserEvent(UserEvent::Select(page)) => {
            gfx.select(page);
        }
        Event::UserEvent(UserEvent::String(text, color, x, y)) => {
            gfx.string(text, color, x, y);
        }
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => *control_flow = ControlFlow::Exit,
        Event::WindowEvent {
            event: WindowEvent::KeyboardInput { input: event, .. },
            ..
        } => {
            input.process_event(event);
        }
        _ => (),
    });
}
