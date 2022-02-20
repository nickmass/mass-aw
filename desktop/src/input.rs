use winit::event::{ElementState, VirtualKeyCode};

use std::sync::{Arc, Mutex};

use engine::input::{Input, InputState};

pub struct WinitInput {
    state: Arc<Mutex<InputState>>,
}

impl WinitInput {
    pub fn new() -> Self {
        WinitInput {
            state: Arc::new(Mutex::new(InputState {
                up: false,
                left: false,
                right: false,
                down: false,
                action: false,
                turbo: false,
            })),
        }
    }

    pub fn handle(&self) -> WinitInputHandle {
        WinitInputHandle {
            state: self.state.clone(),
        }
    }

    pub fn process_event(&self, event: winit::event::KeyboardInput) {
        if let Some(key) = event.virtual_keycode {
            let mut state = self.state.lock().unwrap();
            let pressed = event.state == ElementState::Pressed;
            match key {
                VirtualKeyCode::Up | VirtualKeyCode::W => state.up = pressed,
                VirtualKeyCode::Down | VirtualKeyCode::S => state.down = pressed,
                VirtualKeyCode::Left | VirtualKeyCode::A => state.left = pressed,
                VirtualKeyCode::Right | VirtualKeyCode::D => state.right = pressed,
                VirtualKeyCode::Space | VirtualKeyCode::Return => state.action = pressed,
                VirtualKeyCode::LShift | VirtualKeyCode::RShift => state.turbo = pressed,
                _ => (),
            }
        }
    }
}

pub struct WinitInputHandle {
    state: Arc<Mutex<InputState>>,
}

impl Input for WinitInputHandle {
    fn get_input(&self) -> InputState {
        let input = self.state.lock().unwrap();
        *input
    }
}
