use crate::engine::font;

/// Logical rendering resolution. 480 x 270 is 16:9 and scales with an exact
/// integer factor to 1920 x 1080 (x4) and 3840 x 2160 (x8).
pub const WIDTH: usize = 480;
pub const HEIGHT: usize = 270;

/// Number of entries in the palette. Every framebuffer pixel is one 8-bit
/// index into it.
pub const PALETTE_SIZE: usize = 256;

/// Bytes per logical frame: 480 * 270 = 129600 (~127 KiB), one palette index
/// per pixel. This is a third of the previous RGB8 buffer, which is the point:
/// the CPU only touches indices, and the GPU turns them into colors.
pub const BYTES_PER_FRAME: usize = WIDTH * HEIGHT;

/// 8-bit RGB color. The render buffer stores a `Palette` index, not the RGB
/// triple itself; `Palette::index_of` maps a color to its index at draw time.
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

// Fixed index slots of the default palette, in index order. The game's colors
// are pinned here so rendering stays stable even when new entries are added.
pub const PAL_BLACK: u8 = 0;
pub const PAL_BG: u8 = 1;
pub const PAL_GRID: u8 = 2;
pub const PAL_SNAKE_0: u8 = 3;
pub const PAL_SNAKE_1: u8 = 4;
pub const PAL_EYE: u8 = 5;
pub const PAL_TONGUE: u8 = 6;
pub const PAL_HUD: u8 = 7;
pub const PAL_MENU_DIM: u8 = 8;
pub const PAL_WHITE: u8 = 9;

/// A fixed 256-entry RGB palette. The framebuffer stores one 8-bit index per
/// pixel; the CPU renderer writes indices via `index_of` and the presentation
/// shader turns them back into RGB with a 256x1 palette texture.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Palette {
    /// Flat RGB entries, 3 bytes each in index order; ready for a 256x1 RGB8
    /// texture upload.
    rgb: [u8; PALETTE_SIZE * 3],
}

impl Palette {
    /// The RGB color for a palette index.
    #[inline]
    pub fn rgb(&self, index: u8) -> Color {
        let o = index as usize * 3;
        Color::rgb(self.rgb[o], self.rgb[o + 1], self.rgb[o + 2])
    }

    /// Flat RGB bytes (768), one palette entry every 3 bytes.
    #[inline]
    pub fn bytes(&self) -> &[u8] {
        &self.rgb
    }

    /// Index of the palette entry for `color`: exact match when the color is
    /// one of the entries (the game's colors are all pinned), otherwise the
    /// nearest entry. Used to store primitives as 8-bit indices.
    pub fn index_of(&self, color: Color) -> u8 {
        for idx in 0..PALETTE_SIZE {
            let o = idx * 3;
            if self.rgb[o] == color.r && self.rgb[o + 1] == color.g && self.rgb[o + 2] == color.b {
                return idx as u8;
            }
        }
        // Fallback (RGBA sprite blits until sprites are palette-indexed).
        let mut best = 0u8;
        let mut best_d = u32::MAX;
        for idx in 0..PALETTE_SIZE {
            let o = idx * 3;
            let dr = self.rgb[o] as i32 - color.r as i32;
            let dg = self.rgb[o + 1] as i32 - color.g as i32;
            let db = self.rgb[o + 2] as i32 - color.b as i32;
            let d = (dr * dr + dg * dg + db * db) as u32;
            if d < best_d {
                best_d = d;
                best = idx as u8;
                if d == 0 {
                    break;
                }
            }
        }
        best
    }
}

