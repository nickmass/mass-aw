use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, UrlSearchParams, Window};

use engine::Executor;

mod gfx;
mod gl;
mod input;
mod resources;
mod shaders;

use gfx::WebGlGfx;
use input::WebInput;
use resources::EmbeddedResources;

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

static mut RUNNER: Option<Runner> = None;

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    ConsoleLogger::initialize();

    unsafe {
        RUNNER = Some(Runner::new());
        RUNNER.as_ref().unwrap().schedule(0);
    };
}

struct Runner {
    closure: Closure<dyn Fn()>,
    executor: Executor<EmbeddedResources, WebGlGfx, WebInput>,
    window: Window,
    time_remainder: f64,
}

impl Runner {
    fn new() -> Self {
        let window = window().unwrap();
        let url_params = window.location().search().unwrap();
        let params = UrlSearchParams::new_with_str(url_params.as_str());
        let scale = params
            .unwrap()
            .get("scale")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);

        let io = EmbeddedResources;
        let gfx = WebGlGfx::new(320 * scale, 200 * scale);
        let input = WebInput::new();

        let executor = Executor::new(io, gfx, input, true);

        Self {
            executor,
            closure: Closure::wrap(Box::new(run) as Box<dyn Fn()>),
            window,
            time_remainder: 0.0,
        }
    }

    fn schedule(&self, sleep_ms: i32) {
        let _ = self
            .window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                self.closure.as_ref().unchecked_ref(),
                sleep_ms as i32,
            );
    }

    fn run(&mut self) {
        let now = self.window.performance().unwrap().now();
        let sleep_ms = self.executor.run() as f64;
        let next = self.window.performance().unwrap().now();
        let sleep_ms = sleep_ms - (next - now) + self.time_remainder;
        if sleep_ms > 0.0 {
            self.time_remainder += sleep_ms.fract();
            self.schedule(sleep_ms.floor() as i32);
        } else {
            self.time_remainder = 0.0;
            self.schedule(0);
        }
    }
}

fn run() {
    let runner = unsafe { RUNNER.as_mut().expect("runner init") };
    runner.run()
}

struct ConsoleLogger;

impl ConsoleLogger {
    pub fn initialize() {
        let _ = log::set_logger(&ConsoleLogger).unwrap();
        log::set_max_level(log::LevelFilter::max());
    }
}

impl log::Log for ConsoleLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        let level = metadata.level();
        cfg!(debug_assertions)
            || (level == log::Level::Error)
            || (level == log::Level::Warn)
            || (level == log::Level::Info)
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let level = record.level();
        let msg = JsValue::from_str(&format!("{}", record.args()));
        match level {
            log::Level::Error => web_sys::console::error_1(&msg),
            log::Level::Warn => web_sys::console::warn_1(&msg),
            log::Level::Info => web_sys::console::info_1(&msg),
            log::Level::Debug | log::Level::Trace => web_sys::console::debug_1(&msg),
        }
    }

    fn flush(&self) {}
}
