use crate::game::{self, Direction, Snake};
use crate::present::Presenter;
use crate::render::{Color, Framebuffer, Renderer};

use miniquad::{EventHandler, KeyCode, KeyMods};

use std::fmt::Write as _;
use std::time::Duration;

/// Fixed simulation timestep.
const SIM_STEP_SECONDS: f64 = 1.0 / game::SIM_STEP_HZ as f64;

/// Cap on a single frame delta. Prevents a large hiccup (debugger, OS sleep)
/// from running dozens of catch-up simulation steps.
const MAX_FRAME_TIME: f64 = 0.2;

/// Target presentation rate (60 FPS). The simulation stays at SIM_STEP_HZ.
const FRAME_TIME: f64 = 1.0 / 60.0;

/// How often the HUD text is refreshed (avoid per-frame text blits).
const HUD_REFRESH_EVERY: u32 = 30;

const HUD_COLOR: Color = Color::rgb(204, 204, 214);
const HUD_POS: (i32, i32) = (6, 6);
const HUD_LINE: i32 = 10;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Input {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Pause,
}

const INPUT_COUNT: usize = 7;

const fn input_index(input: Input) -> usize {
    match input {
        Input::Up => 0,
        Input::Down => 1,
        Input::Left => 2,
        Input::Right => 3,
        Input::Confirm => 4,
        Input::Back => 5,
        Input::Pause => 6,
    }
}

impl Input {
    fn from_keycode(key: KeyCode) -> Option<Input> {
        match key {
            KeyCode::Up | KeyCode::W => Some(Input::Up),
            KeyCode::Down | KeyCode::S => Some(Input::Down),
            KeyCode::Left | KeyCode::A => Some(Input::Left),
            KeyCode::Right | KeyCode::D => Some(Input::Right),
            KeyCode::Enter => Some(Input::Confirm),
            KeyCode::Escape | KeyCode::Back => Some(Input::Back),
            KeyCode::Space | KeyCode::Menu => Some(Input::Pause),
            _ => None,
        }
    }
}

pub struct Stage {
    game: Snake,
    framebuffer: Framebuffer,
    presenter: Presenter,

    // Timing.
    frame_start: f64,
    sim_accumulator: f64,
    frame_count: u32,
    fps_value: u32,
    fps_accum: f64,

    // Input (edge detection via held state; Android auto-repeat is ignored).
    held: [bool; INPUT_COUNT],
    pause_requested: bool,
    paused: bool,

    // HUD.
    hud_buffer: String,
    hud_dirty: bool,
    window_w: i32,
    window_h: i32,
}

impl Stage {
    pub fn new() -> Stage {
        let now = miniquad::date::now();
        Stage {
            game: Snake::new(),
            framebuffer: Framebuffer::new(),
            presenter: Presenter::new(),
            frame_start: now,
            sim_accumulator: 0.0,
            frame_count: 0,
            fps_value: 0,
            fps_accum: 0.0,
            held: [false; INPUT_COUNT],
            pause_requested: false,
            paused: false,
            hud_buffer: String::with_capacity(128),
            hud_dirty: true,
            window_w: 0,
            window_h: 0,
        }
    }

    fn refresh_hud(&mut self) {
        self.hud_buffer.clear();
        let _ = write!(
            self.hud_buffer,
            "FPS: {}  screen {}x{}  render {}x{}  scale {}",
            self.fps_value,
            self.window_w,
            self.window_h,
            crate::render::WIDTH,
            crate::render::HEIGHT,
            self.presenter.scale(),
        );
        self.hud_dirty = false;
    }

    fn render(&mut self) {
        let alpha = self.game.alpha();

        self.framebuffer.clear(game::bg_color());
        self.game.draw(&mut self.framebuffer, alpha);

        if self.hud_dirty {
            self.refresh_hud();
        }
        self.framebuffer
            .draw_text(HUD_POS.0, HUD_POS.1, 1, HUD_COLOR, &self.hud_buffer);

        if self.paused {
            self.framebuffer
                .draw_text(HUD_POS.0, HUD_POS.1 + HUD_LINE, 1, HUD_COLOR, "PAUSED");
        }

        self.presenter.present(&self.framebuffer);
    }
}

impl EventHandler for Stage {
    fn update(&mut self) {
        // Pace at 60 FPS: sleep until the start of the next frame slot. On a
        // v-synced 60 Hz display the swap in draw() already takes the full
        // frame budget and this sleep becomes a no-op.
        let now = miniquad::date::now();
        let slot_start = self.frame_start + FRAME_TIME;
        if now < slot_start {
            std::thread::sleep(Duration::from_secs_f64(slot_start - now));
        }
        let now = miniquad::date::now();
        let dt = (now - self.frame_start).min(MAX_FRAME_TIME);
        self.frame_start = now;

        if self.pause_requested {
            self.pause_requested = false;
            self.paused = !self.paused;
        }

        // Fixed-timestep simulation.
        if !self.paused {
            self.sim_accumulator += dt;
            let mut steps = 0;
            while self.sim_accumulator >= SIM_STEP_SECONDS && steps < 16 {
                self.game.step();
                self.sim_accumulator -= SIM_STEP_SECONDS;
                steps += 1;
            }
        }

        // HUD refresh + FPS smoothing.
        self.frame_count += 1;
        self.fps_accum += dt;
        if self.frame_count.is_multiple_of(HUD_REFRESH_EVERY) {
            self.fps_value = (30.0 / self.fps_accum.max(1e-9)).round() as u32;
            self.fps_accum = 0.0;
            self.hud_dirty = true;
        }
    }

    fn draw(&mut self) {
        self.render();
    }

    fn resize_event(&mut self, width: f32, height: f32) {
        self.window_w = width as i32;
        self.window_h = height as i32;
        self.presenter.resize(width, height);
        self.hud_dirty = true;
    }

    fn key_down_event(&mut self, keycode: KeyCode, _keymods: KeyMods, _repeat: bool) {
        let Some(input) = Input::from_keycode(keycode) else {
            return;
        };
        let idx = input_index(input);
        if self.held[idx] {
            return; // ignore Android key auto-repeat
        }
        self.held[idx] = true;
        match input {
            Input::Up => self.game.queue_direction(Direction::Up),
            Input::Down => self.game.queue_direction(Direction::Down),
            Input::Left => self.game.queue_direction(Direction::Left),
            Input::Right => self.game.queue_direction(Direction::Right),
            Input::Confirm => {}
            Input::Back => miniquad::window::request_quit(),
            Input::Pause => self.pause_requested = true,
        }
    }

    fn key_up_event(&mut self, keycode: KeyCode, _keymods: KeyMods) {
        if let Some(input) = Input::from_keycode(keycode) {
            self.held[input_index(input)] = false;
        }
    }

    fn window_minimized_event(&mut self) {
        // Lifecycle pause: stop simulating while the activity is backgrounded.
        self.paused = true;
    }

    fn window_restored_event(&mut self) {
        self.paused = false;
        let now = miniquad::date::now();
        self.frame_start = now;
        self.sim_accumulator = 0.0;
    }

    fn quit_requested_event(&mut self) {}
}
