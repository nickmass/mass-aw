use glium::{
    framebuffer::{DepthRenderBuffer, SimpleFrameBuffer},
    index::PrimitiveType,
    texture::{MipmapsOption, RawImage2d, UncompressedUintFormat, UnsignedTexture2d},
    uniforms::Sampler,
    DrawParameters, IndexBuffer, Rect, Surface, Texture2d, VertexBuffer,
};
use lyon::{
    lyon_tessellation::{BuffersBuilder, FillOptions, FillVertex, VertexBuffers},
    path::traits::PathBuilder,
    tessellation::FillTessellator,
};
use winit::event_loop::{EventLoop, EventLoopProxy};

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};

use crate::shaders::*;
use crate::video::{BlendMode, Page, Polygon};
use crate::UserEvent;

pub trait Gfx {
    fn blit(&mut self, page: Page);
    fn draw_polygon(&mut self, polygon: Polygon);
    fn fill_page(&mut self, page: Page, color: u8);
    fn select_page(&mut self, page: Page);
    fn copy_page(&mut self, src: Page, dest: Page, scroll: i16);
    fn set_palette(&mut self, palette: [(u8, u8, u8); 16]);
    fn draw_string(&mut self, text: &'static str, color: u8, x: i16, y: i16);
}

struct RenderPage {
    texture: UnsignedTexture2d,
    depth: DepthRenderBuffer,
}

impl RenderPage {
    fn new(display: &glium::Display) -> Self {
        let (width, height) = display.get_framebuffer_dimensions();

        let texture = UnsignedTexture2d::empty_with_format(
            display,
            UncompressedUintFormat::U8,
            MipmapsOption::NoMipmap,
            width,
            height,
        )
        .unwrap();

        let depth =
            DepthRenderBuffer::new(display, glium::texture::DepthFormat::I16, width, height)
                .unwrap();

        Self { texture, depth }
    }

    fn frame(&self, display: &glium::Display) -> SimpleFrameBuffer<'_> {
        SimpleFrameBuffer::with_depth_buffer(display, &self.texture, &self.depth).unwrap()
    }

    fn sampled(&self) -> Sampler<UnsignedTexture2d> {
        self.texture
            .sampled()
            .minify_filter(glium::uniforms::MinifySamplerFilter::Nearest)
            .magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest)
    }
}

struct RenderPalette {
    colors: [(u8, u8, u8); 16],
    texture: Texture2d,
}

impl RenderPalette {
    fn new(display: &glium::Display) -> Self {
        let colors = [(0, 0, 0); 16];

        let texture = Texture2d::new(
            display,
            RawImage2d {
                data: (colors.as_slice()).into(),
                width: 16,
                height: 1,
                format: glium::texture::ClientFormat::U8U8U8,
            },
        )
        .unwrap();

        Self { colors, texture }
    }

    fn update(&mut self, palette: &mut Option<[(u8, u8, u8); 16]>) {
        if let Some(data) = palette.take() {
            self.colors = data;
            self.texture.write(
                Rect {
                    left: 0,
                    bottom: 0,
                    width: 16,
                    height: 1,
                },
                RawImage2d {
                    data: (self.colors.as_slice()).into(),
                    width: 16,
                    height: 1,
                    format: glium::texture::ClientFormat::U8U8U8,
                },
            )
        }
    }

    fn sampled(&self) -> Sampler<Texture2d> {
        self.texture.sampled()
    }
}

const SCREEN_QUAD: [QuadPoint; 6] = [
    QuadPoint {
        position: (-1.0, -1.0),
    },
    QuadPoint {
        position: (1.0, -1.0),
    },
    QuadPoint {
        position: (-1.0, 1.0),
    },
    QuadPoint {
        position: (1.0, 1.0),
    },
    QuadPoint {
        position: (1.0, -1.0),
    },
    QuadPoint {
        position: (-1.0, 1.0),
    },
];

struct Sync {
    lock: Mutex<bool>,
    condvar: Condvar,
}

