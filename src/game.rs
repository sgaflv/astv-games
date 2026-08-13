//! Game orchestration: two snakes on one board sharing a single food.

use crate::engine::color::Color;
pub use crate::snake::{
    BOARD_PX, BOARD_PY, BOARD_X, BOARD_Y, CELL, Cell, Direction, GRID_SIZE_X, GRID_SIZE_Y, HUD_H,
    MAX_QUEUED_INPUTS, Segment, Snake,
};

use crate::engine::render::Renderer;
use crate::engine::sprites::RleSprite;

use rand::RngExt;
use rand::rngs::ThreadRng;

/// Fixed simulation timestep (Hz). The game simulation advances in fixed 1/60 s
/// steps, independently of the display refresh rate.
pub const TARGET_FRAMES: usize = 60;

/// Game update happens every SIM_FRAMES, so a few times per second but not on each frame
pub const SIM_FRAMES: usize = 24;

/// Number of players (one snake each).
pub const PLAYERS: usize = 2;

// Palette
const BG_COLOR: Color = Color::rgb(13, 13, 18);
const GRID_COLOR: Color = Color::rgb(38, 38, 46);

/// Per-player snake colors.
const SNAKE_COLORS: [Color; PLAYERS] = [Color::rgb(51, 204, 51), Color::rgb(77, 148, 255)];

/// Starting cells: player 0 top-left facing right, player 1 bottom-right
/// facing left (mirrored spawns on the 20 x 11 board).
const SPAWNS: [(Cell, Direction); PLAYERS] = [
    (Cell { x: 3, y: 0 }, Direction::Right),
    (Cell { x: 16, y: 10 }, Direction::Left),
];

/// The game owns the snakes, the shared food pool, and the shared tick clock.
pub struct Game {
    pub snakes: Vec<Snake>,
    /// All food cells currently on the board. Cells shed by bites turn straight
    /// into extra food, so there can be several at once.
    food: Vec<Cell>,
    rng: ThreadRng,

