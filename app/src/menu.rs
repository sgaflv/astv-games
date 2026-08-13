//! Player-count selection screen: choose 1 or 2 players for the game selected
//! on the game-selection screen, then start it.

use crate::game_select::GameKind;
use engine::color::Color;
use engine::font;
use engine::input::{Input, InputState};
use engine::render::{Framebuffer, Renderer};
use engine::scene::{Scene, SceneAction};
use gibbon::play::Playing as GibbonPlaying;
use snake::play::Playing as SnakePlaying;

// Menu visuals.
const TITLE_SCALE: i32 = 3;
const TITLE_Y: i32 = 54;
const OPTIONS: [&str; 2] = ["1 PLAYER", "2 PLAYERS"];
const OPTION_SCALE: i32 = 2;
const OPTION_Y: i32 = 150;
const OPTION_LINE: i32 = 36;
const TEXT_COLOR: Color = Color::rgb(204, 204, 214);
const DIM_COLOR: Color = Color::rgb(120, 120, 130);

/// The player-count selection screen for a chosen game. Direction keys cycle
/// the selection, Confirm (Enter/OK) starts the selected game with the chosen
/// player count, Back quits.
pub struct Menu {
    selection: usize,
    kind: GameKind,
}

impl Menu {
    /// Player-count selection for the given game.
    pub fn new(kind: GameKind) -> Menu {
        Menu { selection: 0, kind }
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
                let players = self.selection + 1;
                match self.kind {
                    GameKind::Snake => SceneAction::Push(Box::new(SnakePlaying::new(players))),
                    GameKind::Gibbon => SceneAction::Push(Box::new(GibbonPlaying::new(players))),
                }
            }
            Input::Back => SceneAction::Pop,
            Input::Pause | Input::GameX | Input::GameY => SceneAction::Continue,
        }
    }

    fn update(&mut self, _dt: f64, _input: &InputState) -> SceneAction {
        SceneAction::Continue
    }

    fn draw(&mut self, fb: &mut Framebuffer) {
        let w = fb.width() as i32;

        let title = self.kind.label();
        let tx = (w - font::text_width(title, TITLE_SCALE)) / 2;
        fb.draw_text(tx, TITLE_Y, TITLE_SCALE, TEXT_COLOR, title);

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
