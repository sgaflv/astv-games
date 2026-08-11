use crate::font;

/// Logical rendering resolution. 480 x 270 is 16:9 and scales with an exact
/// integer factor to 1920 x 1080 (x4) and 3840 x 2160 (x8).
pub const WIDTH: usize = 480;
pub const HEIGHT: usize = 270;

/// Bytes per logical frame: 480 * 270 * 3 = 388800 (~380 KiB).
pub const BYTES_PER_FRAME: usize = WIDTH * HEIGHT * 3;

/// 8-bit RGB color.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b }
    }

    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const BLACK: Color = Color::rgb(0, 0, 0);
}

/// Lightweight renderer abstraction. All coordinates are integer logical
/// pixels in the 480 x 270 space with a top-left origin.
pub trait Renderer {
    fn clear(&mut self, color: Color);
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color);
    fn fill_circle(&mut self, cx: i32, cy: i32, radius: i32, color: Color);
    fn draw_text(&mut self, x: i32, y: i32, scale: i32, color: Color, text: &str);
}

/// CPU software framebuffer. The game renders into this at logical resolution;
/// the GPU only uploads it and performs the final integer nearest-neighbour
/// upscale.
pub struct Framebuffer {
    pixels: Vec<u8>,
}

impl Framebuffer {
    pub fn new() -> Framebuffer {
        Framebuffer {
            pixels: vec![0; BYTES_PER_FRAME],
        }
    }

    #[inline]
    pub fn width(&self) -> usize {
        WIDTH
    }

    #[inline]
    pub fn height(&self) -> usize {
        HEIGHT
    }

    /// Raw RGB8 row-major pixels, ready for GPU upload.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Raw mutable RGB8 row-major pixels.
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    #[inline]
    fn set_pixel(&mut self, x: i32, y: i32, color: Color) {
        let idx = (y as usize * WIDTH + x as usize) * 3;
        self.pixels[idx] = color.r;
        self.pixels[idx + 1] = color.g;
        self.pixels[idx + 2] = color.b;
    }
}

impl Default for Framebuffer {
    fn default() -> Framebuffer {
        Framebuffer::new()
    }
}

