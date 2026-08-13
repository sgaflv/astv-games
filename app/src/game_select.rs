//! Game selection screen: choose which game to play before the player count.

use engine::color::Color;
use engine::font;
use engine::input::{Input, InputState};
use engine::render::{Framebuffer, Renderer};
use engine::scene::{Scene, SceneAction};
use gibbon::play::Playing as GibbonPlaying;
use snake::play::Playing as SnakePlaying;

/// The games that can be selected and played. Confirming a game creates its
/// instance right here (see `Scene::input`), so only the chosen game's palette
/// and sprites are in memory at a time.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GameKind {
    Snake,
    Gibbon,
}

impl GameKind {
    /// All games, in menu order.
    const ALL: [GameKind; 2] = [GameKind::Snake, GameKind::Gibbon];

    /// Short display name, used on the game selection and player-count
    /// screens.
    pub fn label(self) -> &'static str {
        match self {
            GameKind::Snake => "SNAKE",
            GameKind::Gibbon => "GIBBON",
        }
    }
}

const TITLE: &str = "SELECT GAME";
const TITLE_SCALE: i32 = 3;
const TITLE_Y: i32 = 54;
const OPTION_SCALE: i32 = 2;
const OPTION_Y: i32 = 150;
const OPTION_LINE: i32 = 36;
// Match the default palette's fixed slots exactly: light gray (7) and gray (8).
const TEXT_COLOR: Color = Color::rgb(192, 192, 192);
const DIM_COLOR: Color = Color::rgb(128, 128, 128);

/// The game-selection screen, shown before the player count. Direction keys
/// cycle the selection, Confirm (Enter/OK) opens the player-count menu for the
/// chosen game, Back quits.
pub struct GameSelect {
    selection: usize,
}

impl GameSelect {
    pub fn new() -> GameSelect {
        GameSelect { selection: 0 }
    }
}

impl Default for GameSelect {
    fn default() -> GameSelect {
        GameSelect::new()
    }
}

impl Scene for GameSelect {
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
                // Create the selected game instance now: the palette and the
                // decoded sprites stay alive while the player-count menu and
                // the game run, and are dropped on exit.
                let game = match GameKind::ALL[self.selection] {
                    GameKind::Snake => crate::menu::PendingGame::Snake(SnakePlaying::new()),
                    GameKind::Gibbon => crate::menu::PendingGame::Gibbon(GibbonPlaying::new()),
                };
                SceneAction::Push(Box::new(crate::menu::Menu::new(game)))
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

        for (i, kind) in GameKind::ALL.iter().enumerate() {
            let selected = i == self.selection;
            let y = OPTION_Y + i as i32 * OPTION_LINE;
            let label = kind.label();
            // Center the option text; the cursor column sits just to its left.
            let x = (w - font::text_width(label, OPTION_SCALE)) / 2;
            let color = if selected { TEXT_COLOR } else { DIM_COLOR };
            if selected {
                fb.draw_text(x - 40, y, OPTION_SCALE, TEXT_COLOR, ">");
            }
            fb.draw_text(x, y, OPTION_SCALE, color, label);
        }
    }
}
