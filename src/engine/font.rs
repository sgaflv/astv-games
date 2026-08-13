use crate::engine::color::Color;
use crate::engine::render::{Framebuffer, Renderer};

/// 8x8 bitmap font glyphs for U+0000..U+007F (public domain font8x8).
use font8x8::legacy::BASIC_LEGACY;

pub const GLYPH_W: i32 = 8;
pub const GLYPH_H: i32 = 8;

/// Blit `text` into the framebuffer at `(x, y)` (top-left) using the 8x8
/// bitmap font scaled by the integer `scale` factor. Out-of-bounds pixels are
/// clipped. Non-ASCII characters fall back to '?'.
pub fn draw_text(fb: &mut Framebuffer, x: i32, y: i32, scale: i32, color: Color, text: &str) {
    let scale = scale.max(1);
    let mut cx = x;
    for ch in text.chars() {
        if ch == '\n' {
            cx = x;
            continue;
        }
        let glyph = glyph(ch);
        for row in 0..GLYPH_H {
            let bits = glyph[row as usize];
            for col in 0..GLYPH_W {
                if bits & (1 << (7 - col)) != 0 {
                    fb.fill_rect(
                        cx + (GLYPH_W - col * scale),
                        y + row * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        cx += GLYPH_W * scale;
    }
}

fn glyph(ch: char) -> &'static [u8; 8] {
    let idx = ch as u32;
    if idx < 128 {
        &BASIC_LEGACY[idx as usize]
    } else {
        &BASIC_LEGACY['?' as usize]
    }
}

/// Width in pixels of `text` at the given integer scale.
pub fn text_width(text: &str, scale: i32) -> i32 {
    text.chars().count() as i32 * GLYPH_W * scale.max(1)
}
