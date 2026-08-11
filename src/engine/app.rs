use crate::engine::font;
use crate::engine::present::Presenter;
use crate::engine::render::{Color, Framebuffer, Renderer};
use crate::engine::start::{Start, State};
use crate::game::{self, Direction, PLAYERS};

use miniquad::{EventHandler, KeyCode, KeyMods};

use std::fmt::Write as _;
use std::time::Duration;

/// Fixed simulation timestep.
const SIM_STEP_SECONDS: f64 = 1.0 / game::SIM_STEP_HZ as f64;

/// Cap on a single frame delta. Prevents a large hiccup (debugger, OS sleep)
/// from running dozens of catch-up simulation steps.
const MAX_FRAME_TIME: f64 = 0.2;

/// Target presentation rate in frames per second. This is the FPS cap:
/// change this value to cap the game at a different frame rate.
/// The simulation always advances at SIM_STEP_HZ (fixed timestep).
const TARGET_FPS: u32 = 60;

/// Target presentation rate derived from TARGET_FPS.
const FRAME_TIME: f64 = 1.0 / TARGET_FPS as f64;

/// How often the HUD text is refreshed (avoid per-frame text blits).
const HUD_REFRESH_EVERY: u32 = 30;
const HUD_COLOR: Color = Color::rgb(204, 204, 214);
const HUD_POS: (i32, i32) = (6, 6);
const HUD_LINE: i32 = 10;

// Menu (player-count selection) constants.
const MENU_TITLE: &str = "SNAKE";
const MENU_TITLE_SCALE: i32 = 3;
const MENU_TITLE_Y: i32 = 54;
const MENU_OPTIONS: [&str; 2] = ["1 PLAYER", "2 PLAYERS"];
const MENU_OPTION_SCALE: i32 = 2;
const MENU_OPTION_Y: i32 = 150;
const MENU_OPTION_LINE: i32 = 36;
const MENU_DIM_COLOR: Color = Color::rgb(120, 120, 130);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Input {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Pause,
    // Gamepad face buttons. On Android, gamepads bypass miniquad's key path
    // entirely (device-aware `surfaceOnPlayerKey`, see input.rs); for other
    // devices the Java glue remaps KEYCODE_BUTTON_* to F1-F4 because miniquad
    // 0.4.11 cannot tell them apart. F1-F8 on a desktop keyboard trigger the
    // same inputs for easy testing.
    GameA,
    GameB,
    GameX,
    GameY,
}

const INPUT_COUNT: usize = 11;

const fn input_index(input: Input) -> usize {
    match input {
        Input::Up => 0,
        Input::Down => 1,
        Input::Left => 2,
        Input::Right => 3,
        Input::Confirm => 4,
        Input::Back => 5,
        Input::Pause => 6,
        Input::GameA => 7,
        Input::GameB => 8,
        Input::GameX => 9,
        Input::GameY => 10,
    }
}

impl Input {
    /// Map a desktop key to a (player, input) pair. Player 1 uses arrows/WASD
    /// and F1-F4, player 2 uses IJKL and F5-F8. Global actions (pause/back)
    /// use player 0; their player index is irrelevant.
    fn from_keycode(key: KeyCode) -> Option<(usize, Input)> {
        use KeyCode::*;
        let input = match key {
            Up | W => Input::Up,
            Down | S => Input::Down,
            Left | A => Input::Left,
            Right | D => Input::Right,
            I => Input::Up,
            K => Input::Down,
            J => Input::Left,
            L => Input::Right,
            Enter => Input::Confirm,
            Escape | Back => Input::Back,
            Space | Menu => Input::Pause,
            F1 => Input::GameA,
            F2 => Input::GameB,
            F3 => Input::GameX,
            F4 => Input::GameY,
            F5 => Input::GameA,
            F6 => Input::GameB,
            F7 => Input::GameX,
            F8 => Input::GameY,
            _ => return None,
        };
        let player = match key {
            I | K | J | L | F5 | F6 | F7 | F8 => 1,
            _ => 0,
        };
        Some((player, input))
    }
}

