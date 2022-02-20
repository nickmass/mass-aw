use byteorder::{LittleEndian, WriteBytesExt};
use lyon::{
    lyon_tessellation::{BuffersBuilder, FillOptions, FillVertex, VertexBuffers},
    path::traits::PathBuilder,
    tessellation::FillTessellator,
};
use wasm_bindgen::JsCast;
use web_sys::{window, HtmlCanvasElement, WebGlRenderingContext as GL};

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use engine::video::{BlendMode, Page, Polygon};
use engine::Gfx;

use crate::gl::*;
use crate::shaders;

pub struct WebGlGfx {
    context: Rc<GlContext>,
    palette_tex: GlTexture,
    pages: HashMap<Page, GlFrameBuffer>,
    current_page: Page,
    frame_program: GlProgram,
    copy_program: RefCell<GlProgram>,
    page_program: GlProgram,
    font_program: GlProgram,
    screen_quad: GlModel<QuadVertex>,
    tessellate_buffer: VertexBuffers<PolyVertex, u16>,
    tessellator: FillTessellator,
    work_texture_self: GlFrameBuffer,
    work_texture_zero: GlFrameBuffer,
    font_texture: GlTexture,
    text_buffer: Vec<TextVertex>,
}

impl WebGlGfx {
    pub fn new(width: u32, height: u32) -> Self {
        let window = window().unwrap();
        let document = window.document().unwrap();
        let canvas: HtmlCanvasElement = document
            .create_element("canvas")
            .unwrap()
            .dyn_into()
            .unwrap();
        let _ = canvas.set_attribute("width", &format!("{}", width));
        let _ = canvas.set_attribute("height", &format!("{}", height));
        let _ = canvas.set_attribute("style", "width: 100%; height: 100%; image-rendering: -moz-crisp-edges; image-rendering: pixelated;");
        let body = document.body().unwrap();
        let _ = body.append_with_node_1(canvas.as_ref());

        let context = Rc::new(GlContext::new(canvas));
        let palette_tex = GlTexture::new(context.clone(), 16, 1, PixelFormat::RGB);

        let mut pages = HashMap::new();
        let page = GlFrameBuffer::new(context.clone(), width, height);
        pages.insert(Page::Zero, page);
        let page = GlFrameBuffer::new(context.clone(), width, height);
        pages.insert(Page::One, page);
        let page = GlFrameBuffer::new(context.clone(), width, height);
        pages.insert(Page::Two, page);
        let page = GlFrameBuffer::new(context.clone(), width, height);
        pages.insert(Page::Three, page);

        let current_page = Page::Zero;

        let frame_program = GlProgram::new(
            context.clone(),
            shaders::FRAME_VERTEX_SHADER,
            shaders::FRAME_FRAGMENT_SHADER,
        );
        let copy_program = RefCell::new(GlProgram::new(
            context.clone(),
            shaders::COPY_VERTEX_SHADER,
            shaders::COPY_FRAGMENT_SHADER,
        ));
        let page_program = GlProgram::new(
            context.clone(),
            shaders::PAGE_VERTEX_SHADER,
            shaders::PAGE_FRAGMENT_SHADER,
        );
        let font_program = GlProgram::new(
            context.clone(),
            shaders::FONT_VERTEX_SHADER,
            shaders::FONT_FRAGMENT_SHADER,
        );

        let screen_quad = GlModel::new(context.clone(), SCREEN_QUAD);

        let tessellate_buffer: VertexBuffers<PolyVertex, u16> = VertexBuffers::new();

        let work_texture_self = GlFrameBuffer::new(context.clone(), width, height);
        let work_texture_zero = GlFrameBuffer::new(context.clone(), width, height);

        let font_texture = create_font(context.clone());

        Self {
            context,
            palette_tex,
            pages,
            current_page,
            frame_program,
            copy_program,
            page_program,
            font_program,
            screen_quad,
            tessellate_buffer,
            work_texture_self,
            work_texture_zero,
            tessellator: FillTessellator::new(),
            font_texture,
            text_buffer: Vec::new(),
        }
    }

    fn do_copy(&self, src: &GlFrameBuffer, dest: &GlFrameBuffer, scroll: i16) {
        let color = 0xff as i32;
        let scroll = scroll as i32;
        let mut uniforms = GlUniformCollection::new();
        uniforms.add("u_fill", &color);
        uniforms.add("u_page", src.texture());
        uniforms.add("u_scroll", &scroll);

        dest.bind();
        self.copy_program
            .borrow_mut()
            .draw(&self.screen_quad, &uniforms, None);
        dest.unbind();
    }
}