impl Default for Palette {
    /// The game palette: the colors the primitives actually use pinned at
    /// fixed indices, followed by a 6x6x6 RGB cube so arbitrary colors (e.g.
    /// sprite pixels) still map to a reasonable entry.
    fn default() -> Palette {
        const NAMED: [[u8; 3]; 10] = [
            [0, 0, 0],       // PAL_BLACK
            [13, 13, 18],    // PAL_BG (game background)
            [38, 38, 46],    // PAL_GRID
            [51, 204, 51],   // PAL_SNAKE_0
            [77, 148, 255],  // PAL_SNAKE_1
            [13, 13, 13],    // PAL_EYE
            [230, 77, 77],   // PAL_TONGUE
            [204, 204, 214], // PAL_HUD
            [120, 120, 130], // PAL_MENU_DIM
            [255, 255, 255], // PAL_WHITE
        ];

        let mut rgb = [0u8; PALETTE_SIZE * 3];
        for (i, c) in NAMED.iter().enumerate() {
            rgb[i * 3] = c[0];
            rgb[i * 3 + 1] = c[1];
            rgb[i * 3 + 2] = c[2];
        }

        let levels = [0u8, 51, 102, 153, 204, 255];
        let mut i = NAMED.len();
        'cube: for &r in &levels {
            for &g in &levels {
                for &b in &levels {
                    if i >= PALETTE_SIZE {
                        break 'cube;
                    }
                    rgb[i * 3] = r;
                    rgb[i * 3 + 1] = g;
                    rgb[i * 3 + 2] = b;
                    i += 1;
                }
            }
        }

        Palette { rgb }
    }
}

/// Lightweight renderer abstraction. All coordinates are integer logical
/// pixels in the 480 x 270 space with a top-left origin. Colors are written
/// as palette indices.
pub trait Renderer {
    fn clear(&mut self, color: Color);
    fn zero(&mut self);
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color);
    fn fill_circle(&mut self, cx: i32, cy: i32, radius: i32, color: Color);
    fn draw_text(&mut self, x: i32, y: i32, scale: i32, color: Color, text: &str);
    /// Blit an RGBA8 image (row-major, 4 bytes per pixel) at `(x, y)`,
    /// alpha-composited over the existing pixels and clipped to the screen.
    /// The result is quantized to the nearest palette entry.
    fn draw_image(&mut self, x: i32, y: i32, pixels: &[u8], width: usize, height: usize);
}

/// CPU software framebuffer. The game renders into this at logical resolution;
/// every pixel is one 8-bit palette index. The GPU uploads the index buffer
/// and performs the palette lookup plus the integer nearest-neighbour upscale.
pub struct Framebuffer {
    /// Palette indices, row-major.
    pixels: Vec<u8>,
    /// Scratch row for `fill_rect`.
    buffer: Vec<u8>,
    palette: Palette,
}

impl Framebuffer {
    pub fn new() -> Framebuffer {
        Framebuffer {
            pixels: vec![0; BYTES_PER_FRAME],
            buffer: vec![0; WIDTH],
            palette: Palette::default(),
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

    /// Raw palette indices, row-major, one byte per pixel, ready for GPU
    /// upload as an 8-bit texture.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Raw mutable palette indices, row-major, one byte per pixel.
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    /// The palette the render buffer is indexed against.
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    #[inline]
    fn set_pixel(&mut self, x: i32, y: i32, index: u8) {
        self.pixels[y as usize * WIDTH + x as usize] = index;
    }
}

impl Default for Framebuffer {
    fn default() -> Framebuffer {
        Framebuffer::new()
    }
}

impl Renderer for Framebuffer {
    fn clear(&mut self, color: Color) {
        let idx = self.palette.index_of(color);
        self.pixels.fill(idx);
    }

    fn zero(&mut self) {
        self.pixels.fill(0);
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        let idx = self.palette.index_of(color);

        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(WIDTH as i32);
        let y1 = (y + h).min(HEIGHT as i32);

        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let x0 = x0 as usize;
        let y0 = y0 as usize;
        let x1 = x1 as usize;
        let y1 = y1 as usize;

        let row_bytes = x1 - x0;

        self.buffer[..row_bytes].fill(idx);

        let source = &self.buffer[..row_bytes];

        for py in y0..y1 {
            let dst = py * WIDTH + x0;
            self.pixels[dst..dst + row_bytes].copy_from_slice(source);
        }
    }

    fn fill_circle(&mut self, cx: i32, cy: i32, radius: i32, color: Color) {
        if radius <= 0 {
            return;
        }
        let idx = self.palette.index_of(color);
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
                    self.set_pixel(px, py, idx);
                }
            }
        }
    }

    fn draw_text(&mut self, x: i32, y: i32, scale: i32, color: Color, text: &str) {
        font::draw_text(self, x, y, scale, color, text);
    }

    fn draw_image(&mut self, x: i32, y: i32, image: &[u8], width: usize, height: usize) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + width as i32).min(WIDTH as i32);
        let y1 = (y + height as i32).min(HEIGHT as i32);

        if x0 >= x1 || y0 >= y1 || width == 0 || height == 0 {
            return;
        }

        // Placeholder until sprites are palette-indexed themselves: each opaque
        // pixel is alpha-composited over the indexed backdrop and the result is
        // quantized back to the nearest palette entry.
        for py in y0..y1 {
            let row = (py - y) as usize;

            for px in x0..x1 {
                let col = (px - x) as usize;
                let src = (row * width + col) * 4;
                let a = image[src + 3] as u32;

                if a == 0 {
                    continue;
                }

                let fg = Color::rgb(image[src], image[src + 1], image[src + 2]);
                let dst = py as usize * WIDTH + px as usize;

                let index = if a == 255 {
                    self.palette.index_of(fg)
                } else {
                    let bg = self.palette.rgb(self.pixels[dst]);
                    self.palette.index_of(blend(bg, fg, a))
                };

                self.pixels[dst] = index;
            }
        }
    }
}