impl Sync {
    fn new() -> Self {
        Self {
            lock: Mutex::new(false),
            condvar: Condvar::new(),
        }
    }
    fn notify(&self) {
        let mut synced = self.lock.lock().unwrap();
        *synced = true;
        self.condvar.notify_one()
    }

    fn wait(&self) {
        let mut synced = self.lock.lock().unwrap();
        while !*synced {
            synced = self.condvar.wait(synced).unwrap();
        }
        *synced = false;
    }
}

struct GfxState {
    polygons: Vec<Polygon>,
    palette: Option<[(u8, u8, u8); 16]>,
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
enum GlPage {
    Game(Page),
    Current,
    Zero,
}

pub struct GlGfx {
    display: glium::Display,
    state: Arc<Mutex<GfxState>>,
    sync: Arc<Sync>,
    proxy: EventLoopProxy<UserEvent>,
    tessellator: FillTessellator,
    palette: RenderPalette,
    page_program: glium::Program,
    frame_program: glium::Program,
    copy_program: glium::Program,
    font_program: glium::Program,
    pages: HashMap<GlPage, RenderPage>,
    output_page: Page,
    active_page: Page,
    screen_vertex_buffer: VertexBuffer<QuadPoint>,
    tessellate_buffer: VertexBuffers<PolyPoint, u16>,
    font_texture: UnsignedTexture2d,
    text_buffer: Vec<TextPoint>,
}

fn create_program(
    display: &glium::Display,
    vertex: &str,
    fragment: &str,
    srgb: bool,
) -> glium::Program {
    let program_input = glium::program::ProgramCreationInput::SourceCode {
        vertex_shader: vertex,
        fragment_shader: fragment,
        outputs_srgb: srgb,
        tessellation_control_shader: None,
        tessellation_evaluation_shader: None,
        geometry_shader: None,
        transform_feedback_varyings: None,
        uses_point_size: false,
    };
    glium::Program::new(display, program_input).unwrap()
}

fn create_font(display: &glium::Display) -> UnsignedTexture2d {
    let mut font_data = vec![0u8; 100 * 64];
    for n in 0..96 {
        let x_ind = (n % 10) * 8;
        let y_ind = (n / 10) * 8;

        for y in 0..8 {
            let mut row = crate::font::FONT[(n * 8) + y];
            for x in 0..8 {
                let bit = row & 0x80 != 0;
                row <<= 1;
                let color = if bit { 0xff } else { 0x00 };

                let x_off = x_ind + x;
                let y_off = y_ind + y;

                font_data[(y_off * 80) + x_off] = color;
            }
        }
    }

    let data = RawImage2d {
        data: font_data.into(),
        width: 80,
        height: 80,
        format: glium::texture::ClientFormat::U8,
    };

    UnsignedTexture2d::with_format(
        display,
        data,
        UncompressedUintFormat::U8,
        MipmapsOption::NoMipmap,
    )
    .unwrap()
}

impl GlGfx {
    pub fn new(display: glium::Display, event_loop: &EventLoop<UserEvent>) -> Self {
        let proxy = event_loop.create_proxy();

        let page_program =
            create_program(&display, PAGE_VERTEX_SHADER, PAGE_FRAGMENT_SHADER, false);
        let frame_program =
            create_program(&display, FRAME_VERTEX_SHADER, FRAME_FRAGMENT_SHADER, true);
        let copy_program =
            create_program(&display, COPY_VERTEX_SHADER, COPY_FRAGMENT_SHADER, false);
        let font_program =
            create_program(&display, FONT_VERTEX_SHADER, FONT_FRAGMENT_SHADER, false);

        let mut pages = HashMap::new();
        let page = RenderPage::new(&display);
        pages.insert(GlPage::Game(Page::Zero), page);
        let page = RenderPage::new(&display);
        pages.insert(GlPage::Game(Page::One), page);
        let page = RenderPage::new(&display);
        pages.insert(GlPage::Game(Page::Two), page);
        let page = RenderPage::new(&display);
        pages.insert(GlPage::Game(Page::Three), page);
        let page = RenderPage::new(&display);
        pages.insert(GlPage::Zero, page);
        let page = RenderPage::new(&display);
        pages.insert(GlPage::Current, page);

        let palette = RenderPalette::new(&display);

        let screen_vertex_buffer = VertexBuffer::new(&display, SCREEN_QUAD.as_slice()).unwrap();
        let tessellate_buffer: VertexBuffers<PolyPoint, u16> = VertexBuffers::new();

        let font_texture = create_font(&display);

        Self {
            display,
            proxy,
            state: Arc::new(Mutex::new(GfxState {
                polygons: Vec::new(),
                palette: Some([(0, 0, 0); 16]),
            })),
            tessellator: FillTessellator::new(),
            palette,
            page_program,
            frame_program,
            copy_program,
            font_program,
            pages,
            output_page: Page::Zero,
            active_page: Page::Zero,
            screen_vertex_buffer,
            tessellate_buffer,
            sync: Arc::new(Sync::new()),
            font_texture,
            text_buffer: Vec::new(),
        }
    }

