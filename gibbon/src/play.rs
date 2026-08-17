//! The playing mode: a `Game` advanced by a fixed-timestep accumulator, plus
//! pause handling, the animated sprite sheets, the HUD and the level
//! clear / game over overlays.

use crate::game::{
    Action, CharacterSheets, Game, GameSprites, HANG_FRAMES, HANG_WALK_FRAMES, STAND_FRAMES, State,
    TARGET_FRAMES, WALK_FRAMES,
};
use crate::palette::{self, BG, HUD, PLAYER2_BODY, PLAYER2_DARK, PLAYER2_FACE};
use engine::color::{Color, Palette};
use engine::font;
use engine::input::{Input, InputState};
use engine::render::{Framebuffer, Renderer};
use engine::scene::{Scene, SceneAction};
use engine::sprites::{RleSprite, SpriteSheet};

/// Fixed simulation timestep: the game advances in fixed 1/60 s steps,
/// independently of the display refresh rate.
const FRAME_TIME: f64 = 1.0 / TARGET_FRAMES as f64;

/// Never run more than this many simulation steps per frame.
const MAX_SIM_STEPS: usize = 8;

// The fruit sprite sheet: 12 frames of 24x24 (one board cell) laid out
// horizontally in this crate's assets/apple_rotate.png.
const FRUIT_SPRITE: &str = "apple_rotate.png";
const FRAME_W: usize = 24;
const FRAME_H: usize = 24;
const FRUIT_FRAMES: usize = 12;

// The gibbon's art is split across three sheets: gibbon.png holds the
// standing look poses (neutral, looking right, looking left),
// gibbon_hang.png holds the same poses for hanging from a railing, and
// gibbon_move_right.png the walking animation facing right. The left-facing
// frames are the same sprites flipped horizontally at load time (see
// SpriteSheet::flipped_horizontal). Player two reuses the same art
// recolored green at load time (see `player2_color`).
const GIBBON_SPRITE: &str = "gibbon.png";
const HANG_SPRITE: &str = "gibbon_hang.png";
const HANG_WALK_SPRITE: &str = "gibbon_hang_right.png";
const WALK_SPRITE: &str = "gibbon_move_right.png";

// The tied pose: a single frame of the gibbon wrapped in rope, drawn in
// place while a guard has tied it up. Player two uses the same art recolored
// green (see `player2_color`).
const TIED_SPRITE: &str = "gibbon_tied.png";
const TIED_FRAMES: usize = 1;

// The guard's art follows the same split as the gibbon: guard.png holds only
// the standing pose and guard_move_right.png the walking animation facing
// right, flipped for the left-facing frames.
const GUARD_SPRITE: &str = "guard.png";
const GUARD_WALK_SPRITE: &str = "guard_move_right.png";

// The wood wall sheet: 12 frames of 24x24, the first the intact wall and the
// last the completely destroyed wood that stays in the dug cell.
const WOOD_SPRITE: &str = "wood.png";
const WOOD_FRAMES: usize = 12;

// The ladder sheet: one 24x24 frame drawn for every ladder rung.
const LADDER_SPRITE: &str = "ladder.png";
const LADDER_FRAMES: usize = 1;

// The stone wall sheet: one 24x24 frame drawn for every unbreakable brick
// tile.
const STONE_SPRITE: &str = "stone.png";
const STONE_FRAMES: usize = 1;

// The railing sheet: one 24x24 frame drawn for every railing tile.
const RAILING_SPRITE: &str = "railing.png";
const RAILING_FRAMES: usize = 1;

const PAUSED_POS: (i32, i32) = (6, 16);

/// The background theme: `assets/music.mid`, rendered to a looping WAV when
/// the game starts.
const MUSIC: &str = "music.mid";

/// A selected game: owns the sprite sheets and the palette up front (created
/// at selection time, so memory is spent only for the chosen game), then owns
/// the `Game` once the player count is confirmed, plus the simulation
/// accumulator and the pause state.
pub struct Playing {
    game: Option<Game>,
    fruit: Vec<RleSprite>,
    gibbon: CharacterSheets,
    gibbon2: CharacterSheets,
    guard: CharacterSheets,
    /// Player one's tied pose (gibbon wrapped in rope).
    tied: Vec<RleSprite>,
    /// Player two's tied pose, recolored green.
    tied2: Vec<RleSprite>,
    wood: Vec<RleSprite>,
    ladder: Vec<RleSprite>,
    stone: Vec<RleSprite>,
    railing: Vec<RleSprite>,
    /// The palette the sprites were quantized against; also the scene's
    /// palette, so framebuffer indices match the loaded sprites.
    palette: Palette,
    /// The rendered theme WAV, kept so playback can restart after a pause or
    /// when the window focus returns, without re-rendering.
    music: Option<Vec<u8>>,
    sim_accumulator: f64,
    paused: bool,
    pause_requested: bool,
}

