use crate::gfx::Gfx;
use crate::input::Input;
use crate::resources::{GamePart, Io, Resources};
use crate::video::Video;
use crate::vm::{FrameResult, Vm, Yield};

pub struct Executor<I: Io, G: Gfx, In: Input> {
    vm: Vm,
    video: Video<G>,
    resources: Resources<I>,
    input: In,
    frame: u64,
}

impl<I: Io, G: Gfx, In: Input> Executor<I, G, In> {
    pub fn new(io: I, gfx: G, input: In, bypass: bool) -> Self {
        let video = Video::new(gfx);
        let vm = Vm::new(bypass);
        let mut resources = Resources::load(io).unwrap();

        if bypass {
            resources.prepare_part(GamePart::Two);
        } else {
            resources.prepare_part(GamePart::One);
        }

        Self {
            vm,
            video,
            resources,
            input,
            frame: 0,
        }
    }

    pub fn run(&mut self) -> u64 {
        loop {
            let input = self.input.get_input();
            let res = self
                .vm
                .execute_frame(self.resources.bytecode().expect("bytecode loaded"), input);

            match res {
                FrameResult::Yield(Yield::Blit(ms)) => {
                    for cmd in self.vm.video_commands() {
                        self.video.push_command(cmd, &self.resources);
                    }

                    if ms > 0 {
                        return ms;
                    }
                }
                FrameResult::Yield(Yield::ReqResource(resource_id)) => {
                    self.resources.load_part_or_entry(resource_id)
                }
                FrameResult::Complete => {
                    self.frame += 1;
                    if let Some(part) = self.resources.requested_part() {
                        self.resources.prepare_part(part);
                        self.vm.init_part();
                    }
                }
            }
        }
    }
}
