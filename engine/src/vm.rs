use crate::input::InputState;
use crate::resources::{PolygonResource, PolygonSource};
use crate::video::{
    BlitCommand, CopyVideoPageCommand, DrawCommand, DrawStringCommand, FillVideoPageCommand,
    PaletteCommand, SelectVideoPageCommand, VideoCommand,
};

pub struct ProgramCounter<'a> {
    pub mem: &'a [u8],
    pub address: usize,
}

impl<'a> ProgramCounter<'a> {
    pub fn read_u8(&mut self) -> u8 {
        let val = self
            .mem
            .get(self.address)
            .expect("read mem outside of range");
        self.address += 1;

        *val
    }
    pub fn read_i16(&mut self) -> i16 {
        self.read_u16() as i16
    }
    pub fn read_u16(&mut self) -> u16 {
        let high = self.read_u8() as u16;
        let low = self.read_u8() as u16;

        (high << 8) | low
    }
}

#[derive(Debug)]
pub struct Vm {
    variables: [i16; 256],
    thread_data: [ThreadData; 64],
    current_thread: u8,
    stack: [u16; 256],
    stack_ptr: usize,
    resume_pending: bool,
    video_commands: Vec<VideoCommand>,
    bypass: bool,
}

impl Vm {
    pub fn new(bypass: bool) -> Self {
        let mut vm = Vm {
            variables: [0; 256],
            thread_data: [ThreadData::default(); 64],
            current_thread: 0,
            stack: [0; 256],
            stack_ptr: 0,
            resume_pending: false,
            video_commands: Vec::new(),
            bypass,
        };

        vm.set_var(0x54, 0x81);
        vm.set_var(vars::RANDOM_SEED, 0x1234);

        if vm.bypass {
            vm.set_var(0xbc, 0x10);
            vm.set_var(0xc6, 0x80);
            vm.set_var(0xf2, 4000);
            vm.set_var(0xdc, 33);
        }

        vm.init_part();

        vm
    }