/// The green recolor of the player gibbon for player two: each opaque pixel
/// keeps its shading but shifts toward the green player-two palette, so the
/// same art covers both players.
pub(crate) fn player2_color(c: Color) -> Color {
    // Relative luminance: bright pixels become the face color, mid tones the
    // body and dark pixels the shading. Near-black pixels (the eye) are left
    // alone.
    let l = c.r as u32 * 299 + c.g as u32 * 587 + c.b as u32 * 114;
    if l < 10_000 {
        c
    } else if l > 200 * 1000 {
        PLAYER2_FACE
    } else if l > 130 * 1000 {
        PLAYER2_BODY
    } else {
        PLAYER2_DARK
    }
}

impl Playing {
    /// Decode one sprite sheet against the game palette, adding the sprites'
    /// colors to it; `flipped` mirrors every frame horizontally first, so a
    /// right-facing animation can be reused for the opposite direction.
    fn load_sheet(palette: &mut Palette, name: &str, frames: usize, flipped: bool) -> SpriteSheet {
        let data = crate::assets::load(name).expect(&format!(
            "embedded sprite sheet must exist, error loading {}",
            name
        ));
        let sheet = SpriteSheet::from_png(data, palette, FRAME_W, FRAME_H, frames)
            .expect("embedded sprite sheet must load");
        if flipped {
            sheet.flipped_horizontal()
        } else {
            sheet
        }
    }

    /// Decode one sprite sheet to RLE against the game palette.
    fn load_sprites(palette: &mut Palette, name: &str, frames: usize) -> Vec<RleSprite> {
        Self::load_sheet(palette, name, frames, false)
            .to_rle()
            .expect("sprite frames must encode to RLE")
    }

    /// Build a gibbon's sheets from its standing sprite (gibbon.png), the
    /// hanging sprite (gibbon_hang.png) and the shared walking animation
    /// (gibbon_move_right.png), flipped for the left-facing frames. Player two
    /// recolors the same art green via [`player2_color`] instead of loading a
    /// separate sheet.
    fn load_gibbon(palette: &mut Palette, recolor: Option<fn(Color) -> Color>) -> CharacterSheets {
        let mut decode = |name: &str, frames: usize, flip: bool| -> Vec<RleSprite> {
            let sheet = Self::load_sheet(palette, name, frames, flip);
            match recolor {
                Some(mapping) => sheet
                    .recolored(palette, mapping)
                    .to_rle()
                    .expect("sprite frames must encode to RLE"),
                None => sheet.to_rle().expect("sprite frames must encode to RLE"),
            }
        };
        CharacterSheets {
            stand: decode(GIBBON_SPRITE, STAND_FRAMES, false),
            stand_left: decode(GIBBON_SPRITE, STAND_FRAMES, true),
            walk_right: decode(WALK_SPRITE, WALK_FRAMES, false),
            walk_left: decode(WALK_SPRITE, WALK_FRAMES, true),
            climb: Vec::new(),
            hang: decode(HANG_SPRITE, HANG_FRAMES, false),
            hang_left: decode(HANG_SPRITE, HANG_FRAMES, true),
            hang_walk_right: decode(HANG_WALK_SPRITE, HANG_WALK_FRAMES, false),
            hang_walk_left: decode(HANG_WALK_SPRITE, HANG_WALK_FRAMES, true),
        }
    }

    /// Build the guard's sheets: the standing pose from guard.png, flipped
    /// for the left-facing copy, and the walking animation from
    /// guard_move_right.png, also flipped for the left-facing frames. The
    /// guard has no climbing art, so climbing falls back to the standing pose
    /// (like the gibbon). Every guard in the game shares these sheets.
    fn load_guard(palette: &mut Palette, name: &str) -> CharacterSheets {
        let stand = Self::load_sheet(palette, name, STAND_FRAMES, false)
            .to_rle()
            .expect("sprite frames must encode to RLE");
        let stand_left = Self::load_sheet(palette, name, STAND_FRAMES, true)
            .to_rle()
            .expect("sprite frames must encode to RLE");
        let walk_right = Self::load_sheet(palette, GUARD_WALK_SPRITE, WALK_FRAMES, false)
            .to_rle()
            .expect("sprite frames must encode to RLE");
        let walk_left = Self::load_sheet(palette, GUARD_WALK_SPRITE, WALK_FRAMES, true)
            .to_rle()
            .expect("sprite frames must encode to RLE");
        CharacterSheets {
            stand,
            stand_left,
            walk_right,
            walk_left,
            climb: Vec::new(),
            hang: Vec::new(),
            hang_left: Vec::new(),
            hang_walk_right: Vec::new(),
            hang_walk_left: Vec::new(),
        }
    }

