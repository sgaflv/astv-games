use std::error::Error;
use std::fmt;

use crate::color::{Color, Palette};
use crate::font;

/// Logical rendering resolution. 480 x 270 is 16:9 and scales with an exact
/// integer factor to 1920 x 1080 (x4) and 3840 x 2160 (x8).
pub const WIDTH: usize = 480;
pub const HEIGHT: usize = 270;

/// Bytes per logical frame: 480 * 270 = 129600 (~127 KiB), one palette index
/// per pixel. This is a third of the previous RGB8 buffer, which is the point:
/// the CPU only touches indices, and the GPU turns them into colors.
pub const BYTES_PER_FRAME: usize = WIDTH * HEIGHT;

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

    /// Swap the palette this buffer is indexed against. Callers that change
    /// the active scene's palette must also re-upload the palette texture
    /// (see `Presenter::set_palette`).
    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
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
    use crate::color::{PAL_BRIGHT_WHITE, PALETTE_SIZE, TRANSPARENT};

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
    fn default_palette_round_trips_the_sixteen_colors() {
        let mut palette = Palette::default();
        assert_eq!(palette.len(), 16);
        assert_eq!(palette.bytes().len(), PALETTE_SIZE * 3);
        for idx in 0..16 {
            let color = palette.rgb(idx as u8);
            // Each default slot is its own exact entry, found by index_of.
            assert_eq!(palette.index_of(color), idx as u8);
            assert_eq!(palette.add(color), idx as u8);
        }
        // Arbitrary colors map to the nearest default entry (bright white for
        // white).
        assert_eq!(palette.index_of(Color::WHITE), PAL_BRIGHT_WHITE);
    }

    #[test]
    fn add_extends_the_palette_and_dedups() {
        let mut palette = Palette::default();

        let a = palette.add(Color::rgb(13, 13, 18));
        assert_eq!(a, 16);
        assert_eq!(palette.len(), 17);

        // Re-adding the same color reuses its slot.
        assert_eq!(palette.add(Color::rgb(13, 13, 18)), a);
        // An existing default color reuses its fixed slot.
        assert_eq!(palette.add(Color::WHITE), PAL_BRIGHT_WHITE);
        // index_of finds added colors exactly.
        assert_eq!(palette.index_of(Color::rgb(13, 13, 18)), a);
        assert_eq!(palette.rgb(a), Color::rgb(13, 13, 18));
    }

    #[test]
    fn add_falls_back_to_nearest_when_full() {
        let mut palette = Palette::default();
        // Fill every usable slot (indices 16..=254) with distinct colors that
        // collide with none of the 16 defaults.
        while palette.len() < PALETTE_SIZE - 1 {
            let c = palette.len() as u8;
            palette.add(Color::rgb(c, 1, 2));
        }
        assert_eq!(palette.len(), PALETTE_SIZE - 1);

        // No slot left: the nearest existing entry is returned, not appended,
        // and the palette does not grow.
        let near = palette.add(Color::rgb(1, 1, 1));
        assert_eq!(palette.len(), PALETTE_SIZE - 1);
        assert_eq!(near, palette.index_of(Color::rgb(1, 1, 1)));
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

    #[test]
    fn quantize_rgba_thresholds_alpha() {
        let mut palette = Palette::default();

        // >= 50% transparent (alpha <= 128) becomes fully transparent.
        assert_eq!(palette.quantize_rgba([255, 0, 0, 128]), TRANSPARENT);
        assert_eq!(palette.quantize_rgba([255, 0, 0, 0]), TRANSPARENT);

        // < 50% transparent becomes opaque and maps to its exact palette entry
        // (adding the color if it was new).
        let idx = palette.quantize_rgba([255, 255, 255, 129]);
        assert_eq!(palette.rgb(idx), Color::WHITE);

        // A brand-new color is appended to the palette.
        let idx = palette.quantize_rgba([13, 13, 18, 255]);
        assert_eq!(palette.rgb(idx), Color::rgb(13, 13, 18));
        assert_eq!(palette.len(), 17);
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