/// Alpha-composite `fg` over `bg` with the given alpha (0..=255), rounding the
/// channel values like the original RGB renderer did.
fn blend(bg: Color, fg: Color, alpha: u32) -> Color {
    let inv = 255 - alpha;
    Color::rgb(
        ((bg.r as u32 * inv + fg.r as u32 * alpha) / 255) as u8,
        ((bg.g as u32 * inv + fg.g as u32 * alpha) / 255) as u8,
        ((bg.b as u32 * inv + fg.b as u32 * alpha) / 255) as u8,
    )
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

    /// The palette index stored at (x, y).
    fn pixel(fb: &Framebuffer, x: i32, y: i32) -> u8 {
        fb.pixels[y as usize * WIDTH + x as usize]
    }

    /// The RGB color of the palette entry stored at (x, y).
    fn rgb_at(fb: &Framebuffer, x: i32, y: i32) -> [u8; 3] {
        let c = fb.palette.rgb(pixel(fb, x, y));
        [c.r, c.g, c.b]
    }

    #[test]
    fn clear_fills_every_pixel() {
        let mut fb = Framebuffer::new();
        fb.clear(Color::rgb(1, 2, 3));
        // (1,2,3) is not in the palette; clear still converges on one entry.
        let idx = pixel(&fb, 0, 0);
        assert_eq!(pixel(&fb, WIDTH as i32 - 1, HEIGHT as i32 - 1), idx);
        assert!(fb.pixels().iter().all(|&p| p == idx));
    }

    #[test]
    fn palette_round_trips_the_game_colors_exactly() {
        let palette = Palette::default();
        let colors = [
            Color::BLACK,
            Color::WHITE,
            Color::rgb(13, 13, 18),
            Color::rgb(38, 38, 46),
            Color::rgb(51, 204, 51),
            Color::rgb(77, 148, 255),
            Color::rgb(13, 13, 13),
            Color::rgb(230, 77, 77),
            Color::rgb(204, 204, 214),
            Color::rgb(120, 120, 130),
        ];
        for color in colors {
            let idx = palette.index_of(color);
            assert_eq!(palette.rgb(idx), color, "nearest match must be exact");
            assert_eq!(palette.bytes().len(), PALETTE_SIZE * 3);
        }
    }

    #[test]
    fn fill_rect_clips_to_bounds() {
        let mut fb = Framebuffer::new();
        fb.clear(Color::BLACK);
        let white = fb.palette.index_of(Color::WHITE);
        // Off-screen rect must not panic and must not draw.
        fb.fill_rect(-10, -10, 5, 5, Color::WHITE);
        assert_eq!(pixel(&fb, 0, 0), fb.palette.index_of(Color::BLACK));
        // Partially clipped rect.
        fb.fill_rect(WIDTH as i32 - 2, HEIGHT as i32 - 2, 10, 10, Color::WHITE);
        assert_eq!(pixel(&fb, WIDTH as i32 - 1, HEIGHT as i32 - 1), white);
        // Fully outside.
        fb.fill_rect(WIDTH as i32 + 1, 0, 10, 10, Color::WHITE);
        assert_eq!(
            pixel(&fb, WIDTH as i32 - 1, 0),
            fb.palette.index_of(Color::BLACK)
        );
    }

    #[test]
    fn fill_circle_matches_radius() {
        let mut fb = Framebuffer::new();
        fb.clear(Color::BLACK);
        let white = fb.palette.index_of(Color::WHITE);
        fb.fill_circle(10, 10, 5, Color::WHITE);
        assert_eq!(pixel(&fb, 10, 10), white);
        // Inside the radius.
        assert_eq!(pixel(&fb, 10, 14), white);
        assert_eq!(pixel(&fb, 14, 10), white);
        // Outside the radius (5,5 distance 7.07 > 5).
        assert_eq!(pixel(&fb, 5, 5), fb.palette.index_of(Color::BLACK));
        assert_eq!(pixel(&fb, 16, 10), fb.palette.index_of(Color::BLACK));
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

        // 'H' paints vertical bars at columns 2-3 and 6-7, joined by a bar in
        // row 3; column 0 and the bottom row are empty.
        assert_eq!(rgb_at(&fb, 0, 0), [0, 0, 0]);
        assert_eq!(rgb_at(&fb, 1, 0), [255, 255, 255]);
        assert_eq!(rgb_at(&fb, 2, 0), [255, 255, 255]);
        assert_eq!(rgb_at(&fb, 7, 0), [0, 0, 0]);
        assert_eq!(rgb_at(&fb, 3, 3), [255, 255, 255]);
        assert_eq!(rgb_at(&fb, 3, 7), [0, 0, 0]);
    }

    /// Helper: RGBA8 image of the given color, 2x1 with the requested alpha.
    fn rgba(r: u8, g: u8, b: u8, a: u8) -> [u8; 8] {
        [r, g, b, a, r, g, b, a]
    }

    #[test]
    fn draw_image_composites_alpha() {
        let mut fb = Framebuffer::new();
        fb.clear(Color::BLACK);

        // Opaque white fully replaces the background.
        fb.draw_image(0, 0, &rgba(255, 255, 255, 255), 2, 1);
        assert_eq!(rgb_at(&fb, 0, 0), [255, 255, 255]);

        // Fully transparent leaves the background untouched.
        fb.draw_image(0, 1, &rgba(255, 0, 0, 0), 2, 1);
        assert_eq!(rgb_at(&fb, 0, 1), [0, 0, 0]);

        // Half alpha blends source and background, then quantizes: the stored
        // entry must be the palette's closest match to the blend (128,0,0).
        fb.draw_image(0, 2, &rgba(255, 0, 0, 128), 2, 1);
        let blend = Color::rgb(128, 0, 0);
        assert_eq!(pixel(&fb, 0, 2), fb.palette.index_of(blend));
    }

    #[test]
    fn draw_image_clips_to_bounds() {
        let mut fb = Framebuffer::new();
        fb.clear(Color::BLACK);
        // Image hanging off the top-left corner clips without panicking.
        let white4x4 = [255u8, 255, 255, 255].repeat(4 * 4);
        fb.draw_image(-2, -2, &white4x4, 4, 4);
        assert_eq!(rgb_at(&fb, 0, 0), [255, 255, 255]);
        // Fully off-screen is a no-op.
        fb.draw_image(WIDTH as i32 + 1, 0, &white4x4, 4, 4);
        assert_eq!(rgb_at(&fb, WIDTH as i32 - 1, 0), [0, 0, 0]);
    }
}