    /// Takes the value of a frame count within a second: 0..63
    pub frame_cnt: usize,
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
            food: Vec::new(),
            rng,
            frame_cnt: 0,
        };
        game.respawn_food();
        game
    }

    /// All food cells currently on the board.
    pub fn foods(&self) -> &[Cell] {
        &self.food
    }

    /// Advance one fixed simulation step.
    pub fn step(&mut self) {
        self.frame_cnt += 1;

        if self.frame_cnt >= SIM_FRAMES {
            self.frame_cnt = 0;

            for s in &mut self.snakes {
                s.move_tick(&self.food);
            }

            // Remove any food cell a snake head reached.
            let heads: Vec<Cell> = self.snakes.iter().map(|s| s.head()).collect();
            self.food.retain(|f| !heads.contains(f));

            // Resolve bites before topping the pool back up: shed cells turn
            // into food immediately, so a bite must not be followed by an
            // extra respawn. A replacement only appears when no food is left.
            self.resolve_bites();
            if self.food.is_empty() {
                self.respawn_food();
            }
        }
    }

    /// Resolve bites from this tick. Only a head can bite, and only non-head
    /// body cells can be bitten: a head landing on another snake's body cell
    /// (index >= 1) bites the owner there, keeping `[0..index]` and shedding
    /// the bitten cell plus the tail behind it. Each shed cell immediately
    /// turns into food. Head-to-head contact has no effect, and neither does
    /// body-to-body contact between different snakes. Each victim is split
    /// once, at the bite closest to its head.
    fn resolve_bites(&mut self) {
        let mut splits: Vec<(usize, usize)> = Vec::new();
        for i in 0..self.snakes.len() {
            let head = self.snakes[i].head();
            for j in 0..self.snakes.len() {
                if j == i {
                    continue; // a snake cannot bite itself
                }
                // Heads (index 0) cannot be bitten, by any snake.
                let idx = self.snakes[j]
                    .body
                    .iter()
                    .enumerate()
                    .skip(1)
                    .find(|(_, s)| s.current == head)
                    .map(|(k, _)| k);
                if let Some(idx) = idx {
                    splits.push((j, idx));
                }
            }
        }

        let mut best: Vec<Option<usize>> = vec![None; self.snakes.len()];
        for (victim, idx) in splits {
            best[victim] = Some(best[victim].map_or(idx, |b| b.min(idx)));
        }
        for (victim, idx) in best.into_iter().enumerate() {
            if let Some(idx) = idx {
                let severed = self.snakes[victim].split_at(idx);
                for s in severed {
                    self.shed_food(s.current);
                }
            }
        }
    }

    /// Turn a cell shed by a bite into food. If the cell is covered by a snake
    /// or already holds food, the new food is respawned on a free cell instead.
    fn shed_food(&mut self, pos: Cell) {
        let occupied = self
            .snakes
            .iter()
            .any(|s| s.body.iter().any(|seg| seg.current == pos))
            || self.food.contains(&pos);

        if occupied {
            if self.food.is_empty() {
                self.respawn_food();
            }
        } else {
            self.food.push(pos);
        }
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

    /// Draw the board, both snakes and the shared food. `apple` is the RLE
    /// sprite sheet used for food; its frames cycle with every move tick.
    pub fn draw(&self, r: &mut impl Renderer, frame: usize, apple: &[RleSprite]) {
        draw_grid(r);
        for s in &self.snakes {
            s.draw(r, frame);
        }
        self.draw_food(r, apple);
    }

    fn draw_food(&self, r: &mut impl Renderer, apple: &[RleSprite]) {
        if apple.is_empty() {
            return;
        }

        let frame = (self.frame_cnt) * 12 / 60;
        for food in &self.food {
            let (cx, cy) = Self::cell_screen(*food);
            apple[frame].draw(r, cx, cy);
        }
    }

    /// Screen pixel position of a cell's top-left corner (top-left origin).
    fn cell_screen(cell: Cell) -> (i32, i32) {
        (BOARD_X + cell.x * CELL, BOARD_Y + cell.y * CELL)
    }

    /// Add a food cell on a free cell (not under any snake body or existing
    /// food).
    fn respawn_food(&mut self) {
        loop {
            let x = self.rng.random_range(0..GRID_SIZE_X);
            let y = self.rng.random_range(0..GRID_SIZE_Y);
            let pos = Cell { x, y };
            let occupied = self
                .snakes
                .iter()
                .any(|s| s.body.iter().any(|seg| seg.current == pos))
                || self.food.contains(&pos);
            if !occupied {
                self.food.push(pos);
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
    fn a_snake_cannot_bite_itself() {
        let mut game = Game::new(2);
        // Plant food off-board so neither snake grows during the test.
        game.food = vec![Cell { x: 100, y: 100 }];
        // Snake 0 curls so its head lands on its own neck (index 2) after the
        // move: head (5,5) moving Right onto (6,5), which the shift keeps.
        game.snakes[0].body = vec![
            Segment {
                current: Cell { x: 5, y: 5 },
                previous: Cell { x: 5, y: 5 },
            },
            Segment {
                current: Cell { x: 6, y: 5 },
                previous: Cell { x: 6, y: 5 },
            },
            Segment {
                current: Cell { x: 6, y: 6 },
                previous: Cell { x: 6, y: 6 },
            },
            Segment {
                current: Cell { x: 5, y: 6 },
                previous: Cell { x: 5, y: 6 },
            },
        ];
        game.snakes[0].direction = Direction::Right;
        // Snake 1 stays far away (off-board direction, no contact).
        game.snakes[1].body = vec![
            Segment {
                current: Cell { x: 16, y: 10 },
                previous: Cell { x: 16, y: 10 },
            },
            Segment {
                current: Cell { x: 17, y: 10 },
                previous: Cell { x: 17, y: 10 },
            },
            Segment {
                current: Cell { x: 18, y: 10 },
                previous: Cell { x: 18, y: 10 },
            },
            Segment {
                current: Cell { x: 19, y: 10 },
                previous: Cell { x: 19, y: 10 },
            },
        ];
        game.snakes[1].direction = Direction::Left;

        for _ in 0..TARGET_FRAMES {
            game.step();
        }

        // The head moved onto its own body, but a snake cannot bite itself:
        // snake 0 keeps all four cells.
        assert_eq!(game.snakes[0].head(), Cell { x: 6, y: 5 });
        assert_eq!(game.snakes[0].body.len(), 4);
        assert_eq!(game.snakes[1].body.len(), 4);
        assert_eq!(game.foods().len(), 1);
    }

    #[test]
    fn initial_two_snake_layout() {
        let game = Game::new(2);
        assert_eq!(game.snakes.len(), 2);
        assert_eq!(head(&game, 0), Cell { x: 3, y: 0 });
        assert_eq!(game.snakes[0].direction, Direction::Right);
        assert_eq!(head(&game, 1), Cell { x: 16, y: 10 });
        assert_eq!(game.snakes[1].direction, Direction::Left);
        // Spawns are mirrored and do not overlap the initial food.
        assert!(game.foods().iter().all(|f| *f != head(&game, 0)));
        assert!(game.foods().iter().all(|f| *f != head(&game, 1)));
    }

    #[test]
    fn one_player_game_spawns_a_single_snake() {
        let game = Game::new(1);
        assert_eq!(game.snakes.len(), 1);
        assert_eq!(head(&game, 0), Cell { x: 3, y: 0 });
    }
    #[test]
    fn both_snakes_move_in_lockstep() {
        let mut game = Game::new(2);
        for _ in 0..TARGET_FRAMES {
            game.step();
        }
        assert_eq!(head(&game, 0), Cell { x: 4, y: 0 });
        assert_eq!(head(&game, 1), Cell { x: 15, y: 10 });
    }

    #[test]
    fn snakes_share_the_food_pool() {
        let mut game = Game::new(2);
        // Steer snake 0 toward the nearest food; snake 1 wanders.
        let mut reached = false;
        for _ in 0..TARGET_FRAMES * 300 {
            let h = head(&game, 0);
            let (fx, fy) = game
                .foods()
                .iter()
                .map(|f| ((f.x - h.x).abs() + (f.y - h.y).abs(), *f))
                .min_by_key(|(dist, _)| *dist)
                .map(|(_, f)| (f.x, f.y))
                .unwrap();
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
        // Every food sits off both snakes.
        assert!(game.foods().iter().all(|f| {
            !game
                .snakes
                .iter()
                .any(|s| s.body.iter().any(|seg| seg.current == *f))
        }));
    }

    #[test]
    fn per_player_input_routes_to_the_right_snake() {
        let mut game = Game::new(2);
        game.queue_direction(0, Direction::Down);
        game.queue_direction(1, Direction::Up);
        for _ in 0..TARGET_FRAMES {
            game.step();
        }
        assert_eq!(head(&game, 0), Cell { x: 3, y: 1 });
        assert_eq!(head(&game, 1), Cell { x: 16, y: 9 });
    }

    #[test]
    fn a_head_bites_off_the_victim_s_tail() {
        let mut game = Game::new(2);
        // Plant food off-board so neither snake grows during the test.
        game.food = vec![Cell { x: 100, y: 100 }];
        // Snake 0 head (6,5) facing right; snake 1 straight line facing right.
        game.snakes[0].body = vec![
            Segment {
                current: Cell { x: 6, y: 5 },
                previous: Cell { x: 6, y: 5 },
            },
            Segment {
                current: Cell { x: 5, y: 5 },
                previous: Cell { x: 5, y: 5 },
            },
        ];
        game.snakes[0].direction = Direction::Right;
        game.snakes[1].body = vec![
            Segment {
                current: Cell { x: 8, y: 5 },
                previous: Cell { x: 8, y: 5 },
            },
            Segment {
                current: Cell { x: 7, y: 5 },
                previous: Cell { x: 7, y: 5 },
            },
            Segment {
                current: Cell { x: 6, y: 5 },
                previous: Cell { x: 6, y: 5 },
            },
            Segment {
                current: Cell { x: 5, y: 5 },
                previous: Cell { x: 5, y: 5 },
            },
        ];
        game.snakes[1].direction = Direction::Right;

        for _ in 0..TARGET_FRAMES {
            game.step();
        }

        // Snake 0's head moved to (7,5), snake 1's body cell at index 2.
        assert_eq!(game.snakes[0].head(), Cell { x: 7, y: 5 });
        // Snake 1 keeps [0..2] (head (9,5) after moving) and sheds the bitten
        // cell plus the tail behind it.
        assert_eq!(game.snakes[1].body.len(), 2);
        assert_eq!(game.snakes[1].body[0].current, Cell { x: 9, y: 5 });
        assert_eq!(game.snakes[1].body[1].current, Cell { x: 8, y: 5 });
        // The shed cells immediately became extra food. Both are covered by
        // snake 0 (its head and neck), so the food was respawned on free cells.
        assert_eq!(game.foods().len(), 3);
        assert!(game.foods().contains(&Cell { x: 100, y: 100 }));
        assert!(game.foods().iter().all(|f| {
            !game
                .snakes
                .iter()
                .any(|s| s.body.iter().any(|seg| seg.current == *f))
        }));
    }

    #[test]
    fn severed_tail_becomes_food_immediately() {
        let mut game = Game::new(2);
        game.food = vec![Cell { x: 100, y: 100 }];
        // Snake 0 is short; snake 1 is a long straight line. After one tick
        // snake 0's head lands on snake 1's body at index 4.
        game.snakes[0].body = vec![
            Segment {
                current: Cell { x: 9, y: 5 },
                previous: Cell { x: 9, y: 5 },
            },
            Segment {
                current: Cell { x: 8, y: 5 },
                previous: Cell { x: 8, y: 5 },
            },
        ];
        game.snakes[0].direction = Direction::Right;
        game.snakes[1].body = (7..=13)
            .rev()
            .map(|x| Segment {
                current: Cell { x, y: 5 },
                previous: Cell { x, y: 5 },
            })
            .collect();
        game.snakes[1].direction = Direction::Right;

        for _ in 0..TARGET_FRAMES {
            game.step();
        }

        // Snake 1 keeps [0..4] and sheds the bitten cell plus the tail.
        assert_eq!(game.snakes[1].body.len(), 4);
        assert_eq!(game.snakes[1].head(), Cell { x: 14, y: 5 });
        // The shed cells immediately become food: each shed cell adds exactly
        // one food. (10,5) and (9,5) sit under snake 0's head and neck, so they
        // respawn; (8,5) is free, so it becomes food right there.
        assert_eq!(game.foods().len(), 4);
        assert!(game.foods().contains(&Cell { x: 8, y: 5 }));
        assert!(game.foods().contains(&Cell { x: 100, y: 100 }));
        assert!(game.foods().iter().all(|f| {
            !game
                .snakes
                .iter()
                .any(|s| s.body.iter().any(|seg| seg.current == *f))
        }));
    }

    #[test]
    fn a_bite_shedding_food_does_not_spawn_extra_food() {
        let mut game = Game::new(2);
        // The only food sits where snake 0's head lands, so the pool empties
        // in the same tick the bite sheds cells.
        game.food = vec![Cell { x: 6, y: 5 }];
        game.snakes[0].body = vec![
            Segment {
                current: Cell { x: 5, y: 5 },
                previous: Cell { x: 5, y: 5 },
            },
            Segment {
                current: Cell { x: 4, y: 5 },
                previous: Cell { x: 4, y: 5 },
            },
        ];
        game.snakes[0].direction = Direction::Right;
        // Snake 1's post-move body: head (8,5) ... (1,5); index 2 is (6,5).
        game.snakes[1].body = (0..=7)
            .rev()
            .map(|x| Segment {
                current: Cell { x, y: 5 },
                previous: Cell { x, y: 5 },
            })
            .collect();
        game.snakes[1].direction = Direction::Right;

        for _ in 0..TARGET_FRAMES {
            game.step();
        }

        // Snake 0 ate the last food and bit snake 1's cell at index 2 in the
        // same tick. Six cells were shed and each turns into food (respawned
        // where a snake covers it, or in place). No extra food may spawn just
        // because the pool emptied in the same tick.
        assert_eq!(game.snakes[0].head(), Cell { x: 6, y: 5 });
        assert_eq!(game.snakes[1].body.len(), 2);
        assert_eq!(game.snakes[1].body[0].current, Cell { x: 8, y: 5 });
        assert_eq!(game.snakes[1].body[1].current, Cell { x: 7, y: 5 });
        assert_eq!(game.foods().len(), 6);
        assert!(game.foods().contains(&Cell { x: 3, y: 5 }));
        assert!(game.foods().contains(&Cell { x: 2, y: 5 }));
        assert!(game.foods().contains(&Cell { x: 1, y: 5 }));
        assert!(game.foods().iter().all(|f| {
            !game
                .snakes
                .iter()
                .any(|s| s.body.iter().any(|seg| seg.current == *f))
        }));
    }

    #[test]
    fn food_replacement_waits_until_pool_is_empty() {
        let mut game = Game::new(2);
        // Two foods: one on the head's path, one off-board.
        game.food = vec![Cell { x: 6, y: 5 }, Cell { x: 100, y: 100 }];
        game.snakes[0].body = vec![
            Segment {
                current: Cell { x: 5, y: 5 },
                previous: Cell { x: 5, y: 5 },
            },
            Segment {
                current: Cell { x: 4, y: 5 },
                previous: Cell { x: 4, y: 5 },
            },
        ];
        game.snakes[0].direction = Direction::Right;

        for _ in 0..TARGET_FRAMES {
            game.step();
        }

        // The head ate (6,5) and grew, but because the off-board food remains
        // on the board no replacement was spawned.
        assert_eq!(game.snakes[0].body.len(), 3);
        assert_eq!(game.foods().len(), 1);
        assert!(game.foods().contains(&Cell { x: 100, y: 100 }));
    }

    #[test]
    fn heads_cannot_bite_heads() {
        let mut game = Game::new(2);
        game.food = vec![Cell { x: 100, y: 100 }];
        // Two snakes driving into each other head-on.
        game.snakes[0].body = vec![
            Segment {
                current: Cell { x: 5, y: 5 },
                previous: Cell { x: 5, y: 5 },
            },
            Segment {
                current: Cell { x: 4, y: 5 },
                previous: Cell { x: 4, y: 5 },
            },
        ];
        game.snakes[0].direction = Direction::Right;
        game.snakes[1].body = vec![
            Segment {
                current: Cell { x: 7, y: 5 },
                previous: Cell { x: 7, y: 5 },
            },
            Segment {
                current: Cell { x: 8, y: 5 },
                previous: Cell { x: 8, y: 5 },
            },
        ];
        game.snakes[1].direction = Direction::Left;

        for _ in 0..TARGET_FRAMES {
            game.step();
        }

        // Both heads land on the same cell, but a head is not biteable.
        assert_eq!(game.snakes[0].head(), Cell { x: 6, y: 5 });
        assert_eq!(game.snakes[1].head(), Cell { x: 6, y: 5 });
        assert_eq!(game.snakes[0].body.len(), 2);
        assert_eq!(game.snakes[1].body.len(), 2);
        assert_eq!(game.foods().len(), 1);
    }

    #[test]
    fn bodies_do_not_interact_between_snakes() {
        let mut game = Game::new(2);
        game.food = vec![Cell { x: 100, y: 100 }];
        // Snake 0 head (5,5) facing left; snake 1 a long straight line facing
        // right. After one tick both snakes will hold a non-head segment on
        // the same cell (5,5) while their heads sit elsewhere.
        game.snakes[0].body = vec![
            Segment {
                current: Cell { x: 5, y: 5 },
                previous: Cell { x: 5, y: 5 },
            },
            Segment {
                current: Cell { x: 6, y: 5 },
                previous: Cell { x: 6, y: 5 },
            },
        ];
        game.snakes[0].direction = Direction::Left;
        game.snakes[1].body = (4..=8)
            .rev()
            .map(|x| Segment {
                current: Cell { x, y: 5 },
                previous: Cell { x, y: 5 },
            })
            .collect();
        game.snakes[1].direction = Direction::Right;

        for _ in 0..TARGET_FRAMES {
            game.step();
        }

        // Both non-heads now share (5,5); no bite may result.
        assert_eq!(game.snakes[0].head(), Cell { x: 4, y: 5 });
        assert_eq!(game.snakes[1].head(), Cell { x: 9, y: 5 });
        let overlap = game.snakes[0]
            .body
            .iter()
            .skip(1)
            .map(|s| s.current)
            .find(|c| game.snakes[1].body.iter().skip(1).any(|s| s.current == *c));
        assert_eq!(overlap, Some(Cell { x: 5, y: 5 }));
        assert_eq!(game.snakes[0].body.len(), 2);
        assert_eq!(game.snakes[1].body.len(), 5);
        assert_eq!(game.foods().len(), 1);
    }
}