/// Raw Android keycode -> game input. Used by the device-aware gamepad path
/// (`surfaceOnPlayerKey`), which bypasses miniquad's keycode translation.
/// Values are the android.view.KeyEvent constants.
#[cfg(target_os = "android")]
fn android_keycode_to_input(keycode: i32) -> Option<Input> {
    match keycode {
        19 => Some(Input::Up),      // KEYCODE_DPAD_UP
        20 => Some(Input::Down),    // KEYCODE_DPAD_DOWN
        21 => Some(Input::Left),    // KEYCODE_DPAD_LEFT
        22 => Some(Input::Right),   // KEYCODE_DPAD_RIGHT
        66 => Some(Input::Confirm), // KEYCODE_ENTER
        23 => Some(Input::Confirm), // KEYCODE_DPAD_CENTER (OK; gamepad A often sends this)
        4 => Some(Input::Back),     // KEYCODE_BACK
        111 => Some(Input::Back),   // KEYCODE_ESCAPE
        82 => Some(Input::Pause),   // KEYCODE_MENU
        62 => Some(Input::Pause),   // KEYCODE_SPACE
        96 => Some(Input::GameA),   // KEYCODE_BUTTON_A
        97 => Some(Input::GameB),   // KEYCODE_BUTTON_B
        99 => Some(Input::GameX),   // KEYCODE_BUTTON_X
        100 => Some(Input::GameY),  // KEYCODE_BUTTON_Y
        _ => None,
    }
}

pub struct Stage {
    start: Start,
    framebuffer: Framebuffer,
    presenter: Presenter,

    // Timing.
    frame_start: f64,
    frame_count: u32,
    fps_value: u32,
    fps_accum: f64,

    // Per-second render timing stats.
    render_time_accum: f64,
    render_time_frames: u32,
    render_us: f64,
    stat_accum: f64,

    // Input (edge detection via per-player held state; Android auto-repeat is
    // ignored). Gamepad device assignment happens in the Java glue: the first
    // gamepad is player 0, the second player 1.
    held: [[bool; INPUT_COUNT]; PLAYERS],
    // The physical key currently holding each logical input, per player. Used
    // to ignore OS/Android auto-repeat (the same key re-sent while held)
    // without swallowing a *different* key that maps to the same input (e.g.
    // arrow-Up while W is held, or a second controller sharing the slot).
    held_keys: [[Option<u32>; INPUT_COUNT]; PLAYERS],

