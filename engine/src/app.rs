//! Engine shell: the miniquad `EventHandler` that owns the framebuffer, the
//! presenter, input routing, timing diagnostics and the currently active
//! [`Scene`]. Nothing here is game-specific; a game supplies its own scenes
//! (the `app` package's `game_select`/`menu`/`play` are an example) and the
//! engine drives them.

use crate::color::{PAL_LIGHT_GRAY, Palette};
use crate::input::{Input, InputState};
use crate::present::Presenter;
use crate::render::{Framebuffer, Renderer};
use crate::scene::{Scene, SceneAction};

use miniquad::{EventHandler, KeyCode, KeyMods};

use std::fmt::Write as _;
use std::mem;

/// Upper bound for a single frame delta, so a pause, debugger break or
/// scheduling hiccup never causes an enormous catch-up.
const MAX_FRAME_TIME: f64 = 0.25;

/// How often the engine's diagnostic HUD text is refreshed (avoid per-frame
/// text blits).
const HUD_REFRESH_EVERY: u32 = 30;
const HUD_POS: (i32, i32) = (6, 6);

/// The engine run loop: owns the framebuffer, presenter, timing/FPS stats,
/// per-player input state and the one active scene. Implements
/// `miniquad::EventHandler`, so the platform calls `update`/`draw` each frame.
pub struct Stage {
    /// The active scene (menu, gameplay, score, game over, ...).
    scene: Box<dyn Scene>,
    /// Scenes below the active one, awaiting a return via
    /// `SceneAction::Pop`/`PopToRoot`. The root scene (e.g. game selection)
    /// sits at the bottom.
    stack: Vec<Box<dyn Scene>>,
    framebuffer: Framebuffer,
    presenter: Presenter,
    /// The palette currently uploaded to the presenter and applied to the
    /// framebuffer; swapped when the active scene's palette changes.
    active_palette: Palette,

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

    // Input (edge detection + per-player held state, see `InputState`).
    input_state: InputState,

    // Diagnostic HUD.
    hud_buffer: String,
    hud_dirty: bool,
    window_w: i32,
    window_h: i32,
}

impl Stage {
    /// Build the engine shell running `initial_scene`.
    pub fn new(initial_scene: Box<dyn Scene>) -> Stage {
        let now = miniquad::date::now();
        Stage {
            scene: initial_scene,
            stack: Vec::new(),
            framebuffer: Framebuffer::new(),
            presenter: Presenter::new(),
            active_palette: Palette::default(),
            frame_start: now,
            frame_count: 0,
            fps_value: 0,
            fps_accum: 0.0,
            render_time_accum: 0.0,
            render_time_frames: 0,
            render_us: 0.0,
            stat_accum: 0.0,
            input_state: InputState::new(),
            hud_buffer: String::with_capacity(128),
            hud_dirty: true,
            window_w: 0,
            window_h: 0,
        }
    }

    /// Apply whatever the scene asked for.
    fn run_action(&mut self, action: SceneAction) {
        match action {
            SceneAction::Continue => {}
            SceneAction::Push(scene) => {
                self.stack.push(mem::replace(&mut self.scene, scene));
            }
            SceneAction::Switch(scene) => self.scene = scene,
            SceneAction::Pop => {
                if let Some(scene) = self.stack.pop() {
                    self.scene = scene;
                } else {
                    miniquad::window::request_quit();
                }
            }
            SceneAction::PopToRoot => {
                if self.stack.is_empty() {
                    miniquad::window::request_quit();
                } else {
                    // Drop everything above the root scene, then make the root
                    // active again.
                    self.stack.drain(1..);
                    self.scene = self.stack.pop().expect("stack checked non-empty");
                }
            }
            SceneAction::Quit => miniquad::window::request_quit(),
        }
    }

    /// Route one input edge (down or up) from the physical `key` (a platform
    /// keycode used only to tell distinct keys apart). Auto-repeat of the
    /// *same* held key is ignored by `InputState`, but a different key that
    /// maps to the same logical input is still delivered, so one player's held
    /// key never blocks another player's (or another key's) press.
    fn apply_input(&mut self, player: usize, key: u32, input: Input, down: bool) {
        if self.input_state.key_edge(player, key, input, down) {
            let action = self.scene.input(player, input, down);
            self.run_action(action);
        }
    }

    /// Drain device-aware gamepad events (Android). No-op on desktop.
    #[cfg(target_os = "android")]
    fn drain_player_input(&mut self) {
        crate::input::drain_into(|event| {
            if let Some(input) = crate::input::android_keycode_to_input(event.keycode) {
                self.apply_input(event.player, event.keycode as u32, input, event.down);
            }
        });
        self.input_state.set_axes(crate::input::axes());
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

    /// Draw the engine's diagnostic overlay (FPS, render time, window, scale)
    /// on top of whatever the scene painted. The HUD uses the default
    /// light-gray slot, which every palette contains.
    fn draw_hud(&mut self) {
        if self.hud_dirty {
            self.refresh_hud();
        }
        let hud_color = self.active_palette.rgb(PAL_LIGHT_GRAY);
        self.framebuffer
            .draw_text(HUD_POS.0, HUD_POS.1, 1, hud_color, &self.hud_buffer);
    }

    fn render(&mut self) {
        // Start time measure.
        let t0 = miniquad::date::now();

        // Swap to the scene's palette if it changed (e.g. a game scene became
        // active). Games own per-game palettes; the shell keeps the framebuffer
        // and the presenter's palette texture in sync.
        let palette = self.scene.palette();
        if palette != self.active_palette {
            self.active_palette = palette;
            self.framebuffer.set_palette(palette);
            self.presenter.set_palette(palette.bytes());
        }

        self.framebuffer.zero();
        self.scene.draw(&mut self.framebuffer);
        self.draw_hud();

        // End time measure.
        let t1 = miniquad::date::now();
        self.render_time_accum += t1 - t0;
        self.render_time_frames += 1;

        self.presenter
            .present(&self.framebuffer, self.scene.clear_color());
    }
}

impl EventHandler for Stage {
    fn update(&mut self) {
        let now = miniquad::date::now();

        let mut dt = now - self.frame_start;
        self.frame_start = now;

        // Don't let a pause, debugger break, scheduling hiccup, etc. cause an
        // enormous catch-up.
        dt = dt.min(MAX_FRAME_TIME);

        #[cfg(target_os = "android")]
        self.drain_player_input();

        let action = self.scene.update(dt, &self.input_state);
        self.run_action(action);

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
        // Lifecycle pause: let the active scene stop advancing while hidden.
        self.scene.suspend();
    }

    fn window_restored_event(&mut self) {
        self.scene.resume();
        let now = miniquad::date::now();
        self.frame_start = now;
    }

    fn quit_requested_event(&mut self) {}
}