    pub fn request_redraw(&self) {
        self.display.gl_window().window().request_redraw()
    }

    pub fn handle(&self) -> GlHandle {
        GlHandle {
            state: self.state.clone(),
            proxy: self.proxy.clone(),
            sync: self.sync.clone(),
        }
    }

    pub fn fill(&mut self, page: Page, color: u8) {
        self.flush_draws();
        let color = color & 0xf;

        let dest_page = self.pages.get(&GlPage::Game(page)).unwrap();
        let mut frame = dest_page.frame(&self.display);
        frame.clear_depth(-1.0);

        let gpu_index_buffer = glium::index::NoIndices(PrimitiveType::TrianglesList);

        let uniforms = glium::uniform! {
            u_fill: color as u32
        };

        frame
            .draw(
                &self.screen_vertex_buffer,
                &gpu_index_buffer,
                &self.copy_program,
                &uniforms,
                &DrawParameters::default(),
            )
            .unwrap();
        self.sync.notify();
    }

    pub fn copy(&mut self, src: Page, dest: Page, scroll: i16) {
        self.flush_draws();
        self.do_copy(GlPage::Game(src), GlPage::Game(dest), scroll);
        self.sync.notify();
    }

    fn do_copy(&self, src: GlPage, dest: GlPage, scroll: i16) {
        let dest_page = self.pages.get(&dest).unwrap();
        let mut frame = dest_page.frame(&self.display);
        frame.clear_depth(-1.0);

        let gpu_index_buffer = glium::index::NoIndices(PrimitiveType::TrianglesList);

        let src_page = self.pages.get(&src).unwrap();
        let uniforms = glium::uniform! {
            u_page: src_page.sampled(),
            u_fill: 255 as u32,
            u_scroll: scroll as i32
        };

        frame
            .draw(
                &self.screen_vertex_buffer,
                &gpu_index_buffer,
                &self.copy_program,
                &uniforms,
                &DrawParameters::default(),
            )
            .unwrap();
    }

    pub fn blit(&mut self, page: Page) {
        self.flush_draws();
        self.output_page = page;
        self.redraw();
        self.sync.notify();
    }

    pub fn select(&mut self, page: Page) {
        self.flush_draws();
        self.active_page = page;
        self.sync.notify();
    }

