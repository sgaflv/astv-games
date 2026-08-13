//! Player-count selection screen: choose 1 or 2 players, then start the game.

use crate::engine::color::Color;
use crate::engine::font;
use crate::engine::input::{Input, InputState};
use crate::engine::render::{Framebuffer, Renderer};
use crate::engine::scene::{Scene, SceneAction};
use crate::play::Playing;

// Menu visuals.
const TITLE: &str = "SNAKE";
const TITLE_SCALE: i32 = 3;
const TITLE_Y: i32 = 54;
const OPTIONS: [&str; 2] = ["1 PLAYER", "2 PLAYERS"];
const OPTION_SCALE: i32 = 2;
const OPTION_Y: i32 = 150;
const OPTION_LINE: i32 = 36;
const TEXT_COLOR: Color = Color::rgb(204, 204, 214);
const DIM_COLOR: Color = Color::rgb(120, 120, 130);

/// The player-count selection screen. Direction keys cycle the selection,
/// Confirm (Enter/OK) starts a game with the chosen player count, Back quits.
pub struct Menu {
    selection: usize,
}

impl Menu {
    pub fn new() -> Menu {
        Menu { selection: 0 }
    }
}

impl Default for Menu {
    fn default() -> Menu {
        Menu::new()
    }
}

impl Scene for Menu {
    fn input(&mut self, _player: usize, input: Input, down: bool) -> SceneAction {
        if !down {
            return SceneAction::Continue;
        }
        match input {
            Input::Up | Input::Down | Input::Left | Input::Right => {
                self.selection = 1 - self.selection;
                SceneAction::Continue
            }
            Input::Confirm | Input::GameA | Input::GameB => {
                SceneAction::Switch(Box::new(Playing::new(self.selection + 1)))
            }
            Input::Back => SceneAction::Quit,
            Input::Pause | Input::GameX | Input::GameY => SceneAction::Continue,
        }
    }

    fn update(&mut self, _dt: f64, _input: &InputState) -> SceneAction {
        SceneAction::Continue
    }

    fn draw(&mut self, fb: &mut Framebuffer) {
        let w = fb.width() as i32;

        let tx = (w - font::text_width(TITLE, TITLE_SCALE)) / 2;
        fb.draw_text(tx, TITLE_Y, TITLE_SCALE, TEXT_COLOR, TITLE);

        for (i, option) in OPTIONS.iter().enumerate() {
            let selected = i == self.selection;
            let y = OPTION_Y + i as i32 * OPTION_LINE;
            // Center the option text; the cursor column sits just to its left.
            let x = (w - font::text_width(option, OPTION_SCALE)) / 2;
            let color = if selected { TEXT_COLOR } else { DIM_COLOR };
            if selected {
                fb.draw_text(x - 40, y, OPTION_SCALE, TEXT_COLOR, ">");
            }
            fb.draw_text(x, y, OPTION_SCALE, color, option);
        }
    }
}
