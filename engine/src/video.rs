use crate::gfx::Gfx;
use crate::resources::{Io, PolygonResource, PolygonSource, Resources};
use crate::vm::ProgramCounter;

#[derive(Debug, Copy, Clone)]
pub struct DrawCommand {
    pub polygon: PolygonResource,
    pub x: i16,
    pub y: i16,
    pub zoom: i16,
}

#[derive(Debug, Copy, Clone)]
pub struct PaletteCommand {
    pub palette_id: u8,
}

#[derive(Debug, Copy, Clone)]
pub struct SelectVideoPageCommand {
    pub page_id: u8,
}

#[derive(Debug, Copy, Clone)]
pub struct FillVideoPageCommand {
    pub page_id: u8,
    pub color: u8,
}

#[derive(Debug, Copy, Clone)]
pub struct CopyVideoPageCommand {
    pub src_page_id: u8,
    pub dest_page_id: u8,
    pub scroll: i16,
}

#[derive(Debug, Copy, Clone)]
pub struct DrawStringCommand {
    pub string_id: u16,
    pub x: u8,
    pub y: u8,
    pub color: u8,
}

#[derive(Debug, Copy, Clone)]
pub struct BlitCommand {
    pub page_id: u8,
}

#[derive(Debug, Copy, Clone)]
pub enum VideoCommand {
    Draw(DrawCommand),
    Palette(PaletteCommand),
    SelectVideoPage(SelectVideoPageCommand),
    FillVideoPage(FillVideoPageCommand),
    CopyVideoPage(CopyVideoPageCommand),
    DrawString(DrawStringCommand),
    Blit(BlitCommand),
}

pub struct Video<T: Gfx> {
    gfx: T,
    requested_palette: Option<[(u8, u8, u8); 16]>,
    current_page: Page,
    working_page_a: Page,
    working_page_b: Page,
}

impl<T: Gfx> Video<T> {
    pub fn new(gfx: T) -> Self {
        Self {
            gfx,
            requested_palette: None,
            current_page: Page::One,
            working_page_a: Page::One,
            working_page_b: Page::Two,
        }
    }

    pub fn push_command<I: Io>(&mut self, command: VideoCommand, resources: &Resources<I>) {
        match command {
            VideoCommand::Draw(draw) => self.draw(draw, resources),
            VideoCommand::Palette(pal) => {
                let offset = (pal.palette_id) as usize * 32;
                let palette = &resources.palette().expect("palette not loaded")[offset..];
                let mut new_palette = [(0, 0, 0); 16];
                for n in 0..16 {
                    let c0 = palette[n * 2];
                    let c1 = palette[n * 2 + 1];

                    let r = (((c0 & 0x0f) << 2) | ((c0 & 0x0f) >> 2)) << 2;
                    let g = (((c1 & 0xf0) >> 2) | ((c1 & 0xf0) >> 6)) << 2;
                    let b = (((c1 & 0x0f) >> 2) | ((c1 & 0x0f) << 2)) << 2;

                    new_palette[n] = (r, g, b);
                }
                self.requested_palette = Some(new_palette);
            }
            VideoCommand::FillVideoPage(fill) => {
                let page = self.get_page(fill.page_id);
                self.gfx.fill_page(page, fill.color);
            }
            VideoCommand::SelectVideoPage(select) => {
                self.current_page = self.get_page(select.page_id);
                self.gfx.select_page(self.current_page);
            }
            VideoCommand::CopyVideoPage(copy) => {
                if copy.src_page_id == copy.dest_page_id {
                    return;
                }

                let (src, dest, scroll) = if copy.src_page_id >= 0xfe {
                    let src = self.get_page(copy.src_page_id);
                    let dest = self.get_page(copy.dest_page_id);
                    (src, dest, 0)
                } else if copy.src_page_id & 0x80 == 0 {
                    let src = self.get_page(copy.src_page_id & 0xbf);
                    let dest = self.get_page(copy.dest_page_id);
                    (src, dest, 0)
                } else {
                    let src = self.get_page(copy.src_page_id & 0x3);
                    let dest = self.get_page(copy.dest_page_id);
                    (src, dest, copy.scroll)
                };

                self.gfx.copy_page(src, dest, scroll)
            }
            VideoCommand::DrawString(string) => {
                for (id, msg) in crate::strings::STRING_TABLE.iter() {
                    if *id == string.string_id {
                        self.gfx.draw_string(
                            msg,
                            string.color,
                            ((string.x as i16) - 1) * 8,
                            string.y as i16,
                        );
                        break;
                    }
                }
            }
            VideoCommand::Blit(blit) => {
                match blit.page_id {
                    0xff => {
                        let temp = self.working_page_a;
                        self.working_page_a = self.working_page_b;
                        self.working_page_b = temp;
                    }
                    0xfe => (),
                    _ => {
                        self.working_page_a = self.get_page(blit.page_id);
                    }
                }

                if let Some(palette) = self.requested_palette.take() {
                    self.gfx.set_palette(palette)
                }

                self.gfx.blit(self.working_page_a);
            }
        }
    }