    pub fn video_commands(&mut self) -> impl Iterator<Item = VideoCommand> + '_ {
        self.video_commands.drain(..)
    }

    pub fn init_part(&mut self) {
        self.set_var(0xe4, 0x14);
        for thread in 0..64 {
            let thread = self.thread(thread);
            thread.pc = 0xffff;
            thread.requested_pc = 0xffff;
            thread.paused = false;
            thread.requested_pause = false;
        }

        self.current_thread = 0;
        self.current_thread().pc = 0x0000;
        self.resume_pending = false;
    }

    fn decode<'a>(&mut self, pc: &mut ProgramCounter<'a>) -> Instruction {
        //print!("{}:{:04X}\t", self.current_thread, pc.address);
        let op = pc.read_u8();
        //print!("{:02X}\t", op);
        match op {
            0x00 => Instruction::MovConst(pc.read_u8(), pc.read_i16()),
            0x01 => Instruction::Mov(pc.read_u8(), pc.read_u8()),
            0x02 => Instruction::Add(pc.read_u8(), pc.read_u8()),
            0x03 => Instruction::AddConst(pc.read_u8(), pc.read_i16()),
            0x04 => Instruction::Call(pc.read_u16()),
            0x05 => Instruction::Ret,
            0x06 => Instruction::TPause,
            0x07 => Instruction::Jmp(pc.read_u16()),
            0x08 => Instruction::SetVec(pc.read_u8(), pc.read_u16()),
            0x09 => Instruction::Jnz(pc.read_u8(), pc.read_u16()),
            0x0a => {
                let op = pc.read_u8();
                let variable = pc.read_u8();

                let operand = match op & 0xc0 {
                    0x80 | 0xc0 => VarOrConst::Variable(pc.read_u8()),
                    0x40 => VarOrConst::Const(pc.read_i16()),
                    0x00 => VarOrConst::Const(pc.read_u8() as i16),
                    _ => unreachable!("invalid match arm"),
                };

                let condition = match op & 0x7 {
                    0 => JmpCondition::Eq,
                    1 => JmpCondition::NotEq,
                    2 => JmpCondition::Greater,
                    3 => JmpCondition::GreaterEq,
                    4 => JmpCondition::Less,
                    5 => JmpCondition::LessEq,
                    _ => panic!("invalid jmp condition: {}", op & 0x8),
                };

                let destination = pc.read_u16();

                Instruction::CondJmp(condition, variable, operand, destination)
            }
            0x0b => Instruction::SetPalette(pc.read_u16()),
            0x0c => Instruction::TReset(pc.read_u8(), pc.read_u8(), pc.read_u8()),
            0x0d => Instruction::SelectVideoPage(pc.read_u8()),
            0x0e => Instruction::FillVideoPage(pc.read_u8(), pc.read_u8()),
            0x0f => Instruction::CopyVideoPage(pc.read_u8(), pc.read_u8()),
            0x10 => Instruction::Blit(pc.read_u8()),
            0x11 => Instruction::TKill,
            0x12 => {
                Instruction::DrawString(pc.read_u16(), pc.read_u8(), pc.read_u8(), pc.read_u8())
            }
            0x13 => Instruction::Sub(pc.read_u8(), pc.read_u8()),
            0x14 => Instruction::And(pc.read_u8(), pc.read_u16()),
            0x15 => Instruction::Or(pc.read_u8(), pc.read_u16()),
            0x16 => Instruction::Shl(pc.read_u8(), pc.read_u16()),
            0x17 => Instruction::Shr(pc.read_u8(), pc.read_u16()),
            0x18 => Instruction::PlaySound(pc.read_u16(), pc.read_u8(), pc.read_u8(), pc.read_u8()),
            0x19 => Instruction::LoadRes(pc.read_u16()),
            0x1a => Instruction::PlayMusic(pc.read_u16(), pc.read_u16(), pc.read_u8()),
            op if op & 0x80 != 0 => {
                let offset = ((op as u16) << 8) | pc.read_u8() as u16;

                let mut x = pc.read_u8() as i16;
                let mut y = pc.read_u8() as i16;

                let h = y - 199;

                if h > 0 {
                    y = 199;
                    x += h;
                }

                let polygon = PolygonResource {
                    buffer_offset: (offset.wrapping_mul(2)) as usize,
                    source: PolygonSource::Cinematic,
                };

                Instruction::Draw(
                    polygon,
                    VarOrConst::Const(x),
                    VarOrConst::Const(y),
                    VarOrConst::Const(0x40),
                )
            }
            op if op & 0x40 != 0 => {
                let offset = pc.read_u16();
                let x = match op & 0x30 {
                    0x00 => VarOrConst::Const(pc.read_i16()),
                    0x10 => VarOrConst::Variable(pc.read_u8()),
                    0x20 => VarOrConst::Const(pc.read_u8() as i16),
                    0x30 => VarOrConst::Const(pc.read_u8() as i16 + 0x100),
                    _ => unreachable!("invalid match arm"),
                };

                let y = match op & 0x0c {
                    0x00 => VarOrConst::Const(pc.read_i16()),
                    0x04 => VarOrConst::Variable(pc.read_u8()),
                    0x08 | 0x0c => VarOrConst::Const(pc.read_u8() as i16),
                    _ => unreachable!("invalid match arm"),
                };

                let zoom = match op & 0x03 {
                    0x00 => VarOrConst::Const(0x40),
                    0x01 => VarOrConst::Variable(pc.read_u8()),
                    0x02 => VarOrConst::Const(pc.read_u8() as i16),
                    0x03 => VarOrConst::Const(0x40),
                    _ => unreachable!("invalid match arm"),
                };

                let source = if op & 0x03 == 0x03 {
                    PolygonSource::AltVideo
                } else {
                    PolygonSource::Cinematic
                };

                let polygon = PolygonResource {
                    buffer_offset: (offset.wrapping_mul(2)) as usize,
                    source,
                };

                Instruction::Draw(polygon, x, y, zoom)
            }
            _ => panic!("invalid opcode"),
        }
    }

    fn get_var(&self, variable_id: u8) -> i16 {
        if variable_id == vars::MUSIC_MARKER {
            eprintln!("unimplemented: read music marker");
        }
        self.variables[variable_id as usize]
    }

    fn set_var(&mut self, variable_id: u8, value: i16) {
        self.variables[variable_id as usize] = value
    }

    fn current_thread(&mut self) -> &mut ThreadData {
        self.thread(self.current_thread)
    }

    fn thread(&mut self, thread_id: u8) -> &mut ThreadData {
        &mut self.thread_data[thread_id as usize]
    }

    fn execute(&mut self, instruction: Instruction) -> InstructionResult {
        //println!("{:?}", instruction);
        match instruction {
            Instruction::MovConst(dest, value) => self.set_var(dest, value),
            Instruction::Mov(dest, src) => self.set_var(dest, self.get_var(src)),
            Instruction::Add(dest, src) => {
                let res = self.get_var(dest).wrapping_add(self.get_var(src));
                self.set_var(dest, res);
            }
            Instruction::AddConst(dest, value) => {
                let res = self.get_var(dest).wrapping_add(value);
                self.set_var(dest, res);
            }
            Instruction::Call(dest) => {
                if self.stack_ptr == 0xff {
                    panic!("stack overflow");
                }

                self.stack[self.stack_ptr] = self.current_thread().pc;
                self.stack_ptr += 1;

                self.current_thread().pc = dest;
            }
            Instruction::Ret => {
                if self.stack_ptr == 0 {
                    panic!("stack underflow");
                }

                self.stack_ptr -= 1;
                self.current_thread().pc = self.stack[self.stack_ptr];
            }
            Instruction::TPause => {
                return InstructionResult::NextThread;
            }
            Instruction::Jmp(dest) => {
                self.current_thread().pc = dest;
            }
            Instruction::SetVec(thread_id, pc) => {
                self.thread(thread_id).requested_pc = pc;
            }
            Instruction::Jnz(var, dest) => {
                let res = self.get_var(var).wrapping_sub(1);
                self.set_var(var, res);

                if res != 0 {
                    self.current_thread().pc = dest;
                }
            }
            Instruction::CondJmp(condition, variable, operand, dest) => {
                let left = self.get_var(variable);
                let right = match operand {
                    VarOrConst::Variable(var) => self.get_var(var),
                    VarOrConst::Const(val) => val,
                };

                let take_jump = match condition {
                    JmpCondition::Eq => left == right,
                    JmpCondition::NotEq => left != right,
                    JmpCondition::Greater => left > right,
                    JmpCondition::GreaterEq => left >= right,
                    JmpCondition::Less => left < right,
                    JmpCondition::LessEq => left <= right,
                };

                if take_jump {
                    self.current_thread().pc = dest;
                }
            }
            Instruction::SetPalette(palette_id) => {
                self.video_commands
                    .push(VideoCommand::Palette(PaletteCommand {
                        palette_id: (palette_id >> 8) as u8,
                    }))
            }
            Instruction::TReset(thread_start, mut thread_end, mode) => {
                if thread_end >= 64 {
                    thread_end &= 63;
                }

                if thread_end < thread_start {
                    panic!(
                        "invalid thread reset range: {} {} {}",
                        thread_start, thread_end, mode
                    );
                }

                if mode == 2 {
                    for thread in thread_start..=thread_end {
                        self.thread(thread).requested_pc = 0xfffe;
                    }
                } else if mode < 2 {
                    for thread in thread_start..=thread_end {
                        self.thread(thread).requested_pause = mode == 1;
                    }
                }
            }
            Instruction::SelectVideoPage(page_id) => {
                self.video_commands
                    .push(VideoCommand::SelectVideoPage(SelectVideoPageCommand {
                        page_id,
                    }))
            }
            Instruction::FillVideoPage(page_id, color) => {
                self.video_commands
                    .push(VideoCommand::FillVideoPage(FillVideoPageCommand {
                        page_id,
                        color,
                    }))
            }
            Instruction::CopyVideoPage(src_page_id, dest_page_id) => {
                let scroll = self.get_var(vars::SCROLL_Y);
                self.video_commands
                    .push(VideoCommand::CopyVideoPage(CopyVideoPageCommand {
                        src_page_id,
                        dest_page_id,
                        scroll,
                    }))
            }
            Instruction::Blit(page_id) => {
                self.set_var(0xf7, 0);
                let duration = self.get_var(vars::SLEEP_TICKS) as u64 * 20;
                self.video_commands
                    .push(VideoCommand::Blit(BlitCommand { page_id }));
                return InstructionResult::Yield(Yield::Blit(duration));
            }
            Instruction::TKill => {
                self.current_thread().pc = 0xffff;
                return InstructionResult::NextThread;
            }
            Instruction::DrawString(string_id, x, y, color) => {
                self.video_commands
                    .push(VideoCommand::DrawString(DrawStringCommand {
                        string_id,
                        x,
                        y,
                        color,
                    }));
            }
            Instruction::Sub(dest, src) => {
                let res = self.get_var(dest).wrapping_sub(self.get_var(src));
                self.set_var(dest, res);
            }
            Instruction::And(dest, value) => {
                let res = (self.get_var(dest) as u16) & value;
                self.set_var(dest, res as i16);
            }
            Instruction::Or(dest, value) => {
                let res = (self.get_var(dest) as u16) | value;
                self.set_var(dest, res as i16);
            }
            Instruction::Shl(dest, value) => {
                let res = (self.get_var(dest) as u16) << value;
                self.set_var(dest, res as i16);
            }
            Instruction::Shr(dest, value) => {
                let res = (self.get_var(dest) as u16) >> value;
                self.set_var(dest, res as i16);
            }
            Instruction::PlaySound(_res_id, _freq, _vol, _channel) => (),
            Instruction::LoadRes(res_id) => {
                return InstructionResult::Yield(Yield::ReqResource(res_id))
            }
            Instruction::PlayMusic(_res_id, _delay, _pos) => (),
            Instruction::Draw(polygon, x, y, zoom) => {
                let x = match x {
                    VarOrConst::Variable(v) => self.get_var(v),
                    VarOrConst::Const(n) => n,
                };
                let y = match y {
                    VarOrConst::Variable(v) => self.get_var(v),
                    VarOrConst::Const(n) => n,
                };
                let zoom = match zoom {
                    VarOrConst::Variable(v) => self.get_var(v),
                    VarOrConst::Const(n) => n,
                };

                self.video_commands.push(VideoCommand::Draw(DrawCommand {
                    polygon,
                    x,
                    y,
                    zoom,
                }));
            }
        }

        InstructionResult::Continue
    }

    fn execute_thread(&mut self, mem: &[u8]) -> ThreadResult {
        loop {
            let mut pc = ProgramCounter {
                mem,
                address: self.current_thread().pc as usize,
            };
            let instruction = self.decode(&mut pc);
            self.current_thread().pc = pc.address as u16;

            match self.execute(instruction) {
                InstructionResult::Yield(y) => break ThreadResult::Yield(y),
                InstructionResult::NextThread => break ThreadResult::Continue,
                InstructionResult::Continue => continue,
            }
        }
    }

    pub fn execute_frame(&mut self, mem: &[u8], input: InputState) -> FrameResult {
        if !self.resume_pending {
            self.update_threads();
            self.current_thread = 0;
        }
        self.resume_frame(mem, input)
    }

    fn update_input(&mut self, input: InputState) {
        let mut left_right = 0;
        let mut up_down = 0;
        let mut input_mask = 0;

        if input.right {
            left_right = 1;
            input_mask |= 1;
        }
        if input.left {
            left_right = -1;
            input_mask |= 2;
        }
        if input.down {
            up_down = 1;
            input_mask |= 4;
        }

        self.set_var(vars::HERO_POS_UP_DOWN, up_down);

        if input.up {
            up_down = -1;
            input_mask |= 8;
            self.set_var(vars::HERO_POS_UP_DOWN, -1);
        }

        self.set_var(vars::HERO_POS_MASK, input_mask);

        if input.action {
            input_mask |= 0x80;
            self.set_var(vars::HERO_ACTION, 1);
        }

        self.set_var(vars::HERO_POS_JUMP_DOWN, up_down);
        self.set_var(vars::HERO_POS_LEFT_RIGHT, left_right);
        self.set_var(vars::HERO_ACTION_POS_MASK, input_mask);
    }

    fn resume_frame(&mut self, mem: &[u8], input: InputState) -> FrameResult {
        self.update_input(input);

        for thread in self.current_thread..64 {
            self.current_thread = thread;
            let thread_data = self.current_thread();

            if thread_data.paused {
                continue;
            }

            if thread_data.pc != 0xffff {
                if !self.resume_pending {
                    self.stack_ptr = 0;
                } else {
                    self.resume_pending = false;
                }

                if let ThreadResult::Yield(y) = self.execute_thread(mem) {
                    self.resume_pending = true;
                    return FrameResult::Yield(y);
                }
            }
        }

        FrameResult::Complete
    }

    fn update_threads(&mut self) {
        for thread in 0..64 {
            let thread_data = self.thread(thread);
            thread_data.paused = thread_data.requested_pause;

            if thread_data.requested_pc != 0xffff {
                let requested_pc = if thread_data.requested_pc == 0xfffe {
                    0xffff
                } else {
                    thread_data.requested_pc
                };

                thread_data.pc = requested_pc;
                thread_data.requested_pc = 0xffff;
            }
        }
    }
}

