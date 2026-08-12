use std::error::Error;
use std::fmt;

use crate::engine::font;

/// Logical rendering resolution. 480 x 270 is 16:9 and scales with an exact
/// integer factor to 1920 x 1080 (x4) and 3840 x 2160 (x8).
pub const WIDTH: usize = 480;
pub const HEIGHT: usize = 270;

/// Number of entries in the palette. Every framebuffer pixel is one 8-bit
/// index into it.
pub const PALETTE_SIZE: usize = 256;

/// Palette index reserved for transparency. Sprite blits skip pixels holding
/// this index instead of drawing them.
pub const TRANSPARENT: u8 = 255;

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

    /// Convert one RGBA8 pixel to a palette index. Alpha is thresholded: a
    /// pixel that is at least 50% transparent (alpha <= 128) maps to
    /// `TRANSPARENT`; anything less transparent becomes fully opaque and takes
    /// the nearest palette entry. Used to preprocess images once at load time,
    /// so `draw_image` never has to look colors up per frame.
    pub fn quantize_rgba(&self, rgba: [u8; 4]) -> u8 {
        if rgba[3] <= 128 {
            TRANSPARENT
        } else {
            self.index_of(Color::rgb(rgba[0], rgba[1], rgba[2]))
        }
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

/// Maximum run length that fits in a control byte's six low bits.
pub(crate) const MAX_RUN: usize = 63;

/// Control type `00`: the next `n` bytes are opaque palette indices.
pub(crate) const CT_OPAQUE: u8 = 0b00 << 6;
/// Control type `01`: skip the next `n` pixels as transparent.
pub(crate) const CT_SKIP: u8 = 0b01 << 6;
/// Control type `10`: skip `n` scan lines and restart at the first pixel of
/// that line (count 1 = switch to the next scan line). Every scan line is
/// terminated with one of these.
pub(crate) const CT_NEXT: u8 = 0b10 << 6;
/// The count bits (low six) of a control byte.
pub(crate) const CT_COUNT: u8 = 0x3F;

/// Errors raised while decoding an RLE sprite stream.
#[derive(Debug)]
pub enum RleError {
    /// A control byte with type bits `11`, which the format does not define.
    ReservedControlType { offset: usize },
    /// An opaque run (`00`) promises more index bytes than the stream holds.
    TruncatedRun { offset: usize },
}

impl fmt::Display for RleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RleError::ReservedControlType { offset } => {
                write!(f, "control type 11 at byte {offset} is not defined")
            }
            RleError::TruncatedRun { offset } => {
                write!(
                    f,
                    "opaque run at byte {offset} runs past the end of the data"
                )
            }
        }
    }
}

impl Error for RleError {}

/// A single opaque run of an RLE stream: `indices.len()` consecutive pixels
/// starting at column `x` of scan line `y`.
pub(crate) struct RleRun<'a> {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) indices: &'a [u8],
}

/// Streaming decoder over an RLE sprite stream. Advances `(x, y)` exactly as
/// the encoder did and yields the opaque runs in order.
pub(crate) struct RleDecoder<'a> {
    data: &'a [u8],
    pos: usize,
    x: usize,
    y: usize,
}