    fn get_page(&self, page_id: u8) -> Page {
        match page_id {
            0 => Page::Zero,
            1 => Page::One,
            2 => Page::Two,
            3 => Page::Three,
            0xff => self.working_page_b,
            0xfe => self.working_page_a,
            _ => Page::Zero,
        }
    }

    fn draw<I: Io>(&mut self, command: DrawCommand, resources: &Resources<I>) {
        let color = 0xff;

        let buffer = match command.polygon.source {
            PolygonSource::Cinematic => resources.cinematic().expect("cinematic not loaded"),
            PolygonSource::AltVideo => resources.alt_video().expect("alt video not loaded"),
        };

        self.do_draw(
            color,
            command.x,
            command.y,
            command.zoom / 64,
            command.polygon.buffer_offset,
            buffer,
        )
    }

    fn do_draw(&mut self, color: u8, x: i16, y: i16, zoom: i16, offset: usize, buffer: &'_ [u8]) {
        let mut pc = ProgramCounter {
            mem: buffer,
            address: offset,
        };

        let mode = pc.read_u8();

        if mode >= 0xc0 {
            let x_bound = pc.read_u8() as i16 * zoom;
            let y_bound = pc.read_u8() as i16 * zoom;
            let num_points = pc.read_u8() as usize;

            let x_min = x - x_bound / 2;
            let x_max = x + x_bound / 2;
            let y_min = y - y_bound / 2;
            let y_max = y + y_bound / 2;

            if x_min > 319 || x_max < 0 || y_min > 199 || y_max < 0 {
                return;
            }

            let color = if color & 0x80 != 0 {
                mode & 0x3f
            } else {
                color
            };

            let blend = if color < 0x10 {
                BlendMode::Solid(color)
            } else if color > 0x10 {
                BlendMode::Blend
            } else {
                BlendMode::Mask(0x8)
            };

            let mut poly = Polygon {
                num_points,
                blend,
                points: [(0, 0); 50],
            };

            if x_bound == 0 && y_bound == 1 && num_points == 4 {
                poly.points[0] = (x, y);
                poly.points[1] = (x - 1, y);
                poly.points[2] = (x - 1, y + 1);
                poly.points[3] = (x, y + 1);
            } else {
                for n in 0..num_points {
                    let x = pc.read_u8() as i16 * zoom;
                    let y = pc.read_u8() as i16 * zoom;

                    // Hack for zero width vertical lines
                    let x_off = if x_bound == 0 && num_points == 4 && n >= 2 {
                        1
                    } else {
                        0
                    };

                    poly.points[n] = (x + x_min - x_off, y + y_min);
                }
            }

            self.gfx.draw_polygon(poly);
        } else if mode & 0x3f == 2 {
            let x = x - pc.read_u8() as i16 * zoom;
            let y = y - pc.read_u8() as i16 * zoom;

            let num_children = pc.read_u8();

            for _ in 0..=num_children {
                let offset = pc.read_u16();

                let child_x = x + pc.read_u8() as i16 * zoom;
                let child_y = y + pc.read_u8() as i16 * zoom;

                let color = if offset & 0x8000 != 0 {
                    let color = pc.read_u8();
                    let _ = pc.read_u8();
                    color
                } else {
                    0xff
                };

                let offset = (offset & 0x7fff) * 2;

                self.do_draw(color, child_x, child_y, zoom, offset as usize, buffer)
            }
        } else {
            panic!("unexpected polygon mode")
        }
    }
}

#[derive(Debug, Clone)]
pub struct Polygon {
    points: [(i16, i16); 50],
    num_points: usize,
    pub blend: BlendMode,
}

impl Polygon {
    pub fn points(&self) -> impl Iterator<Item = (i16, i16)> + '_ {
        self.points[0..self.num_points]
            .iter()
            .map(|(x, y)| (*x, *y))
    }
}

#[derive(Debug, Copy, Clone)]
pub enum BlendMode {
    Solid(u8),
    Mask(u8),
    Blend,
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum Page {
    Zero,
    One,
    Two,
    Three,
}
