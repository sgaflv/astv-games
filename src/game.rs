//! Game orchestration: two snakes on one board sharing a single food.

pub use crate::snake::{
    BOARD_PX, BOARD_PY, BOARD_X, BOARD_Y, CELL, Cell, Direction, GRID_SIZE_X, GRID_SIZE_Y, HUD_H,
    MAX_QUEUED_INPUTS, Segment, Snake,
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

/// Number of ticks a shed corpse waits before turning into the shared food.
pub const CORPSE_LIFETIME_TICKS: u32 = 5;

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

/// A static body cell shed by a bite. After `CORPSE_LIFETIME_TICKS` move ticks
/// it turns into the shared food.
pub struct Corpse {
    pub segment: Segment,
    pub color: Color,
    /// Ticks the corpse has been on the board (incremented on each move tick).
    pub age: u32,
}

/// The game owns the snakes, the shared food pool, the corpses shed by bites,
/// and the shared tick clock.
pub struct Game {
    pub snakes: Vec<Snake>,
    /// All food cells currently on the board. Corpses mature into extra food,
    /// so there can be several at once.
    food: Vec<Cell>,
    /// Static body cells severed by bites; they age and become food.
    pub dead: Vec<Corpse>,
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
            food: Vec::new(),
            dead: Vec::new(),
            rng,
            steps_in_tick: 0,
        };
        game.respawn_food();
        game
    }

    /// All food cells currently on the board.
    pub fn foods(&self) -> &[Cell] {
        &self.food
    }

    /// Advance one fixed simulation step. The snakes move one cell every
    /// TICK_STEPS steps, in lockstep, sharing the food pool. Eaten food cells
    /// are removed; a replacement only spawns when no food is left on the
    /// board. Bites (a head landing on another snake's body) are resolved
    /// after the move, and corpses age until they turn into additional food.
    pub fn step(&mut self) {
        self.steps_in_tick += 1;
        if self.steps_in_tick >= TICK_STEPS {
            self.steps_in_tick = 0;
            for s in &mut self.snakes {
                s.move_tick(&self.food);
            }
            // Remove any food cell a snake head reached. New food only appears
            // when the pool has become empty.
            let heads: Vec<Cell> = self.snakes.iter().map(|s| s.head()).collect();
            self.food.retain(|f| !heads.contains(f));
            if self.food.is_empty() {
                self.respawn_food();
            }
            self.resolve_bites();
            for c in &mut self.dead {
                c.age += 1;
            }
            self.mature_corpses();
        }
    }

    /// Turn the oldest corpse that has reached `CORPSE_LIFETIME_TICKS` into an
    /// additional food cell. If that cell is covered by a snake, another corpse
    /// or existing food, the new food is respawned on a free cell instead.
    fn mature_corpses(&mut self) {
        let idx = self
            .dead
            .iter()
            .position(|c| c.age >= CORPSE_LIFETIME_TICKS);
        let Some(idx) = idx else {
            return;
        };
        let corpse = self.dead.remove(idx);
        let pos = corpse.segment.current;
        let occupied = self
            .snakes
            .iter()
            .any(|s| s.body.iter().any(|seg| seg.current == pos))
            || self.dead.iter().any(|c| c.segment.current == pos)
            || self.food.contains(&pos);
        if occupied {
            self.respawn_food();
        } else {
            self.food.push(pos);
        }
    }

    /// Resolve bites from this tick. Only a head can bite, and only non-head
    /// body cells can be bitten: a head landing on another snake's body cell
    /// (index >= 1) bites the owner there, keeping `[0..=index]` and shedding
    /// the tail behind the bite as a static corpse (in the victim's color).
    /// Head-to-head contact has no effect, and neither does body-to-body
    /// contact between different snakes. Corpses never bite: a head moving
    /// onto a corpse cell just passes over it. Each victim is split once, at
    /// the bite closest to its head.
    fn resolve_bites(&mut self) {
        let mut splits: Vec<(usize, usize)> = Vec::new();
        for i in 0..self.snakes.len() {
            let head = self.snakes[i].head();
            for j in 0..self.snakes.len() {
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
                let color = self.snakes[victim].color();
                let severed = self.snakes[victim].split_at(idx);
                self.dead.extend(severed.into_iter().map(|s| Corpse {
                    segment: s,
                    color,
                    age: 0,
                }));
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

    /// Draw the board, both snakes, the static corpses and the shared food.
    pub fn draw(&self, r: &mut impl Renderer, alpha: u32) {
        draw_grid(r);
        for s in &self.snakes {
            s.draw(r, alpha);
        }
        self.draw_corpses(r);
        self.draw_food(r);
    }

    fn draw_corpses(&self, r: &mut impl Renderer) {
        for corpse in &self.dead {
            let (x, y) = Self::cell_screen(corpse.segment.current);
            r.fill_rect(x, y, CELL, CELL, corpse.color);
        }
    }

    fn draw_food(&self, r: &mut impl Renderer) {
        for food in &self.food {
            let (cx, cy) = Self::cell_screen(*food);
            r.fill_circle(cx + CELL / 2, cy + CELL / 2, FOOD_RADIUS, FOOD_COLOR);
        }
    }

    /// Screen pixel position of a cell's top-left corner (top-left origin).
    fn cell_screen(cell: Cell) -> (i32, i32) {
        (BOARD_X + cell.x * CELL, BOARD_Y + cell.y * CELL)
    }

    /// Add a food cell on a free cell (not under any snake body, corpse or
    /// existing food).
    fn respawn_food(&mut self) {
        loop {
            let x = self.rng.random_range(0..GRID_SIZE_X);
            let y = self.rng.random_range(0..GRID_SIZE_Y);
            let pos = Cell { x, y };
            let occupied = self
                .snakes
                .iter()
                .any(|s| s.body.iter().any(|seg| seg.current == pos))
                || self.dead.iter().any(|c| c.segment.current == pos)
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
        for _ in 0..TICK_STEPS {
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
        for _ in 0..TICK_STEPS * 300 {
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
        for _ in 0..TICK_STEPS {
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

        for _ in 0..TICK_STEPS {
            game.step();
        }

        // Snake 0's head moved to (7,5), snake 1's body cell at index 2.
        assert_eq!(game.snakes[0].head(), Cell { x: 7, y: 5 });
        // Snake 1 keeps [0..=2] (head (9,5) after moving) and sheds the tail.
        assert_eq!(game.snakes[1].body.len(), 3);
        assert_eq!(game.snakes[1].body[0].current, Cell { x: 9, y: 5 });
        assert_eq!(game.snakes[1].body[2].current, Cell { x: 7, y: 5 });
        // The severed tail became a static corpse in the victim's color.
        assert_eq!(game.dead.len(), 1);
        assert_eq!(game.dead[0].segment.current, Cell { x: 6, y: 5 });
        assert_eq!(game.dead[0].segment.current, game.dead[0].segment.previous);
        assert_eq!(game.dead[0].color, game.snakes[1].color());
        // The corpse aged once at the end of the tick it was shed.
        assert_eq!(game.dead[0].age, 1);
    }

    #[test]
    fn corpses_do_not_bite() {
        let mut game = Game::new(2);
        game.food = vec![Cell { x: 100, y: 100 }];
        game.snakes[0].body = vec![
            Segment {
                current: Cell { x: 6, y: 5 },
                previous: Cell { x: 6, y: 5 },
            },
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
        game.dead.push(Corpse {
            segment: Segment {
                current: Cell { x: 7, y: 5 },
                previous: Cell { x: 7, y: 5 },
            },
            color: SNAKE_COLORS[1],
            age: 0,
        });

        for _ in 0..TICK_STEPS {
            game.step();
        }

        // The head moved onto the corpse, but the corpse does not bite back.
        assert_eq!(game.snakes[0].body.len(), 3);
        assert_eq!(game.snakes[0].head(), Cell { x: 7, y: 5 });
        assert_eq!(game.dead.len(), 1);
    }

    #[test]
    fn corpses_age_and_turn_into_food() {
        let mut game = Game::new(2);
        game.food = vec![Cell { x: 100, y: 100 }];
        game.dead.push(Corpse {
            segment: Segment {
                current: Cell { x: 5, y: 5 },
                previous: Cell { x: 5, y: 5 },
            },
            color: SNAKE_COLORS[0],
            age: CORPSE_LIFETIME_TICKS - 1,
        });

        // Already one tick shy of the limit: the first step ages it to the
        // lifetime and turns its cell into an additional food.
        for _ in 0..TICK_STEPS {
            game.step();
        }
        assert!(game.dead.is_empty());
        assert_eq!(game.foods().len(), 2);
        assert!(game.foods().contains(&Cell { x: 5, y: 5 }));
    }

    #[test]
    fn corpses_do_not_turn_into_food_early() {
        let mut game = Game::new(2);
        game.food = vec![Cell { x: 100, y: 100 }];
        game.dead.push(Corpse {
            segment: Segment {
                current: Cell { x: 5, y: 5 },
                previous: Cell { x: 5, y: 5 },
            },
            color: SNAKE_COLORS[0],
            age: 0,
        });

        for _ in 0..TICK_STEPS {
            game.step();
        }
        assert_eq!(game.dead.len(), 1);
        assert_eq!(game.dead[0].age, 1);
        assert!(game.foods().iter().all(|f| *f != Cell { x: 5, y: 5 }));
    }

    #[test]
    fn matured_corpse_under_a_snake_respawns_food() {
        let mut game = Game::new(2);
        game.food = vec![Cell { x: 100, y: 100 }];
        // Snake 0's head leaves (5,5) but its neck fills it in the same tick.
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
        game.dead.push(Corpse {
            segment: Segment {
                current: Cell { x: 5, y: 5 },
                previous: Cell { x: 5, y: 5 },
            },
            color: SNAKE_COLORS[1],
            age: CORPSE_LIFETIME_TICKS - 1,
        });

        for _ in 0..TICK_STEPS {
            game.step();
        }

        // The corpse cell is now covered by snake 0's body, so the new food
        // is respawned on a free cell instead.
        assert!(game.dead.is_empty());
        assert_eq!(game.foods().len(), 2);
        assert!(game.foods().iter().all(|f| *f != Cell { x: 5, y: 5 }));
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

        for _ in 0..TICK_STEPS {
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

        for _ in 0..TICK_STEPS {
            game.step();
        }

        // Both heads land on the same cell, but a head is not biteable.
        assert_eq!(game.snakes[0].head(), Cell { x: 6, y: 5 });
        assert_eq!(game.snakes[1].head(), Cell { x: 6, y: 5 });
        assert_eq!(game.snakes[0].body.len(), 2);
        assert_eq!(game.snakes[1].body.len(), 2);
        assert!(game.dead.is_empty());
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

        for _ in 0..TICK_STEPS {
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
        assert!(game.dead.is_empty());
    }

    #[test]
    fn food_respawns_off_corpses() {
        let mut game = Game::new(2);
        game.food = vec![Cell { x: 100, y: 100 }];
        // Scatter a row of corpses across the top of the board.
        for x in 0..GRID_SIZE_X {
            game.dead.push(Corpse {
                segment: Segment {
                    current: Cell { x, y: 0 },
                    previous: Cell { x, y: 0 },
                },
                color: SNAKE_COLORS[0],
                age: 0,
            });
        }
        for _ in 0..200 {
            game.food.clear();
            game.respawn_food();
            let last = game.food.last().unwrap();
            assert!(!game.dead.iter().any(|c| c.segment.current == *last));
        }
    }

    #[test]
    fn alpha_is_strictly_increasing_and_wraps() {
        let mut game = Game::new(2);
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
