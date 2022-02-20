#[derive(Debug, Copy, Clone)]
pub struct InputState {
    pub up: bool,
    pub left: bool,
    pub right: bool,
    pub down: bool,
    pub action: bool,
    pub turbo: bool,
}

pub trait Input {
    fn get_input(&self) -> InputState;
}
