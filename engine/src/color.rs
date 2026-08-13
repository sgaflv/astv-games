/// Number of entries in the palette. Every framebuffer pixel is one 8-bit
/// index into it.
pub const PALETTE_SIZE: usize = 256;

/// Palette index reserved for transparency. Sprite blits skip pixels holding
/// this index instead of drawing them. Indices `0..PALETTE_SIZE - 1`
/// (`0..=254`) are usable colors; 255 is always the transparent slot.
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

// The 16 fixed default slots, in index order. Every palette starts from these,
// so the engine and the menu screens can rely on them being present (the
// diagnostic HUD uses slot `PAL_LIGHT_GRAY`). Games add their own fixed colors
// on top with `Palette::add`.
pub const PAL_BLACK: u8 = 0;
pub const PAL_BLUE: u8 = 1;
pub const PAL_GREEN: u8 = 2;
pub const PAL_CYAN: u8 = 3;
pub const PAL_RED: u8 = 4;
pub const PAL_MAGENTA: u8 = 5;
pub const PAL_BROWN: u8 = 6;
pub const PAL_LIGHT_GRAY: u8 = 7;
pub const PAL_GRAY: u8 = 8;
pub const PAL_BRIGHT_BLUE: u8 = 9;
pub const PAL_BRIGHT_GREEN: u8 = 10;
pub const PAL_BRIGHT_CYAN: u8 = 11;
pub const PAL_BRIGHT_RED: u8 = 12;
pub const PAL_BRIGHT_MAGENTA: u8 = 13;
pub const PAL_YELLOW: u8 = 14;
pub const PAL_BRIGHT_WHITE: u8 = 15;

/// The classic 16-color palette every `Palette` starts from (the DOS/VGA
/// color set: black, dark blue/green/cyan/red/magenta/brown, light gray, then
/// the bright variants and bright white).
const DEFAULT_NAMED: [[u8; 3]; 16] = [
    [0, 0, 0],       // PAL_BLACK
    [0, 0, 128],     // PAL_BLUE
    [0, 128, 0],     // PAL_GREEN
    [0, 128, 128],   // PAL_CYAN
    [128, 0, 0],     // PAL_RED
    [128, 0, 128],   // PAL_MAGENTA
    [128, 128, 0],   // PAL_BROWN
    [192, 192, 192], // PAL_LIGHT_GRAY
    [128, 128, 128], // PAL_GRAY
    [0, 0, 255],     // PAL_BRIGHT_BLUE
    [0, 255, 0],     // PAL_BRIGHT_GREEN
    [0, 255, 255],   // PAL_BRIGHT_CYAN
    [255, 0, 0],     // PAL_BRIGHT_RED
    [255, 0, 255],   // PAL_BRIGHT_MAGENTA
    [255, 255, 0],   // PAL_YELLOW
    [255, 255, 255], // PAL_BRIGHT_WHITE
];

/// A 256-entry RGB palette. The framebuffer stores one 8-bit index per pixel;
/// the CPU renderer writes indices via `index_of` and the presentation shader
/// turns them back into RGB with a 256x1 palette texture.
///
/// A palette always starts from the 16 fixed default colors; `add` appends
/// extra colors (game-defined or discovered while loading images) at the first
/// free index. When all usable slots are taken, `add` falls back to the most
/// similar existing color, so loading never fails or overflows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Palette {
    /// Flat RGB entries, 3 bytes each in index order; ready for a 256x1 RGB8
    /// texture upload.
    rgb: [u8; PALETTE_SIZE * 3],
    /// Number of defined entries (`0..=PALETTE_SIZE - 1`). Index
    /// `PALETTE_SIZE - 1` (255) is reserved for `TRANSPARENT`.
    len: usize,
}

impl Palette {
    /// The RGB color for a palette index.
    #[inline]
    pub fn rgb(&self, index: u8) -> Color {
        let o = index as usize * 3;
        Color::rgb(self.rgb[o], self.rgb[o + 1], self.rgb[o + 2])
    }

    /// Flat RGB bytes (768), one palette entry every 3 bytes. Unused entries
    /// after `len` are zeros.
    #[inline]
    pub fn bytes(&self) -> &[u8] {
        &self.rgb
    }

    /// The number of defined color entries (always at least 16).
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// The default palette is never empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Reserve a color in the palette and return its index. Exact matches are
    /// reused; new colors take the first free index (16, 17, ...); once every
    /// usable slot is taken, the most similar existing color is returned, so
    /// the palette never overflows. Games add their fixed colors this way, and
    /// image loading does too (via `quantize_rgba`).
    pub fn add(&mut self, color: Color) -> u8 {
        if let Some(i) = self.find_exact(color) {
            return i as u8;
        }
        if self.len < PALETTE_SIZE - 1 {
            let i = self.len;
            let o = i * 3;
            self.rgb[o] = color.r;
            self.rgb[o + 1] = color.g;
            self.rgb[o + 2] = color.b;
            self.len += 1;
            return i as u8;
        }
        self.nearest_index(color)
    }

    /// Index of the palette entry for `color`: the exact entry when the color
    /// was added to this palette, otherwise the nearest entry. Used to store
    /// primitives as 8-bit indices at draw time.
    pub fn index_of(&self, color: Color) -> u8 {
        if let Some(i) = self.find_exact(color) {
            return i as u8;
        }
        self.nearest_index(color)
    }

    /// Convert one RGBA8 pixel to a palette index, adding the color to the
    /// palette if it is new (see [`Palette::add`]). Alpha is thresholded: a
    /// pixel that is at least 50% transparent (alpha <= 128) maps to
    /// `TRANSPARENT`; anything less transparent becomes fully opaque. Used to
    /// preprocess images once at load time, so drawing never has to look
    /// colors up per frame.
    pub fn quantize_rgba(&mut self, rgba: [u8; 4]) -> u8 {
        if rgba[3] <= 128 {
            TRANSPARENT
        } else {
            self.add(Color::rgb(rgba[0], rgba[1], rgba[2]))
        }
    }

    /// Exact-match index of `color` among the defined entries.
    fn find_exact(&self, color: Color) -> Option<usize> {
        for idx in 0..self.len {
            let o = idx * 3;
            if self.rgb[o] == color.r && self.rgb[o + 1] == color.g && self.rgb[o + 2] == color.b {
                return Some(idx);
            }
        }
        None
    }

    /// Index of the closest defined entry to `color` (sum of squared RGB
    /// deltas), scanning only the defined entries.
    fn nearest_index(&self, color: Color) -> u8 {
        let mut best = 0u8;
        let mut best_d = u32::MAX;
        for idx in 0..self.len {
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
    /// The classic 16-color palette: the fixed default entries at indices
    /// 0..16, with every further slot free for `add`.
    fn default() -> Palette {
        let mut rgb = [0u8; PALETTE_SIZE * 3];
        for (i, c) in DEFAULT_NAMED.iter().enumerate() {
            rgb[i * 3] = c[0];
            rgb[i * 3 + 1] = c[1];
            rgb[i * 3 + 2] = c[2];
        }
        Palette {
            rgb,
            len: DEFAULT_NAMED.len(),
        }
    }
}
