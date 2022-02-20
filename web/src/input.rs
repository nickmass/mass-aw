use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{window, KeyboardEvent};

use engine::input::InputState;
use engine::Input;

static mut INPUT_STATE: InputState = InputState {
    up: false,
    down: false,
    left: false,
    right: false,
    action: false,
    turbo: false,
};

#[allow(dead_code)]
pub struct WebInput {
    key_down: Closure<dyn Fn(JsValue)>,
    key_up: Closure<dyn Fn(JsValue)>,
}
impl WebInput {
    pub fn new() -> Self {
        let window = window().unwrap();
        let document = window.document().unwrap();

        let key_down = Closure::wrap(Box::new(key_down) as Box<dyn Fn(JsValue)>);
        let key_up = Closure::wrap(Box::new(key_up) as Box<dyn Fn(JsValue)>);

        let _ =
            document.add_event_listener_with_callback("keydown", key_down.as_ref().unchecked_ref());
        let _ = document.add_event_listener_with_callback("keyup", key_up.as_ref().unchecked_ref());

        Self { key_down, key_up }
    }
}

fn key_down(event: JsValue) {
    let event: KeyboardEvent = event.dyn_into().unwrap();
    let mut state = unsafe { INPUT_STATE };
    match event.code().as_str() {
        "ArrowUp" | "KeyW" => state.up = true,
        "ArrowDown" | "KeyS" => state.down = true,
        "ArrowLeft" | "KeyA" => state.left = true,
        "ArrowRight" | "KeyD" => state.right = true,
        "Space" | "Enter" => state.action = true,
        _ => (),
    }

    unsafe { INPUT_STATE = state };
}

fn key_up(event: JsValue) {
    let event: KeyboardEvent = event.dyn_into().unwrap();
    let mut state = unsafe { INPUT_STATE };
    match event.code().as_str() {
        "ArrowUp" | "KeyW" => state.up = false,
        "ArrowDown" | "KeyS" => state.down = false,
        "ArrowLeft" | "KeyA" => state.left = false,
        "ArrowRight" | "KeyD" => state.right = false,
        "Space" | "Enter" => state.action = false,
        _ => (),
    }
    unsafe { INPUT_STATE = state };
}

impl Input for WebInput {
    fn get_input(&self) -> InputState {
        unsafe { INPUT_STATE }
    }
}