    // Player-count menu.
    selection: usize,

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
            start: Start::new(),
            framebuffer: Framebuffer::new(),
            presenter: Presenter::new(),
            frame_start: now,
            frame_count: 0,
            fps_value: 0,
            fps_accum: 0.0,
            render_time_accum: 0.0,
            render_time_frames: 0,
            render_us: 0.0,
            stat_accum: 0.0,
            held: [[false; INPUT_COUNT]; PLAYERS],
            held_keys: [[None; INPUT_COUNT]; PLAYERS],
            selection: 0,
            hud_buffer: String::with_capacity(128),
            hud_dirty: true,
            window_w: 0,
            window_h: 0,
        }
    }

    /// Route one input edge (down or up) from the physical `key` (a platform
    /// keycode used only to tell distinct keys apart). Key-down edges are
    /// dispatched to the active state; key-up only clears the held state.
    /// Auto-repeat of the *same* held key is ignored, but a different key that
    /// maps to the same logical input is still delivered, so one player's held
    /// key never blocks another player's (or another key's) press.
    fn apply_input(&mut self, player: usize, key: u32, input: Input, down: bool) {
        let idx = input_index(input);
        if down {
            if self.held_keys[player][idx] == Some(key) {
                return; // OS/Android auto-repeat of an already-held key
            }
            self.held_keys[player][idx] = Some(key);
            self.held[player][idx] = true;
            match self.start.state {
                State::Menu => self.menu_input(input),
                State::Playing => self.game_input(player, input),
            }
        } else if self.held_keys[player][idx] == Some(key) {
            self.held_keys[player][idx] = None;
            self.held[player][idx] = false;
        }
    }

    /// Menu navigation: direction keys cycle the selection, confirm starts.
    fn menu_input(&mut self, input: Input) {
        match input {
            Input::Up | Input::Down | Input::Left | Input::Right => {
                self.selection = 1 - self.selection
            }
            Input::Confirm | Input::GameA | Input::GameB => self.start.begin(self.selection + 1),
            Input::Back => miniquad::window::request_quit(),
            Input::Pause | Input::GameX | Input::GameY => {}
        }
    }

    /// In-game input: direction queues a turn on the player's snake, face
    /// buttons are held-state only.
    fn game_input(&mut self, player: usize, input: Input) {
        match input {
            Input::Up => self.start.game.queue_direction(player, Direction::Up),
            Input::Down => self.start.game.queue_direction(player, Direction::Down),
            Input::Left => self.start.game.queue_direction(player, Direction::Left),
            Input::Right => self.start.game.queue_direction(player, Direction::Right),
            Input::Confirm => {}
            Input::Back => miniquad::window::request_quit(),
            Input::Pause => self.start.pause_requested = true,
            // Gamepad face buttons are held-state only for now.
            Input::GameA | Input::GameB | Input::GameX | Input::GameY => {}
        }
    }

    /// Drain device-aware gamepad events (Android). No-op on desktop.
    #[cfg(target_os = "android")]
    fn drain_player_input(&mut self) {
        crate::input::drain_into(|event| {
            if let Some(input) = android_keycode_to_input(event.keycode) {
                self.apply_input(event.player, event.keycode as u32, input, event.down);
            }
        });
    }

    fn refresh_hud(&mut self) {
        self.hud_buffer.clear();
        let _ = write!(
            self.hud_buffer,
            "FPS: {}  render {:.1} us  screen {}x{}  scale {}",
            self.fps_value,
            self.render_us,
            self.window_w,
            self.window_h,
            self.presenter.scale(),
        );
        self.hud_dirty = false;
    }

    fn render(&mut self) {
        // start time measure
        let t0 = miniquad::date::now();

        self.framebuffer.zero();

        match self.start.state {
            State::Menu => self.draw_menu(),
            State::Playing => {
                let alpha = self.start.game.alpha();
                self.start.game.draw(&mut self.framebuffer, alpha);

                if self.hud_dirty {
                    self.refresh_hud();
                }

                self.framebuffer
                    .draw_text(HUD_POS.0, HUD_POS.1, 1, HUD_COLOR, &self.hud_buffer);

                if self.start.paused {
                    self.framebuffer.draw_text(
                        HUD_POS.0,
                        HUD_POS.1 + HUD_LINE,
                        1,
                        HUD_COLOR,
                        "PAUSED",
                    );
                }
            }
        }

        // end time measure
        let t1 = miniquad::date::now();
        self.render_time_accum += t1 - t0;
        self.render_time_frames += 1;

        self.presenter.present(&self.framebuffer);
    }

    /// Draw the player-count selection menu. The selected option gets a '>'
    /// cursor and a brighter color.
    fn draw_menu(&mut self) {
        let w = self.framebuffer.width() as i32;

        let tx = (w - font::text_width(MENU_TITLE, MENU_TITLE_SCALE)) / 2;
        self.framebuffer
            .draw_text(tx, MENU_TITLE_Y, MENU_TITLE_SCALE, HUD_COLOR, MENU_TITLE);

        for (i, option) in MENU_OPTIONS.iter().enumerate() {
            let selected = i == self.selection;
            let y = MENU_OPTION_Y + i as i32 * MENU_OPTION_LINE;
            // Center the option text; the cursor column sits just to its left.
            let x = (w - font::text_width(option, MENU_OPTION_SCALE)) / 2;
            let color = if selected { HUD_COLOR } else { MENU_DIM_COLOR };
            if selected {
                self.framebuffer
                    .draw_text(x - 40, y, MENU_OPTION_SCALE, HUD_COLOR, ">");
            }
            self.framebuffer
                .draw_text(x, y, MENU_OPTION_SCALE, color, option);
        }
    }
}

impl Default for Stage {
    fn default() -> Stage {
        Stage::new()
    }
}