#[derive(Debug, Default, Copy, Clone)]
struct ThreadData {
    pub pc: u16,
    pub requested_pc: u16,
    pub paused: bool,
    pub requested_pause: bool,
}

#[derive(Debug, Copy, Clone)]
enum Instruction {
    MovConst(u8, i16),
    Mov(u8, u8),
    Add(u8, u8),
    AddConst(u8, i16),
    Call(u16),
    Ret,
    TPause,
    Jmp(u16),
    SetVec(u8, u16),
    Jnz(u8, u16),
    CondJmp(JmpCondition, u8, VarOrConst, u16),
    SetPalette(u16),
    TReset(u8, u8, u8),
    SelectVideoPage(u8),
    FillVideoPage(u8, u8),
    CopyVideoPage(u8, u8),
    Blit(u8),
    TKill,
    DrawString(u16, u8, u8, u8),
    Sub(u8, u8),
    And(u8, u16),
    Or(u8, u16),
    Shl(u8, u16),
    Shr(u8, u16),
    PlaySound(u16, u8, u8, u8),
    LoadRes(u16),
    PlayMusic(u16, u16, u8),
    Draw(PolygonResource, VarOrConst, VarOrConst, VarOrConst),
}

#[derive(Debug, Copy, Clone)]
enum JmpCondition {
    Eq,
    NotEq,
    Greater,
    GreaterEq,
    Less,
    LessEq,
}

