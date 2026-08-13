//! The snake game's palette: the game's fixed colors, added on top of the
//! classic 16-color default. Loading food sprites grows this palette further
//! with the sprite's colors.

use engine::color::{Color, Palette};

/// Frame background (also the letterbox blend color).
pub const BG: Color = Color::rgb(13, 13, 18);
/// The 1px grid lines between the cells.
pub const GRID: Color = Color::rgb(38, 38, 46);
/// Per-player snake colors.
pub const SNAKE_COLORS: [Color; 2] = [Color::rgb(51, 204, 51), Color::rgb(77, 148, 255)];
/// The 2x2 eye squares on the head.
pub const EYE: Color = Color::rgb(13, 13, 13);
/// The forked tongue.
pub const TONGUE: Color = Color::rgb(230, 77, 77);
/// HUD/pause text.
pub const HUD: Color = Color::rgb(204, 204, 214);

/// The game palette: the 16 default colors plus every color the game draws,
/// so each one lands on an exact index. `Playing` adds the food sprite colors
/// to this while loading.
pub fn palette() -> Palette {
    let mut p = Palette::default();
    for c in [BG, GRID, EYE, TONGUE, HUD, SNAKE_COLORS[0], SNAKE_COLORS[1]] {
        p.add(c);
    }
    p
}
