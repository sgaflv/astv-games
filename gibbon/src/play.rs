//! The playing mode: a `Game` advanced by a fixed-timestep accumulator, plus
//! pause handling and the animated food sprites.

use crate::game::{Direction, Game, TARGET_FRAMES};
use crate::palette::{self, BG, HUD};
use engine::color::Palette;
use engine::input::{Input, InputState};
use engine::render::{Framebuffer, Renderer};
use engine::scene::{Scene, SceneAction};
use engine::sprites::{RleSprite, SpriteSheet};

/// Fixed simulation timestep: the game advances in fixed 1/60 s steps,
/// independently of the display refresh rate.
const FRAME_TIME: f64 = 1.0 / TARGET_FRAMES as f64;

/// Never run more than this many simulation steps per frame.
const MAX_SIM_STEPS: usize = 8;

// The apple food sprite sheet: 12 frames of 24x24 (one board cell) laid out
// horizontally in this crate's assets/apple_rotate.png.
const APPLE_SPRITE: &str = "apple_rotate.png";
const APPLE_FRAME_W: usize = 24;
const APPLE_FRAME_H: usize = 24;
const APPLE_FRAMES: usize = 12;

const PAUSED_POS: (i32, i32) = (6, 16);

/// A selected game: owns the food sprites and the palette up front (created at
/// selection time, so memory is spent only for the chosen game), then owns the
/// `Game` once the player count is confirmed, plus the simulation accumulator
/// and the pause state.
pub struct Playing {
    game: Option<Game>,
    apple: Vec<RleSprite>,
    /// The palette the food sprites were quantized against; also the scene's
    /// palette, so framebuffer indices match the loaded sprites.
    palette: Palette,
    sim_accumulator: f64,
    paused: bool,
    pause_requested: bool,
}

impl Playing {
    /// Create the game instance: build the game palette (the 16 default colors
    /// plus the game's fixed colors), then decode the food sprites against it,
    /// which adds the sprite's colors to the palette. Called when the game is
    /// selected, before the player count is known; [`Playing::start`] sets the
    /// player count when the menu confirms.
    pub fn new() -> Playing {
        let mut palette = palette::palette();
        let data = crate::assets::load(APPLE_SPRITE).expect("apple_rotate.png is embedded");
        let apple = SpriteSheet::from_png(
            data,
            &mut palette,
            APPLE_FRAME_W,
            APPLE_FRAME_H,
            APPLE_FRAMES,
        )
        .expect("embedded apple sprite sheet must load")
        .to_rle()
        .expect("apple frames must encode to RLE");
        Playing {
            game: None,
            apple,
            palette,
            sim_accumulator: 0.0,
            paused: false,
            pause_requested: false,
        }
    }

    /// Set the player count and spawn the snakes. Called once by the player
    /// count menu right before this scene becomes active.
    pub fn start(&mut self, players: usize) {
        self.game = Some(Game::new(players));
    }
}

impl Default for Playing {
    fn default() -> Playing {
        Playing::new()
    }
}

impl Scene for Playing {
    fn input(&mut self, player: usize, input: Input, down: bool) -> SceneAction {
        if !down {
            return SceneAction::Continue;
        }
        let game = self.game.as_mut().expect("game started before it ran");
        match input {
            Input::Up => game.queue_direction(player, Direction::Up),
            Input::Down => game.queue_direction(player, Direction::Down),
            Input::Left => game.queue_direction(player, Direction::Left),
            Input::Right => game.queue_direction(player, Direction::Right),
            Input::Pause => self.pause_requested = true,
            Input::Back => return SceneAction::PopToRoot,
            // Face buttons are sampled as held state during `update`.
            Input::Confirm | Input::GameA | Input::GameB | Input::GameX | Input::GameY => {}
        }
        SceneAction::Continue
    }

    fn update(&mut self, dt: f64, input: &InputState) -> SceneAction {
        if self.pause_requested {
            self.pause_requested = false;
            self.paused = !self.paused;
        }

        if self.paused {
            return SceneAction::Continue;
        }

        let game = self.game.as_mut().expect("game started before it ran");
        // Face buttons are held-state only: A hides the tongue, B closes the
        // eyes.
        for (p, snake) in game.snakes.iter_mut().enumerate() {
            snake.tongue_hidden = input.held(p, Input::GameA);
            snake.eyes_closed = input.held(p, Input::GameB);
        }

        self.sim_accumulator += dt;

        let mut steps = 0;

        while self.sim_accumulator >= FRAME_TIME && steps < MAX_SIM_STEPS {
            game.step();
            self.sim_accumulator -= FRAME_TIME;
            steps += 1;
        }

        // Optional: avoid carrying a huge backlog forever.
        if steps == MAX_SIM_STEPS {
            self.sim_accumulator = 0.0;
        }

        SceneAction::Continue
    }

    fn draw(&mut self, fb: &mut Framebuffer) {
        fb.clear(BG);
        let apple = &self.apple;
        let game = self.game.as_mut().expect("game started before it ran");
        game.draw(fb, game.frame_cnt, apple);

        if self.paused {
            fb.draw_text(PAUSED_POS.0, PAUSED_POS.1, 1, HUD, "PAUSED");
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
    }

    fn resume(&mut self) {
        self.paused = false;
        self.sim_accumulator = 0.0;
    }
}