impl Renderer for Framebuffer {
    fn clear(&mut self, color: Color) {
        for px in self.pixels.chunks_exact_mut(3) {
            px[0] = color.r;
            px[1] = color.g;
            px[2] = color.b;
        }
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(WIDTH as i32);
        let y1 = (y + h).min(HEIGHT as i32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let stride = WIDTH as i32;
        for py in y0..y1 {
            let base = (py * stride + x0) * 3;
            let row = &mut self.pixels[base as usize..(base + (x1 - x0) * 3) as usize];
            for px in row.chunks_exact_mut(3) {
                px[0] = color.r;
                px[1] = color.g;
                px[2] = color.b;
            }
        }
    }

    fn fill_circle(&mut self, cx: i32, cy: i32, radius: i32, color: Color) {
        if radius <= 0 {
            return;
        }
        let x0 = (cx - radius).max(0);
        let y0 = (cy - radius).max(0);
        let x1 = (cx + radius).min(WIDTH as i32 - 1);
        let y1 = (cy + radius).min(HEIGHT as i32 - 1);
        let r2 = radius * radius;
        for py in y0..=y1 {
            for px in x0..=x1 {
                let dx = px - cx;
                let dy = py - cy;
                if dx * dx + dy * dy <= r2 {
                    self.set_pixel(px, py, color);
                }
            }
        }
    }

    fn draw_text(&mut self, x: i32, y: i32, scale: i32, color: Color, text: &str) {
        font::draw_text(self, x, y, scale, color, text);
    }
}

/// Largest integer scale factor that fits the given physical output size
/// without distorting the 16:9 frame.
pub fn integer_scale(output_width: i32, output_height: i32) -> i32 {
    let sx = output_width / WIDTH as i32;
    let sy = output_height / HEIGHT as i32;
    sx.min(sy).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(fb: &Framebuffer, x: i32, y: i32) -> [u8; 3] {
        let idx = (y as usize * WIDTH + x as usize) * 3;
        let p = &fb.pixels[idx..idx + 3];
        [p[0], p[1], p[2]]
    }

    #[test]
    fn clear_fills_every_pixel() {
        let mut fb = Framebuffer::new();
        fb.clear(Color::rgb(1, 2, 3));
        assert_eq!(pixel(&fb, 0, 0), [1, 2, 3]);
        assert_eq!(pixel(&fb, WIDTH as i32 - 1, HEIGHT as i32 - 1), [1, 2, 3]);
        assert!(
            fb.pixels()
                .chunks_exact(3)
                .all(|px| { px[0] == 1 && px[1] == 2 && px[2] == 3 })
        );
    }

    #[test]
    fn fill_rect_clips_to_bounds() {
        let mut fb = Framebuffer::new();
        fb.clear(Color::BLACK);
        // Off-screen rect must not panic and must not draw.
        fb.fill_rect(-10, -10, 5, 5, Color::WHITE);
        assert_eq!(pixel(&fb, 0, 0), [0, 0, 0]);
        // Partially clipped rect.
        fb.fill_rect(WIDTH as i32 - 2, HEIGHT as i32 - 2, 10, 10, Color::WHITE);
        assert_eq!(
            pixel(&fb, WIDTH as i32 - 1, HEIGHT as i32 - 1),
            [255, 255, 255]
        );
        // Fully outside.
        fb.fill_rect(WIDTH as i32 + 1, 0, 10, 10, Color::WHITE);
        assert_eq!(pixel(&fb, WIDTH as i32 - 1, 0), [0, 0, 0]);
    }

    #[test]
    fn fill_circle_matches_radius() {
        let mut fb = Framebuffer::new();
        fb.clear(Color::BLACK);
        fb.fill_circle(10, 10, 5, Color::WHITE);
        assert_eq!(pixel(&fb, 10, 10), [255, 255, 255]);
        // Inside the radius.
        assert_eq!(pixel(&fb, 10, 14), [255, 255, 255]);
        assert_eq!(pixel(&fb, 14, 10), [255, 255, 255]);
        // Outside the radius (5,5 distance 7.07 > 5).
        assert_eq!(pixel(&fb, 5, 5), [0, 0, 0]);
        assert_eq!(pixel(&fb, 16, 10), [0, 0, 0]);
    }

    #[test]
    fn integer_scale_is_exact() {
        assert_eq!(integer_scale(1920, 1080), 4);
        assert_eq!(integer_scale(3840, 2160), 8);
        // Odd window sizes still produce the largest integer scale that fits.
        assert_eq!(integer_scale(1919, 1080), 3);
        assert_eq!(integer_scale(1000, 1000), 2);
        assert_eq!(integer_scale(100, 100), 1);
    }

    #[test]
    fn draw_text_paints_glyphs() {
        let mut fb = Framebuffer::new();
        fb.clear(Color::BLACK);
        fb.draw_text(0, 0, 1, Color::WHITE, "H");

        for y in 0..13 {
            for x in 0..13 {
                let c = if pixel(&fb, x, y) == [0, 0, 0] {
                    '.'
                } else {
                    '*'
                };

                print!("{c}");
            }
            println!();
        }
        // 'H' paints vertical bars at columns 2-3 and 6-7, joined by a bar in
        // row 3; column 0 and the bottom row are empty.
        assert_eq!(pixel(&fb, 0, 0), [0, 0, 0]);
        assert_eq!(pixel(&fb, 1, 0), [255, 255, 255]);
        assert_eq!(pixel(&fb, 2, 0), [255, 255, 255]);
        assert_eq!(pixel(&fb, 7, 0), [0, 0, 0]);
        assert_eq!(pixel(&fb, 3, 3), [255, 255, 255]);
        assert_eq!(pixel(&fb, 3, 7), [0, 0, 0]);
    }
}
