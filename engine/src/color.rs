/// Number of entries in the palette. Every framebuffer pixel is one 8-bit
/// index into it.
pub const PALETTE_SIZE: usize = 256;

/// Palette index reserved for transparency. Sprite blits skip pixels holding
/// this index instead of drawing them.
pub const TRANSPARENT: u8 = 255;

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