impl<'a> RleDecoder<'a> {
    pub(crate) fn new(data: &'a [u8]) -> RleDecoder<'a> {
        RleDecoder {
            data,
            pos: 0,
            x: 0,
            y: 0,
        }
    }

    /// The next opaque run, or `None` at the end of the stream.
    pub(crate) fn next_run(&mut self) -> Result<Option<RleRun<'a>>, RleError> {
        while self.pos < self.data.len() {
            let control = self.data[self.pos];
            let offset = self.pos;
            self.pos += 1;
            let count = (control & CT_COUNT) as usize;
            match control >> 6 {
                0 => {
                    let end = self.pos + count;
                    if end > self.data.len() {
                        return Err(RleError::TruncatedRun { offset });
                    }
                    let run = RleRun {
                        x: self.x,
                        y: self.y,
                        indices: &self.data[self.pos..end],
                    };
                    self.pos = end;
                    self.x += count;
                    return Ok(Some(run));
                }
                1 => self.x += count,
                2 => {
                    self.y += count;
                    self.x = 0;
                }
                _ => return Err(RleError::ReservedControlType { offset }),
            }
        }
        Ok(None)
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

    /// Blit an RLE-encoded sprite at `(x, y)`, clipped to the screen. The
    /// stream (see `sprites::rle_encode`) is control bytes `xx yyyyyy`: type
    /// `00` is followed by `n` opaque palette indices on the current scan
    /// line, type `01` skips `n` transparent pixels and type `10` skips `n`
    /// scan lines (count 1 = next line), ending at the first pixel of that
    /// line. Only opaque runs are drawn, so transparent pixels never touch the
    /// target. Renderers decode the stream and write the runs straight into
    /// their buffer.
    fn draw_rle_image(&mut self, x: i32, y: i32, data: &[u8]);
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

    fn draw_rle_image(&mut self, x: i32, y: i32, data: &[u8]) {
        let mut pos = 0usize;

        let mut cx = x;
        let mut cy = y;

        while pos < data.len() {
            let control = data[pos];
            pos += 1;

            let count = (control & CT_COUNT) as i32;

            match control >> 6 {
                0 => {
                    // Opaque run: the next `count` bytes are palette indices.
                    let available = data.len() - pos;
                    let count = (count as usize).min(available);

                    if count == 0 {
                        continue;
                    }

                    let run_start = pos;
                    let run_end = pos + count;
                    let run = &data[run_start..run_end];

                    // Advance encoded-data position regardless of clipping.
                    pos = run_end;

                    // Entirely above/below the framebuffer.
                    if cy < 0 {
                        cx += count as i32;
                        continue;
                    }

                    if cy >= self.height() as i32 {
                        break;
                    }

                    let run_start_x = cx;
                    let run_end_x = cx + count as i32;

                    // Clip horizontally.
                    let visible_start_x = run_start_x.max(0);
                    let visible_end_x = run_end_x.min(self.width() as i32);

                    if visible_start_x < visible_end_x {
                        let src_start = (visible_start_x - run_start_x) as usize;
                        let src_end = (visible_end_x - run_start_x) as usize;

                        let dst_start = cy as usize * WIDTH + visible_start_x as usize;

                        self.pixels[dst_start..dst_start + (src_end - src_start)]
                            .copy_from_slice(&run[src_start..src_end]);
                    }

                    cx = run_end_x;
                }

                1 => {
                    // Transparent run.
                    cx += count;
                }

                2 => {
                    // Skip scan lines.
                    cy += count;
                    cx = x;
                }

                _ => {
                    // Reserved control type.
                    break;
                }
            }
        }
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

    /// Helper: a 2x1 image of `indices` (one byte per pixel).
    fn indexed(a: u8, b: u8) -> [u8; 2] {
        [a, b]
    }

    #[test]
    fn quantize_rgba_thresholds_alpha() {
        let palette = Palette::default();

        // >= 50% transparent (alpha <= 128) becomes fully transparent.
        assert_eq!(palette.quantize_rgba([255, 0, 0, 128]), TRANSPARENT);
        assert_eq!(palette.quantize_rgba([255, 0, 0, 0]), TRANSPARENT);

        // < 50% transparent becomes opaque and maps to the nearest entry.
        let idx = palette.quantize_rgba([255, 255, 255, 255]);
        assert_eq!(palette.rgb(idx), Color::WHITE);
        let idx = palette.quantize_rgba([255, 255, 255, 127]);
        assert_eq!(palette.rgb(idx), Color::WHITE);
    }

    #[test]
    fn draw_rle_image_blits_only_opaque_runs() {
        let mut fb = Framebuffer::new();
        fb.clear(Color::BLACK);
        let white = fb.palette.index_of(Color::WHITE);
        let black = fb.palette.index_of(Color::BLACK);
        // Hand-written stream for a 4x2 sprite:
        //   row 0: skip 1, opaque [a, b] at (1,0), next line
        //   row 1: opaque [c] at (0,1)
        // Stream: 41 02 a b 81 01 c
        let stream = [0x41, 0x02, white, white, 0x81, 0x01, white];
        fb.draw_rle_image(0, 0, &stream);

        // Skipped and trailing pixels stay on the background.
        assert_eq!(pixel(&fb, 0, 0), black);
        assert_eq!(pixel(&fb, 3, 0), black);
        assert_eq!(pixel(&fb, 1, 1), black);
        // Opaque pixels land exactly on their encoded positions.
        assert_eq!(pixel(&fb, 1, 0), white);
        assert_eq!(pixel(&fb, 2, 0), white);
        assert_eq!(pixel(&fb, 0, 1), white);

        // Partially visible placement clips without panicking and draws the
        // visible part of the run.
        fb.clear(Color::BLACK);
        fb.draw_rle_image(-2, 0, &stream);
        assert_eq!(pixel(&fb, 0, 0), white); // second pixel of the x=1 run
        assert_eq!(pixel(&fb, 0, 1), black); // row 1 is fully off-screen
    }
}