    /// Create the game instance: build the game palette (the 16 default colors
    /// plus the game's fixed colors), then decode the sprite sheets against it,
    /// which adds the sprites' colors to the palette. Called when the game is
    /// selected, before the player count is known; [`Playing::start`] creates
    /// the `Game` when the menu confirms.
    pub fn new() -> Playing {
        let mut palette = palette::palette();
        let fruit = Self::load_sprites(&mut palette, FRUIT_SPRITE, FRUIT_FRAMES);
        let gibbon = Self::load_gibbon(&mut palette, None);
        let gibbon2 = Self::load_gibbon(&mut palette, Some(player2_color));
        let guard = Self::load_guard(&mut palette, GUARD_SPRITE);
        let tied = Self::load_sprites(&mut palette, TIED_SPRITE, TIED_FRAMES);
        let tied2 = Self::load_sheet(&mut palette, TIED_SPRITE, TIED_FRAMES, false)
            .recolored(&palette, player2_color)
            .to_rle()
            .expect("tied frame must encode to RLE");
        let wood = Self::load_sprites(&mut palette, WOOD_SPRITE, WOOD_FRAMES);
        let ladder = Self::load_sprites(&mut palette, LADDER_SPRITE, LADDER_FRAMES);
        let stone = Self::load_sprites(&mut palette, STONE_SPRITE, STONE_FRAMES);
        let railing = Self::load_sprites(&mut palette, RAILING_SPRITE, RAILING_FRAMES);
        // Render the theme once and start it looping; the WAV is kept so
        // playback can restart after a pause or when focus returns.
        let music = music_wav();
        if let Some(wav) = &music {
            engine::audio::play_loop(wav);
        }
        Playing {
            game: None,
            fruit,
            gibbon,
            gibbon2,
            guard,
            tied,
            tied2,
            wood,
            ladder,
            stone,
            railing,
            palette,
            music,
            sim_accumulator: 0.0,
            paused: false,
            pause_requested: false,
        }
    }

    /// Create the game. Called once by the player count menu right before
    /// this scene becomes active; the player count picks one or two
    /// cooperative gibbons.
    pub fn start(&mut self, players: usize) {
        self.game = Some(Game::new(players));
    }
}

impl Default for Playing {
    fn default() -> Playing {
        Playing::new()
    }
}

/// Render `assets/music.mid` to a WAV. Returns `None` when the asset is
/// missing or cannot be rendered, so the game runs fine without it.
fn music_wav() -> Option<Vec<u8>> {
    crate::assets::load(MUSIC).and_then(engine::midi::render_wav)
}

impl Playing {
    /// Start the looping theme, if one was rendered.
    fn music_play(&self) {
        if let Some(wav) = &self.music {
            engine::audio::play_loop(wav);
        }
    }

    /// Stop the looping theme.
    fn music_stop(&self) {
        engine::audio::stop();
    }
}

impl Drop for Playing {
    fn drop(&mut self) {
        self.music_stop();
    }
}

impl Scene for Playing {
    fn input(&mut self, _player: usize, input: Input, down: bool) -> SceneAction {
        if !down {
            return SceneAction::Continue;
        }
        let game = self.game.as_mut().expect("game started before it ran");
        match input {
            Input::Pause => self.pause_requested = true,
            Input::Back => return SceneAction::PopToRoot,
            // Restart from level one when the run is over; irrelevant while
            // playing, clearing or dead.
            Input::Confirm => {
                if matches!(game.game_state, State::GameOver | State::Win) {
                    game.restart();
                }
            }
            // Directions are sampled as held state during `update`. Stick
            // directions are deliberately not game directions: D-pad is the
            // primary control, stick movement would be wired in explicitly.
            Input::Up | Input::Down | Input::Left | Input::Right => {}
            Input::GameA
            | Input::GameB
            | Input::GameX
            | Input::GameY
            | Input::StickUp
            | Input::StickDown
            | Input::StickLeft
            | Input::StickRight => {}
        }
        SceneAction::Continue
    }