#[derive(Debug, Copy, Clone)]
enum VarOrConst {
    Variable(u8),
    Const(i16),
}

#[derive(Debug, Copy, Clone)]
pub enum Yield {
    Blit(u64),
    ReqResource(u16),
}

#[derive(Debug, Copy, Clone)]
enum InstructionResult {
    Yield(Yield),
    Continue,
    NextThread,
}
#[derive(Debug, Copy, Clone)]
enum ThreadResult {
    Yield(Yield),
    Continue,
}

#[derive(Debug, Copy, Clone)]
pub enum FrameResult {
    Yield(Yield),
    Complete,
}

pub mod vars {
    pub const HERO_POS_UP_DOWN: u8 = 0xe5;
    pub const HERO_ACTION: u8 = 0xfa;
    pub const HERO_POS_JUMP_DOWN: u8 = 0xfb;
    pub const HERO_POS_LEFT_RIGHT: u8 = 0xfc;
    pub const HERO_POS_MASK: u8 = 0xfd;
    pub const HERO_ACTION_POS_MASK: u8 = 0xfe;
    pub const RANDOM_SEED: u8 = 0x3c;
    pub const MUSIC_MARKER: u8 = 0xf4;
    pub const SCROLL_Y: u8 = 0xf9;
    pub const SLEEP_TICKS: u8 = 0xff;
}