impl EventHandler for Stage {
    fn update(&mut self) {
        // Pace at TARGET_FPS: sleep until the start of the next frame slot.
        // On a v-synced 60 Hz display the swap in draw() already takes the
        // full frame budget and this sleep becomes a no-op.
        let now = miniquad::date::now();
        let slot_start = self.frame_start + FRAME_TIME;
        if now < slot_start {
            std::thread::sleep(Duration::from_secs_f64(slot_start - now));
        }
        let now = miniquad::date::now();
        let dt = (now - self.frame_start).min(MAX_FRAME_TIME);
        self.frame_start = now;

        if self.start.pause_requested {
            self.start.pause_requested = false;
            self.start.paused = !self.start.paused;
        }

        // Device-aware gamepad input (Android); the Java glue assigned each
        // gamepad a player slot. No-op on desktop.
        #[cfg(target_os = "android")]
        self.drain_player_input();

        // Fixed-timestep simulation.
        if !self.start.paused && self.start.state == State::Playing {
            // Placeholder gamepad face-button actions (while held). A hides
            // the tongue, B closes the eyes; X/Y are reserved for later use.
            for (p, snake) in self.start.game.snakes.iter_mut().enumerate() {
                snake.tongue_hidden = self.held[p][input_index(Input::GameA)];
                snake.eyes_closed = self.held[p][input_index(Input::GameB)];
            }

            self.start.sim_accumulator += dt;
            let mut steps = 0;
            while self.start.sim_accumulator >= SIM_STEP_SECONDS && steps < 16 {
                self.start.game.step();
                self.start.sim_accumulator -= SIM_STEP_SECONDS;
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

        // Average render time over the last second, stat updated once a second.
        self.stat_accum += dt;
        if self.stat_accum >= 1.0 {
            self.render_us =
                self.render_time_accum / self.render_time_frames.max(1) as f64 * 1_000_000.0;
            self.render_time_accum = 0.0;
            self.render_time_frames = 0;
            self.stat_accum -= 1.0;
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
        if let Some((player, input)) = Input::from_keycode(keycode) {
            self.apply_input(player, keycode as u32, input, true);
        }
    }

    fn key_up_event(&mut self, keycode: KeyCode, _keymods: KeyMods) {
        if let Some((player, input)) = Input::from_keycode(keycode) {
            self.apply_input(player, keycode as u32, input, false);
        }
    }

    fn window_minimized_event(&mut self) {
        // Lifecycle pause: stop simulating while the activity is backgrounded.
        self.start.paused = true;
    }

    fn window_restored_event(&mut self) {
        self.start.paused = false;
        let now = miniquad::date::now();
        self.frame_start = now;
        self.start.sim_accumulator = 0.0;
    }

    fn quit_requested_event(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Player 0 (arrows/WASD/F1-F4) and player 1 (IJKL/F5-F8) must never be
    /// routed to the same slot: this is what keeps two keyboard players'
    /// controls independent on the desktop.
    #[test]
    fn player_key_sets_are_routed_to_separate_players() {
        use KeyCode::*;
        let player_0 = [Up, Down, Left, Right, W, A, S, D, F1, F2, F3, F4];
        let player_1 = [I, K, J, L, F5, F6, F7, F8];
        for key in player_0 {
            assert_eq!(
                Input::from_keycode(key).unwrap().0,
                0,
                "keycode {} -> player 0",
                key as u32
            );
        }
        for key in player_1 {
            assert_eq!(
                Input::from_keycode(key).unwrap().0,
                1,
                "keycode {} -> player 1",
                key as u32
            );
        }
    }

    /// Distinct physical keys may map to the same logical input (arrow-Up and
    /// W both steer up). They carry distinct keycodes, so the auto-repeat
    /// suppression in `apply_input` must treat them as separate keys: holding
    /// one never swallows a press of the other.
    #[test]
    fn distinct_keys_sharing_an_input_stay_distinct() {
        use KeyCode::*;
        let (p0, up_arrow) = Input::from_keycode(Up).unwrap();
        let (w0, w) = Input::from_keycode(W).unwrap();
        assert_eq!(p0, w0);
        assert!(matches!(up_arrow, Input::Up));
        assert!(matches!(w, Input::Up));
        assert_ne!(Up as u32, W as u32);
    }
}