impl Gfx for WebGlGfx {
    fn blit(&mut self, page: Page) {
        let page = self.pages.get(&page).unwrap();
        let mut uniforms = GlUniformCollection::new();
        uniforms.add("u_page", page.texture());
        uniforms.add("u_palette", &self.palette_tex);

        self.frame_program.draw(&self.screen_quad, &uniforms, None);
    }

    fn draw_polygon(&mut self, polygon: Polygon) {
        let fill_options = FillOptions::default();
        let (color, mask) = match polygon.blend {
            BlendMode::Solid(col) => (col & 0xf, 0),
            BlendMode::Mask(mask) => (0, mask),
            BlendMode::Blend => (0xff, 0),
        };
        let mut points = polygon
            .points()
            .map(|(x, y)| lyon::math::point(x as f32, y as f32));

        if let Some(first) = points.next() {
            let mut buffer_builder =
                BuffersBuilder::new(&mut self.tessellate_buffer, |vertex: FillVertex| {
                    PolyVertex {
                        position: vertex.position().to_tuple(),
                        color,
                        mask,
                    }
                });

            let mut builder = self.tessellator.builder(&fill_options, &mut buffer_builder);

            builder.begin(first);
            for point in points {
                builder.line_to(point);
            }
            builder.close();

            let _ = builder.build().unwrap();
        }

        let page = self.pages.get(&self.current_page).unwrap();

        let page_self = self.pages.get(&self.current_page).unwrap();
        let page_zero = self.pages.get(&Page::Zero).unwrap();

        if color >= 0xf || mask != 0 {
            self.do_copy(page_self, &self.work_texture_self, 0);
            self.do_copy(page_zero, &self.work_texture_zero, 0);
        }

        let poly_model = GlModel::new(
            self.context.clone(),
            self.tessellate_buffer.vertices.iter().cloned(),
        );
        let poly_index = GlIndexBuffer::new(self.context.clone(), &self.tessellate_buffer.indices);
        let mut uniforms = GlUniformCollection::new();
        uniforms.add("u_page_self", self.work_texture_self.texture());
        uniforms.add("u_page_zero", self.work_texture_zero.texture());

        page.bind();
        self.page_program
            .draw_indexed(&poly_model, &uniforms, Some(&poly_index), None);
        page.unbind();

        self.tessellate_buffer.indices.clear();
        self.tessellate_buffer.vertices.clear();
    }

    fn fill_page(&mut self, page: Page, color: u8) {
        let color = color & 0xf;
        let dest_page = self.pages.get(&page).unwrap();

        let color = color as i32;
        let mut uniforms = GlUniformCollection::new();
        uniforms.add("u_fill", &color);
        uniforms.add("u_page", self.work_texture_self.texture());

        dest_page.bind();
        self.copy_program
            .borrow_mut()
            .draw(&self.screen_quad, &uniforms, None);
        dest_page.unbind();
    }
    fn copy_page(&mut self, src: Page, dest: Page, scroll: i16) {
        let dest_page = self.pages.get(&dest).unwrap();
        let src_page = self.pages.get(&src).unwrap();

        self.do_copy(src_page, dest_page, scroll);
    }

    fn select_page(&mut self, page: Page) {
        self.current_page = page;
    }

    fn set_palette(&mut self, palette: [(u8, u8, u8); 16]) {
        let pixels = palette
            .iter()
            .map(|p| [p.0, p.1, p.2])
            .flatten()
            .collect::<Vec<_>>();
        self.palette_tex
            .sub_image(0, 0, 16, 1, PixelFormat::RGB, pixels.as_slice());
    }

    fn draw_string(&mut self, text: &'static str, color: u8, mut x: i16, mut y: i16) {
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

            self.text_buffer.push(TextVertex {
                position: (x_pos, y_pos),
                uv: (x_ind, y_ind),
            });

            self.text_buffer.push(TextVertex {
                position: (x_pos, y_pos + 8.0),
                uv: (x_ind, y_ind + step),
            });

            self.text_buffer.push(TextVertex {
                position: (x_pos + 8.0, y_pos),
                uv: (x_ind + step, y_ind),
            });

            self.text_buffer.push(TextVertex {
                position: (x_pos + 8.0, y_pos + 8.0),
                uv: (x_ind + step, y_ind + step),
            });

            self.text_buffer.push(TextVertex {
                position: (x_pos, y_pos + 8.0),
                uv: (x_ind, y_ind + step),
            });

            self.text_buffer.push(TextVertex {
                position: (x_pos + 8.0, y_pos),
                uv: (x_ind + step, y_ind),
            });
        }

