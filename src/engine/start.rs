//! Starting and restarting the game: the menu/playing state machine plus the
//! gameplay session state that starting a game resets.

use crate::game::{Game, PLAYERS};

/// Top-level app state: player-count menu or the running game.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Menu,
    Playing,
}

/// The runnable game plus the session state a new game begins with. A
/// two-player game is built eagerly so `Stage` always has one to draw behind
/// the menu, but simulation only advances once `begin` puts the app into
/// `State::Playing`.
pub struct Start {
    pub game: Game,
    pub state: State,
    pub paused: bool,
    pub pause_requested: bool,
    pub sim_accumulator: f64,
}

impl Default for Start {
    fn default() -> Start {
        Start::new()
    }
}

impl Start {
    /// Initial state: a two-player game built eagerly, but the app sits in the
    /// player-count menu.
    pub fn new() -> Start {
        Start {
            game: Game::new(PLAYERS),
            state: State::Menu,
            paused: false,
            pause_requested: false,
            sim_accumulator: 0.0,
        }
    }

    /// Start a new game with `players` snakes and enter the playing state.
    pub fn begin(&mut self, players: usize) {
        self.game = Game::new(players);
        self.state = State::Playing;
        self.paused = false;
        self.pause_requested = false;
        self.sim_accumulator = 0.0;
    }
}