    pub fn string(&mut self, text: &'static str, color: u8, mut x: i16, mut y: i16) {
        self.flush_draws();
        self.text_buffer.clear();

        let x_origin = x;
        for c in text.bytes() {
            if c == b'\n' {
                x = x_origin;
                y += 8;
                continue;
            }

            let c = c - b' ';

            let x_ind = (c % 10) * 8;
            let y_ind = (c / 10) * 8;

            let x_ind = x_ind as f32 / 80.0;
            let y_ind = y_ind as f32 / 80.0;

            let step = 8.0 / 80.0;

            let x_pos = x as f32;
            let y_pos = y as f32;

            x += 8;

            self.text_buffer.push(TextPoint {
                position: (x_pos, y_pos),
                uv: (x_ind, y_ind),
            });

            self.text_buffer.push(TextPoint {
                position: (x_pos, y_pos + 8.0),
                uv: (x_ind, y_ind + step),
            });

            self.text_buffer.push(TextPoint {
                position: (x_pos + 8.0, y_pos),
                uv: (x_ind + step, y_ind),
            });

            self.text_buffer.push(TextPoint {
                position: (x_pos + 8.0, y_pos + 8.0),
                uv: (x_ind + step, y_ind + step),
            });

            self.text_buffer.push(TextPoint {
                position: (x_pos, y_pos + 8.0),
                uv: (x_ind, y_ind + step),
            });

            self.text_buffer.push(TextPoint {
                position: (x_pos + 8.0, y_pos),
                uv: (x_ind + step, y_ind),
            });
        }

        let gpu_vertex_buffer = VertexBuffer::new(&self.display, &self.text_buffer).unwrap();
        let gpu_index_buffer = glium::index::NoIndices(PrimitiveType::TrianglesList);

        let uniforms = glium::uniform! {
            u_font_atlas: self.font_texture.sampled().minify_filter(glium::uniforms::MinifySamplerFilter::Nearest).magnify_filter(glium::uniforms::MagnifySamplerFilter::Nearest),
            u_color: color as u32,
        };

        let page = self.pages.get(&GlPage::Game(self.active_page)).unwrap();
        let mut page_frame = page.frame(&self.display);
        page_frame
            .draw(
                &gpu_vertex_buffer,
                &gpu_index_buffer,
                &self.font_program,
                &uniforms,
                &DrawParameters::default(),
            )
            .unwrap();
        self.sync.notify();
    }

    fn flush_draws(&mut self) {
        let mut state = self.state.lock().unwrap();

        let poly_count = state.polygons.len();
        let mut current_poly = 0;
        let mut pending_polys;
        let mut special;

        let fill_options = FillOptions::default();

        while current_poly < poly_count {
            pending_polys = 0;
            special = false;
            while current_poly < poly_count {
                if let Some(poly) = state.polygons.get(current_poly) {
                    let (color, mask) = match poly.blend {
                        BlendMode::Solid(col) => (col & 0xf, 0),
                        BlendMode::Mask(mask) if pending_polys == 0 => {
                            special = true;
                            (0, mask)
                        }
                        BlendMode::Mask(_) => break,
                        BlendMode::Blend if pending_polys == 0 => {
                            special = true;
                            (0xff, 0)
                        }
                        BlendMode::Blend => break,
                    };
                    let mut points = poly
                        .points()
                        .map(|(x, y)| lyon::math::point(x as f32, y as f32));

                    if let Some(first) = points.next() {
                        let mut buffer_builder = BuffersBuilder::new(
                            &mut self.tessellate_buffer,
                            |vertex: FillVertex| PolyPoint {
                                position: vertex.position().to_tuple(),
                                color,
                                depth: current_poly as u16,
                                mask,
                            },
                        );

                        let mut builder =
                            self.tessellator.builder(&fill_options, &mut buffer_builder);

                        builder.begin(first);
                        for point in points {
                            builder.line_to(point);
                        }
                        builder.close();

                        let _ = builder.build().unwrap();
                    }
                    pending_polys += 1;
                    current_poly += 1;

                    if special {
                        break;
                    }
                }
            }

            let page = self.pages.get(&GlPage::Game(self.active_page)).unwrap();
            let mut page_frame = page.frame(&self.display);
            page_frame.clear_depth(-1.0);

            let gpu_vertex_buffer =
                VertexBuffer::new(&self.display, &self.tessellate_buffer.vertices).unwrap();
            let gpu_index_buffer = IndexBuffer::new(
                &self.display,
                glium::index::PrimitiveType::TrianglesList,
                &self.tessellate_buffer.indices,
            )
            .unwrap();

            let page_self = self.pages.get(&GlPage::Current).unwrap();
            let page_zero = self.pages.get(&GlPage::Zero).unwrap();

            if special {
                self.do_copy(GlPage::Game(self.active_page), GlPage::Current, 0);
                self.do_copy(GlPage::Game(Page::Zero), GlPage::Zero, 0);
            }

            let uniforms = glium::uniform! {
                u_max_depth: poly_count as u32 + 1,
                u_page_zero: page_zero.sampled(),
                u_page_self: page_self.sampled(),
            };

            let page_params = DrawParameters {
                depth: glium::Depth {
                    test: glium::DepthTest::IfMoreOrEqual,
                    write: true,
                    ..Default::default()
                },
                ..Default::default()
            };

            page_frame
                .draw(
                    &gpu_vertex_buffer,
                    &gpu_index_buffer,
                    &self.page_program,
                    &uniforms,
                    &page_params,
                )
                .unwrap();

            self.tessellate_buffer.indices.clear();
            self.tessellate_buffer.vertices.clear();
        }

        state.polygons.clear();
    }

