use crate::video::{Page, Polygon};

pub trait Gfx {
    fn blit(&mut self, page: Page);
    fn draw_polygon(&mut self, polygon: Polygon);
    fn fill_page(&mut self, page: Page, color: u8);
    fn select_page(&mut self, page: Page);
    fn copy_page(&mut self, src: Page, dest: Page, scroll: i16);
    fn set_palette(&mut self, palette: [(u8, u8, u8); 16]);
    fn draw_string(&mut self, text: &'static str, color: u8, x: i16, y: i16);
}