        let text_model = GlModel::new(self.context.clone(), self.text_buffer.iter().cloned());

        let color = color as i32;
        let mut uniforms = GlUniformCollection::new();
        uniforms.add("u_font_atlas", &self.font_texture);
        uniforms.add("u_color", &color);

        let page = self.pages.get(&self.current_page).unwrap();
        page.bind();
        self.font_program.draw(&text_model, &uniforms, None);
        page.unbind();
    }
}

fn create_font(context: Rc<GlContext>) -> GlTexture {
    let mut font_data = vec![0u8; 100 * 64];
    for n in 0..96 {
        let x_ind = (n % 10) * 8;
        let y_ind = (n / 10) * 8;

        for y in 0..8 {
            let mut row = engine::font::FONT[(n * 8) + y];
            for x in 0..8 {
                let bit = row & 0x80 != 0;
                row <<= 1;
                let color = if bit { 0xff } else { 0x00 };

                let x_off = x_ind + x;
                let y_off = y_ind + y;

                let index = (y_off * 80) + x_off;

                font_data[index] = color;
            }
        }
    }

    let texture = GlTexture::new(context, 80, 80, PixelFormat::Alpha);
    texture.sub_image(0, 0, 80, 80, PixelFormat::Alpha, font_data.as_slice());
    texture
}

const SCREEN_QUAD: [QuadVertex; 6] = [
    QuadVertex {
        position: (-1.0, -1.0),
    },
    QuadVertex {
        position: (1.0, -1.0),
    },
    QuadVertex {
        position: (-1.0, 1.0),
    },
    QuadVertex {
        position: (1.0, 1.0),
    },
    QuadVertex {
        position: (1.0, -1.0),
    },
    QuadVertex {
        position: (-1.0, 1.0),
    },
];

#[derive(Debug, Clone, Copy)]
struct QuadVertex {
    position: (f32, f32),
}

impl AsGlVertex for QuadVertex {
    const ATTRIBUTES: &'static [(&'static str, GlValueType)] = &[("position", GlValueType::Vec2)];
    const POLY_TYPE: u32 = GL::TRIANGLES;
    const SIZE: usize = 8;

    fn write(&self, mut buf: impl std::io::Write) {
        let _ = buf.write_f32::<LittleEndian>(self.position.0);
        let _ = buf.write_f32::<LittleEndian>(self.position.1);
    }
}

#[derive(Debug, Clone, Copy)]
struct PolyVertex {
    position: (f32, f32),
    color: u8,
    mask: u8,
}

impl AsGlVertex for PolyVertex {
    const ATTRIBUTES: &'static [(&'static str, GlValueType)] = &[
        ("position", GlValueType::Vec2),
        ("color", GlValueType::Float),
        ("mask", GlValueType::Float),
    ];
    const POLY_TYPE: u32 = GL::TRIANGLES;
    const SIZE: usize = 16;

    fn write(&self, mut buf: impl std::io::Write) {
        let _ = buf.write_f32::<LittleEndian>(self.position.0);
        let _ = buf.write_f32::<LittleEndian>(self.position.1);
        let _ = buf.write_f32::<LittleEndian>(self.color as f32);
        let _ = buf.write_f32::<LittleEndian>(self.mask as f32);
    }
}

#[derive(Debug, Clone, Copy)]
struct TextVertex {
    position: (f32, f32),
    uv: (f32, f32),
}

impl AsGlVertex for TextVertex {
    const ATTRIBUTES: &'static [(&'static str, GlValueType)] =
        &[("position", GlValueType::Vec2), ("uv", GlValueType::Vec2)];
    const POLY_TYPE: u32 = GL::TRIANGLES;
    const SIZE: usize = 16;

    fn write(&self, mut buf: impl std::io::Write) {
        let _ = buf.write_f32::<LittleEndian>(self.position.0);
        let _ = buf.write_f32::<LittleEndian>(self.position.1);
        let _ = buf.write_f32::<LittleEndian>(self.uv.0);
        let _ = buf.write_f32::<LittleEndian>(self.uv.1);
    }
}
