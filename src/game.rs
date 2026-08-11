//! Game orchestration: two snakes on one board sharing a single food.

pub use crate::snake::{
    BOARD_PX, BOARD_PY, BOARD_X, BOARD_Y, CELL, Cell, Direction, GRID_SIZE_X, GRID_SIZE_Y, HUD_H,
    MAX_QUEUED_INPUTS, Snake,
};

use crate::engine::render::{Color, Renderer};

use rand::RngExt;
use rand::rngs::ThreadRng;

/// Fixed simulation timestep (Hz). The game simulation advances in fixed 1/60 s
/// steps, independently of the display refresh rate.
pub const SIM_STEP_HZ: u32 = 60;

/// Seconds between snake move ticks.
pub const MOVE_INTERVAL: f64 = 0.5;

/// Number of fixed simulation steps per move tick (0.5 s * 60 Hz).
pub const TICK_STEPS: u32 = (MOVE_INTERVAL * SIM_STEP_HZ as f64) as u32;

/// Number of players (one snake each).
pub const PLAYERS: usize = 2;

// Palette (matches the original Bevy rendering as closely as practical).
const BG_COLOR: Color = Color::rgb(13, 13, 18);
const GRID_COLOR: Color = Color::rgb(38, 38, 46);
const FOOD_COLOR: Color = Color::rgb(230, 26, 26);

const FOOD_RADIUS: i32 = 5;

/// Per-player snake colors.
const SNAKE_COLORS: [Color; PLAYERS] = [Color::rgb(51, 204, 51), Color::rgb(77, 148, 255)];

/// Starting cells: player 0 top-left facing right, player 1 bottom-right
/// facing left (mirrored spawns on the 20 x 11 board).
const SPAWNS: [(Cell, Direction); PLAYERS] = [
    (Cell { x: 3, y: 0 }, Direction::Right),
    (Cell { x: 16, y: 10 }, Direction::Left),
];

/// The game owns the snakes, the shared food, and the shared tick clock.
pub struct Game {
    pub snakes: Vec<Snake>,
    food: Cell,
    rng: ThreadRng,
    steps_in_tick: u32,
}

impl Default for Game {
    fn default() -> Game {
        Game::new(PLAYERS)
    }
}

impl Game {
    /// Build a game with `players` snakes (1 or 2). The rest of the board
    /// (shared food, tick clock) is identical regardless of the player count.
    pub fn new(players: usize) -> Game {
        let players = players.min(PLAYERS);
        let rng = rand::rng();
        let snakes: Vec<Snake> = SPAWNS
            .iter()
            .zip(SNAKE_COLORS)
            .take(players)
            .map(|((head, direction), color)| Snake::spawn(color, *head, *direction))
            .collect();
        let mut game = Game {
            snakes,
            food: Cell { x: 0, y: 0 },
            rng,
            steps_in_tick: 0,
        };
        game.respawn_food();
        game
    }

    pub fn food(&self) -> Cell {
        self.food
    }

    /// Advance one fixed simulation step. The snakes move one cell every
    /// TICK_STEPS steps, in lockstep, sharing one food.
    pub fn step(&mut self) {
        self.steps_in_tick += 1;
        if self.steps_in_tick >= TICK_STEPS {
            self.steps_in_tick = 0;
            let mut ate = false;
            for s in &mut self.snakes {
                s.move_tick(self.food);
                ate |= s.grew_last_tick;
            }
            if ate {
                self.respawn_food();
            }
        }
    }

    /// Interpolation alpha for the current tick, fixed point in 0..=65536.
    /// `(steps + 1) / TICK_STEPS` keeps motion continuous across tick
    /// boundaries (a freshly moved segment starts just past its previous cell).
    pub fn alpha(&self) -> u32 {
        let n = (self.steps_in_tick + 1).min(TICK_STEPS);
        (n * 65536 + TICK_STEPS / 2) / TICK_STEPS
    }

    /// Queue a direction change for a player's snake (buffered until the next
    /// move tick).
    pub fn queue_direction(&mut self, player: usize, dir: Direction) {
        if let Some(s) = self.snakes.get_mut(player) {
            s.queue_direction(dir);
        }
    }

    /// Immediately set a player's snake direction, clearing the input queue.
    pub fn set_direction(&mut self, player: usize, dir: Direction) {
        if let Some(s) = self.snakes.get_mut(player) {
            s.set_direction(dir);
        }
    }

    /// Draw the board, both snakes and the shared food.
    pub fn draw(&self, r: &mut impl Renderer, alpha: u32) {
        draw_grid(r);
        for s in &self.snakes {
            s.draw(r, alpha);
        }
        self.draw_food(r);
    }

