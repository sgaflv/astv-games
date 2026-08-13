//! The gibbon game's palette: the game's fixed colors, added on top of the
//! classic 16-color default. The warm theme diverges from the snake game.

use engine::color::{Color, Palette};

/// Frame background (also the letterbox blend color).
pub const BG: Color = Color::rgb(26, 20, 34);
/// The yellow wood floor tile.
pub const WOOD: Color = Color::rgb(214, 164, 72);
/// The wood tile's shading/outline.
pub const WOOD_DARK: Color = Color::rgb(142, 100, 40);
/// The wood tile's lit top edge.
pub const WOOD_TOP: Color = Color::rgb(244, 210, 138);
/// The red brick floor tile.
pub const BRICK: Color = Color::rgb(178, 66, 52);
/// The brick tile's shading/outline.
pub const BRICK_DARK: Color = Color::rgb(104, 34, 30);
/// The ladder's rungs.
pub const LADDER: Color = Color::rgb(168, 138, 84);
/// The ladder's side rails.
pub const LADDER_DARK: Color = Color::rgb(108, 86, 54);
/// The open pit left by a dug tile.
pub const HOLE: Color = Color::rgb(12, 9, 16);
/// The rim around an open pit.
pub const HOLE_EDGE: Color = Color::rgb(40, 31, 52);
/// The fruit.
pub const FRUIT: Color = Color::rgb(255, 96, 120);
/// The fruit's highlight.
pub const FRUIT_HI: Color = Color::rgb(255, 168, 182);
/// The fruit's stem.
pub const FRUIT_STEM: Color = Color::rgb(92, 178, 76);
/// The player gibbon's body.
pub const PLAYER_BODY: Color = Color::rgb(255, 168, 60);
/// The player's body shading.
pub const PLAYER_DARK: Color = Color::rgb(196, 116, 28);
/// The player's face.
pub const PLAYER_FACE: Color = Color::rgb(255, 224, 172);
/// The player's eye.
pub const PLAYER_EYE: Color = Color::rgb(20, 16, 28);
/// The guard's body.
pub const GUARD_BODY: Color = Color::rgb(210, 96, 128);
/// The guard's body shading.
pub const GUARD_DARK: Color = Color::rgb(148, 52, 84);
/// The guard's face.
pub const GUARD_FACE: Color = Color::rgb(238, 150, 158);
/// The guard's eye.
pub const GUARD_EYE: Color = Color::rgb(20, 16, 28);
/// HUD / overlay text.
pub const HUD: Color = Color::rgb(255, 224, 178);

/// The game palette: the 16 default colors plus every color the game draws,
/// so each one lands on an exact index.
pub fn palette() -> Palette {
    let mut p = Palette::default();
    for c in [
        BG,
        WOOD,
        WOOD_DARK,
        WOOD_TOP,
        BRICK,
        BRICK_DARK,
        LADDER,
        LADDER_DARK,
        HOLE,
        HOLE_EDGE,
        FRUIT,
        FRUIT_HI,
        FRUIT_STEM,
        PLAYER_BODY,
        PLAYER_DARK,
        PLAYER_FACE,
        PLAYER_EYE,
        GUARD_BODY,
        GUARD_DARK,
        GUARD_FACE,
        GUARD_EYE,
        HUD,
    ] {
        p.add(c);
    }
    p
}
