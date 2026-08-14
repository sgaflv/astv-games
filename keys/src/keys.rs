//! The key-tester scene: samples the engine's per-player held input state
//! every frame and prints the currently held inputs, so any physical key or
//! gamepad button can be checked against its logical input. The engine's
//! diagnostic HUD (FPS, render time, window, scale) stays visible on top-left,
//! like in every other scene.

use engine::color::{Color, Palette};
use engine::input::{INPUT_COUNT, Input, InputState, PLAYERS};
use engine::render::{Framebuffer, Renderer};
use engine::scene::{Scene, SceneAction};

/// The scene background.
const BG: Color = Color::rgb(16, 20, 28);
/// Text colors: exact matches of the default palette's fixed slots, so they
/// land on the classic light-gray / gray / bright-white / bright-cyan entries.
const LIGHT_GRAY: Color = Color::rgb(192, 192, 192);
const GRAY: Color = Color::rgb(128, 128, 128);
const BRIGHT_WHITE: Color = Color::rgb(255, 255, 255);
const BRIGHT_CYAN: Color = Color::rgb(0, 255, 255);

/// Every logical input, in display order.
const ALL_INPUTS: [Input; INPUT_COUNT] = [
    Input::Up,
    Input::Down,
    Input::Left,
    Input::Right,
    Input::Confirm,
    Input::Back,
    Input::Pause,
    Input::GameA,
    Input::GameB,
    Input::GameX,
    Input::GameY,
];

// Layout.
const TITLE_Y: i32 = 22;
const PLAYER_Y: [i32; PLAYERS] = [62, 118];
const HELD_Y: [i32; PLAYERS] = [86, 142];
const LEGEND_Y: [i32; 3] = [200, 214, 228];

/// Short label for a logical input.
fn input_label(input: Input) -> &'static str {
    match input {
        Input::Up => "UP",
        Input::Down => "DOWN",
        Input::Left => "LEFT",
        Input::Right => "RIGHT",
        Input::Confirm => "OK",
        Input::Back => "BACK",
        Input::Pause => "PAUSE",
        Input::GameA => "A",
        Input::GameB => "B",
        Input::GameX => "X",
        Input::GameY => "Y",
    }
}

/// The held inputs as a space-separated string, "NONE" when empty.
fn held_text(inputs: &[Input]) -> String {
    let mut text = String::new();
    if inputs.is_empty() {
        text.push_str("NONE");
        return text;
    }
    for (i, input) in inputs.iter().enumerate() {
        if i > 0 {
            text.push(' ');
        }
        text.push_str(input_label(*input));
    }
    text
}

/// The key-tester scene.
pub struct Keys {
    /// The held logical inputs per player, refreshed from `InputState` in
    /// `update`.
    held: [Vec<Input>; PLAYERS],
}

impl Keys {
    pub fn new() -> Keys {
        Keys {
            held: [Vec::new(), Vec::new()],
        }
    }

    /// Snapshot the per-player held inputs for the next draw.
    fn refresh(&mut self, input: &InputState) {
        for player in 0..PLAYERS {
            self.held[player].clear();
            self.held[player].extend(
                ALL_INPUTS
                    .iter()
                    .copied()
                    .filter(|&i| input.held(player, i)),
            );
        }
    }
}

impl Default for Keys {
    fn default() -> Keys {
        Keys::new()
    }
}

impl Scene for Keys {
    fn input(&mut self, _player: usize, input: Input, down: bool) -> SceneAction {
        if down && input == Input::Back {
            return SceneAction::PopToRoot;
        }
        SceneAction::Continue
    }

    fn update(&mut self, _dt: f64, input: &InputState) -> SceneAction {
        self.refresh(input);
        SceneAction::Continue
    }

    fn draw(&mut self, fb: &mut Framebuffer) {
        fb.clear(BG);

        let w = fb.width() as i32;
        let title = "KEY TESTER";
        let tx = (w - engine::font::text_width(title, 2)) / 2;
        fb.draw_text(tx, TITLE_Y, 2, BRIGHT_WHITE, title);

        for (player, (&ly, &hy)) in PLAYER_Y.iter().zip(HELD_Y.iter()).enumerate() {
            let label = format!("PLAYER {}", player + 1);
            fb.draw_text(16, ly, 2, BRIGHT_CYAN, &label);
            let held = held_text(&self.held[player]);
            let color = if self.held[player].is_empty() {
                LIGHT_GRAY
            } else {
                BRIGHT_WHITE
            };
            fb.draw_text(16, hy, 2, color, &held);
        }

        fb.draw_text(16, LEGEND_Y[0], 1, GRAY, "P1: ARROWS / WASD / F1-F4");
        fb.draw_text(16, LEGEND_Y[1], 1, GRAY, "P2: IJKL / F5-F8");
        fb.draw_text(
            16,
            LEGEND_Y[2],
            1,
            GRAY,
            "ENTER OK   ESC BACK   SPACE PAUSE",
        );
    }

    fn palette(&self) -> Palette {
        let mut p = Palette::default();
        p.add(BG);
        p
    }

    fn clear_color(&self) -> Color {
        BG
    }
}
