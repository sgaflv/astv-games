//! The playing mode: a `Game` advanced by a fixed-timestep accumulator, plus
//! pause handling and the animated food sprites.

use crate::game::{Direction, Game, TARGET_FRAMES};
use engine::color::Color;
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
// horizontally in assets/apple_rotate.png.
const APPLE_SPRITE: &str = "apple_rotate.png";
const APPLE_FRAME_W: usize = 24;
const APPLE_FRAME_H: usize = 24;
const APPLE_FRAMES: usize = 12;

const PAUSED_POS: (i32, i32) = (6, 16);
const PAUSED_COLOR: Color = Color::rgb(204, 204, 214);

/// A running game: owns the `Game`, the food sprites, the simulation
/// accumulator and the pause state. Created by [`crate::menu::Menu`] with the
/// chosen player count.
pub struct Playing {
    game: Game,
    apple: Vec<RleSprite>,
    sim_accumulator: f64,
    paused: bool,
    pause_requested: bool,
}

impl Playing {
    /// Start a new game with `players` snakes.
    pub fn new(players: usize) -> Playing {
        let apple = SpriteSheet::load(APPLE_SPRITE, APPLE_FRAME_W, APPLE_FRAME_H, APPLE_FRAMES)
            .expect("embedded apple sprite sheet must load")
            .to_rle()
            .expect("apple frames must encode to RLE");
        Playing {
            game: Game::new(players),
            apple,
            sim_accumulator: 0.0,
            paused: false,
            pause_requested: false,
        }
    }
}

impl Scene for Playing {
    fn input(&mut self, player: usize, input: Input, down: bool) -> SceneAction {
        if !down {
            return SceneAction::Continue;
        }
        match input {
            Input::Up => self.game.queue_direction(player, Direction::Up),
            Input::Down => self.game.queue_direction(player, Direction::Down),
            Input::Left => self.game.queue_direction(player, Direction::Left),
            Input::Right => self.game.queue_direction(player, Direction::Right),
            Input::Pause => self.pause_requested = true,
            Input::Back => return SceneAction::Quit,
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

        // Face buttons are held-state only: A hides the tongue, B closes the
        // eyes.
        for (p, snake) in self.game.snakes.iter_mut().enumerate() {
            snake.tongue_hidden = input.held(p, Input::GameA);
            snake.eyes_closed = input.held(p, Input::GameB);
        }

        self.sim_accumulator += dt;

        let mut steps = 0;

        while self.sim_accumulator >= FRAME_TIME && steps < MAX_SIM_STEPS {
            self.game.step();
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
        self.game.draw(fb, self.game.frame_cnt, &self.apple);

        if self.paused {
            fb.draw_text(PAUSED_POS.0, PAUSED_POS.1, 1, PAUSED_COLOR, "PAUSED");
        }
    }

    fn suspend(&mut self) {
        self.paused = true;
    }

    fn resume(&mut self) {
        self.paused = false;
        self.sim_accumulator = 0.0;
    }
}
