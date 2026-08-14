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
// Match the default palette's fixed slots exactly: light gray (7) and gray (8).
const TEXT_COLOR: Color = Color::rgb(192, 192, 192);
const DIM_COLOR: Color = Color::rgb(128, 128, 128);

/// The game instance for the game currently selected: created at the
/// game-selection screen, so only the chosen game's palette and sprites occupy
/// memory. Held here while the player picks the player count, then started and
/// pushed; dropped when the menu or the game is exited. Boxed so the enum
/// stays small (the scenes hold sprite sheets and a game).
pub enum PendingGame {
    Snake(Box<SnakePlaying>),
    Gibbon(Box<GibbonPlaying>),
}

impl PendingGame {
    /// Short display name, used on the player-count screen title.
    fn label(&self) -> &'static str {
        match self {
            PendingGame::Snake(_) => GameKind::Snake.label(),
            PendingGame::Gibbon(_) => GameKind::Gibbon.label(),
        }
    }

    /// How many players this game supports; the player-count screen shows one
    /// option per supported player count. Both games support 1 or 2 players.
    fn players(&self) -> usize {
        match self {
            PendingGame::Snake(_) => 2,
            PendingGame::Gibbon(_) => 2,
        }
    }

    /// Confirm the player count and produce the playing scene.
    fn start(self, players: usize) -> Box<dyn Scene> {
        match self {
            PendingGame::Snake(mut game) => {
                game.start(players);
                Box::new(*game)
            }
            PendingGame::Gibbon(mut game) => {
                game.start(players);
                Box::new(*game)
            }
        }
    }
}

/// The player-count selection screen for a chosen game. Direction keys cycle
/// the selection, Confirm (Enter/OK) starts the selected game with the chosen
/// player count, Back quits (which drops the selected game).
pub struct Menu {
    selection: usize,
    game: Option<PendingGame>,
}

impl Menu {
    /// Player-count selection for the given selected game.
    pub fn new(game: PendingGame) -> Menu {
        Menu {
            selection: 0,
            game: Some(game),
        }
    }
}

impl Scene for Menu {
    fn input(&mut self, _player: usize, input: Input, down: bool) -> SceneAction {
        if !down {
            return SceneAction::Continue;
        }
        match input {
            Input::Up | Input::Down | Input::Left | Input::Right => {
                let count = self
                    .game
                    .as_ref()
                    .expect("menu holds the selected game")
                    .players();
                self.selection = (self.selection + 1) % count;
                SceneAction::Continue
            }
            Input::Confirm | Input::GameA | Input::GameB => {
                let players = self.selection + 1;
                let game = self.game.take().expect("menu holds the selected game");
                SceneAction::Push(game.start(players))
            }
            Input::Back => SceneAction::Pop,
            Input::Pause
            | Input::GameX
            | Input::GameY
            | Input::StickUp
            | Input::StickDown
            | Input::StickLeft
            | Input::StickRight => SceneAction::Continue,
        }
    }

    fn update(&mut self, _dt: f64, _input: &InputState) -> SceneAction {
        SceneAction::Continue
    }

    fn draw(&mut self, fb: &mut Framebuffer) {
        let w = fb.width() as i32;

        let title = self
            .game
            .as_ref()
            .expect("menu holds the selected game")
            .label();
        let tx = (w - font::text_width(title, TITLE_SCALE)) / 2;
        fb.draw_text(tx, TITLE_Y, TITLE_SCALE, TEXT_COLOR, title);

        let players = self
            .game
            .as_ref()
            .expect("menu holds the selected game")
            .players();
        for (i, option) in OPTIONS.iter().take(players).enumerate() {
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