    fn update(&mut self, dt: f64, input: &InputState) -> SceneAction {
        if self.pause_requested {
            self.pause_requested = false;
            self.paused = !self.paused;
            if self.paused {
                self.music_stop();
            } else {
                self.music_play();
            }
        }

        if self.paused {
            return SceneAction::Continue;
        }

        let game = self.game.as_mut().expect("game started before it ran");

        // Pressing a direction sets a gibbon's latched movement direction: it
        // keeps walking, climbing or hanging in that direction even after the
        // key is released, until the way is blocked. Releasing all keys does
        // not stop it, so the game is only told about new directions. Player
        // one uses the arrow keys / WASD, player two the IJKL cluster.
        let up = input.held(0, Input::Up);
        let down = input.held(0, Input::Down);
        let left = input.held(0, Input::Left);
        let right = input.held(0, Input::Right);
        let a = input.held(0, Input::GameA);
        let b = input.held(0, Input::GameB);

        let up2 = input.held(1, Input::Up);
        let down2 = input.held(1, Input::Down);
        let left2 = input.held(1, Input::Left);
        let right2 = input.held(1, Input::Right);
        let a2 = input.held(1, Input::GameA);
        let b2 = input.held(1, Input::GameB);

        if up && !down {
            game.set_action(Some(Action::Up));
        } else if down && !up {
            game.set_action(Some(Action::Down));
        } else if left && !right {
            game.set_action(Some(Action::Left));
        } else if right && !left {
            game.set_action(Some(Action::Right));
        } else if a {
            game.set_action(Some(Action::DigRight));
        } else if b {
            game.set_action(Some(Action::DigLeft));
        }

        if up2 && !down2 {
            game.set_action2(Some(Action::Up));
        } else if down2 && !up2 {
            game.set_action2(Some(Action::Down));
        } else if left2 && !right2 {
            game.set_action2(Some(Action::Left));
        } else if right2 && !left2 {
            game.set_action2(Some(Action::Right));
        } else if a2 {
            game.set_action2(Some(Action::DigRight));
        } else if b2 {
            game.set_action2(Some(Action::DigLeft));
        }

        self.sim_accumulator += dt;

        let mut steps = 0;

        while self.sim_accumulator >= FRAME_TIME && steps < MAX_SIM_STEPS {
            game.step();
            self.sim_accumulator -= FRAME_TIME;
            steps += 1;
        }

        // Avoid carrying a huge backlog forever.
        if steps == MAX_SIM_STEPS {
            self.sim_accumulator = 0.0;
        }

        SceneAction::Continue
    }

    fn draw(&mut self, fb: &mut Framebuffer) {
        fb.clear(BG);
        let game = self.game.as_mut().expect("game started before it ran");
        let sprites = GameSprites {
            fruit: &self.fruit,
            gibbon: self.gibbon.sprites(),
            gibbon2: self.gibbon2.sprites(),
            guard: self.guard.sprites(),
            tied: self.tied.first().expect("a tied frame is always loaded"),
            tied2: self.tied2.first().expect("a tied frame is always loaded"),
            wood: &self.wood,
            ladder: &self.ladder,
            stone: &self.stone,
            railing: &self.railing,
        };
        game.draw(fb, game.frame_cnt, &sprites);

        draw_hud(fb, game);

        if self.paused {
            fb.draw_text(PAUSED_POS.0, PAUSED_POS.1, 1, HUD, "PAUSED");
        }

        // Terminal and transitional overlays, drawn centered over the board.
        match game.game_state {
            State::Cleared => overlay(fb, "LEVEL CLEAR", 128),
            State::Dead => overlay(fb, "GOTCHA", 128),
            State::GameOver => {
                overlay(fb, "GAME OVER", 116);
                overlay(fb, "PRESS OK TO RESTART", 146);
            }
            State::Win => {
                overlay(fb, "YOU WIN", 116);
                overlay(fb, "PRESS OK TO RESTART", 146);
            }
            State::Playing => {}
        }
    }

    fn palette(&self) -> Palette {
        self.palette
    }

    fn clear_color(&self) -> engine::color::Color {
        BG
    }

    fn suspend(&mut self) {
        self.paused = true;
        self.music_stop();
    }

    fn resume(&mut self) {
        self.paused = false;
        self.sim_accumulator = 0.0;
        self.music_play();
    }
}

/// Draw the level / fruit / lives line in the top HUD band, right-aligned so
/// it never collides with the engine's diagnostics on the left.
fn draw_hud(fb: &mut Framebuffer, game: &Game) {
    let text = format!(
        "LVL {}  FRUITS {}  LIVES {}",
        game.level_index + 1,
        game.fruits_left,
        game.lives
    );
    let width = font::text_width(&text, 1);
    fb.draw_text(480 - 6 - width, 6, 1, HUD, &text);
}

/// Center `text` horizontally and draw it at `y` (top-left of the line).
fn overlay(fb: &mut Framebuffer, text: &str, y: i32) {
    let width = font::text_width(text, 2);
    fb.draw_text((480 - width) / 2, y, 2, HUD, text);
}