    pub fn redraw(&mut self) {
        self.flush_draws();

        let mut state = self.state.lock().unwrap();
        self.palette.update(&mut state.palette);

        let mut frame = self.display.draw();
        frame.clear_color_srgb(0.0, 0.0, 0.0, 1.0);

        let gpu_index_buffer = glium::index::NoIndices(PrimitiveType::TrianglesList);

        let output_page = self.pages.get(&GlPage::Game(self.output_page)).unwrap();
        let uniforms = glium::uniform! {
            u_palette: self.palette.sampled(),
            u_page: output_page.sampled(),
            u_font_atlas: self.font_texture.sampled()
        };

        frame
            .draw(
                &self.screen_vertex_buffer,
                &gpu_index_buffer,
                &self.frame_program,
                &uniforms,
                &DrawParameters::default(),
            )
            .unwrap();

        frame.finish().unwrap();
    }
}

#[derive(Copy, Clone)]
struct PolyPoint {
    position: (f32, f32),
    color: u8,
    depth: u16,
    mask: u8,
}

glium::implement_vertex!(PolyPoint, position, color, depth, mask);

#[derive(Copy, Clone)]
struct QuadPoint {
    position: (f32, f32),
}
glium::implement_vertex!(QuadPoint, position);

#[derive(Copy, Clone)]
struct TextPoint {
    position: (f32, f32),
    uv: (f32, f32),
}
glium::implement_vertex!(TextPoint, position, uv);

pub struct GlHandle {
    state: Arc<Mutex<GfxState>>,
    sync: Arc<Sync>,
    proxy: EventLoopProxy<UserEvent>,
}

impl Gfx for GlHandle {
    fn blit(&mut self, page: Page) {
        let _ = self.proxy.send_event(UserEvent::Blit(page));
        self.sync.wait();
    }

    fn draw_polygon(&mut self, polygon: Polygon) {
        let mut state = self.state.lock().unwrap();
        state.polygons.push(polygon);
    }

    fn fill_page(&mut self, page: Page, color: u8) {
        let _ = self.proxy.send_event(UserEvent::Fill(page, color));
        self.sync.wait();
    }

    fn copy_page(&mut self, src: Page, dest: Page, scroll: i16) {
        let _ = self.proxy.send_event(UserEvent::Copy(src, dest, scroll));
        self.sync.wait();
    }

    fn set_palette(&mut self, palette: [(u8, u8, u8); 16]) {
        let mut state = self.state.lock().unwrap();
        state.palette = Some(palette);
    }

    fn select_page(&mut self, page: Page) {
        let _ = self.proxy.send_event(UserEvent::Select(page));
        self.sync.wait();
    }

    fn draw_string(&mut self, text: &'static str, color: u8, x: i16, y: i16) {
        let _ = self.proxy.send_event(UserEvent::String(text, color, x, y));
        self.sync.wait();
    }
}