    fn draw_food(&self, r: &mut impl Renderer) {
        let (cx, cy) = Self::cell_screen(self.food);
        r.fill_circle(cx + CELL / 2, cy + CELL / 2, FOOD_RADIUS, FOOD_COLOR);
    }

    /// Screen pixel position of a cell's top-left corner (top-left origin).
    fn cell_screen(cell: Cell) -> (i32, i32) {
        (BOARD_X + cell.x * CELL, BOARD_Y + cell.y * CELL)
    }

    /// Place the food on a free cell (not under any snake body).
    fn respawn_food(&mut self) {
        loop {
            let x = self.rng.random_range(0..GRID_SIZE_X);
            let y = self.rng.random_range(0..GRID_SIZE_Y);
            let pos = Cell { x, y };
            let occupied = self
                .snakes
                .iter()
                .any(|s| s.body.iter().any(|seg| seg.current == pos));
            if !occupied {
                self.food = pos;
                break;
            }
        }
    }
}

/// 1px dark grid lines between the cells, drawn behind the snake so the snake
/// covers them as it passes (same layering as the original).
fn draw_grid(r: &mut impl Renderer) {
    for i in 1..GRID_SIZE_X {
        let x = BOARD_X + i * CELL;
        r.fill_rect(x, BOARD_Y, 1, BOARD_PY, GRID_COLOR);
    }

    for i in 1..GRID_SIZE_Y {
        let y = BOARD_Y + i * CELL;
        r.fill_rect(BOARD_X, y, BOARD_PX, 1, GRID_COLOR);
    }
}

pub const fn bg_color() -> Color {
    BG_COLOR
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(game: &Game, player: usize) -> Cell {
        game.snakes[player].head()
    }

    #[test]
    fn initial_two_snake_layout() {
        let game = Game::new();
        assert_eq!(game.snakes.len(), 2);
        assert_eq!(head(&game, 0), Cell { x: 3, y: 0 });
        assert_eq!(game.snakes[0].direction, Direction::Right);
        assert_eq!(head(&game, 1), Cell { x: 16, y: 10 });
        assert_eq!(game.snakes[1].direction, Direction::Left);
        // Spawns are mirrored and do not overlap the shared food.
        assert_ne!(game.food(), head(&game, 0));
        assert_ne!(game.food(), head(&game, 1));
    }

    #[test]
    fn both_snakes_move_in_lockstep() {
        let mut game = Game::new();
        for _ in 0..TICK_STEPS {
            game.step();
        }
        assert_eq!(head(&game, 0), Cell { x: 4, y: 0 });
        assert_eq!(head(&game, 1), Cell { x: 15, y: 10 });
    }

    #[test]
    fn snakes_share_one_food() {
        let mut game = Game::new();
        // Steer snake 0 onto the food; snake 1 wanders.
        let mut reached = false;
        for _ in 0..TICK_STEPS * 300 {
            let fx = game.food().x;
            let fy = game.food().y;
            let h = head(&game, 0);
            let dir = if h.y == fy {
                if fx > h.x {
                    Direction::Right
                } else {
                    Direction::Left
                }
            } else if fy < h.y {
                Direction::Up
            } else {
                Direction::Down
            };
            game.set_direction(0, dir);
            game.step();
            if game.snakes[0].grew_last_tick {
                reached = true;
                break;
            }
        }
        assert!(
            reached,
            "snake 0 never reached the shared food in 300 ticks"
        );
        assert_eq!(game.snakes[0].body.len(), 5);
        // The food respawned off both snakes.
        assert!(
            game.snakes
                .iter()
                .all(|s| !s.body.iter().any(|seg| seg.current == game.food()))
        );
    }

    #[test]
    fn per_player_input_routes_to_the_right_snake() {
        let mut game = Game::new();
        game.queue_direction(0, Direction::Down);
        game.queue_direction(1, Direction::Up);
        for _ in 0..TICK_STEPS {
            game.step();
        }
        assert_eq!(head(&game, 0), Cell { x: 3, y: 1 });
        assert_eq!(head(&game, 1), Cell { x: 16, y: 9 });
    }

    #[test]
    fn alpha_is_strictly_increasing_and_wraps() {
        let mut game = Game::new();
        let mut prev = 0u32;
        for _ in 0..TICK_STEPS {
            let a = game.alpha();
            assert!(a > prev, "alpha must increase during a tick");
            assert!(a <= 65536);
            prev = a;
            game.step();
        }
        // After a tick the alpha wraps back down to just above 0.
        let wrapped = game.alpha();
        assert!(wrapped < prev);
        assert!(wrapped > 0);
    }
}
