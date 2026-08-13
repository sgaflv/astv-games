//! The gibbon game: a Lode Runner-style grid game. The player controls a
//! gibbon that runs across floors, climbs ladders, hangs from railings and
//! digs through wooden floors to collect fruits. Two guards with different
//! chase priorities try to catch it; a level is complete once every fruit is
//! taken.

use crate::level::GRID_X as GX;
pub use crate::level::{GRID_X, GRID_Y, Level, Tile};
pub use crate::palette::{
    BG, BRICK, BRICK_DARK, GUARD_BODY, GUARD_DARK, GUARD_EYE, GUARD_FACE, HOLE, HOLE_EDGE, HUD,
    LADDER, LADDER_DARK, PLAYER_BODY, PLAYER_DARK, PLAYER_EYE, PLAYER_FACE, WOOD, WOOD_DARK,
    WOOD_TOP,
};
use engine::render::Renderer;
use engine::sprites::RleSprite;

use rand::RngExt;
use rand::rngs::ThreadRng;

/// Fixed simulation timestep (Hz). The game advances in fixed 1/60 s steps,
/// independently of the display refresh rate.
pub const TARGET_FRAMES: usize = 60;

/// The gibbon acts every SIM_FRAMES frames, so 6 moves per second.
pub const SIM_FRAMES: usize = 10;

/// Guards advance every GUARD_EVERY gibbon moves.
const GUARD_EVERY: usize = 2;

/// A dug wooden tile stays open for DIG_TICKS gibbon moves (10 seconds).
const DIG_TICKS: usize = 10 * TARGET_FRAMES / SIM_FRAMES;

/// Starting lives.
const LIVES: i32 = 3;

/// Sim ticks spent showing "LEVEL CLEAR" before the next level loads.
const CLEAR_TICKS: usize = 50;

/// Sim ticks spent showing the gibbon was caught before it respawns.
const DEAD_TICKS: usize = 40;

/// One board cell in logical pixels.
pub const CELL: i32 = 24;
/// The board in pixels (480 x 264).
pub const BOARD_PX: i32 = GRID_X as i32 * CELL;
pub const BOARD_PY: i32 = GRID_Y as i32 * CELL;
/// Rows of the top HUD band.
pub const HUD_H: i32 = 3;
/// The board's top-left corner in screen pixels.
pub const BOARD_X: i32 = 0;
pub const BOARD_Y: i32 = HUD_H;

/// Sprite frame count of the character sheets: [right0, right1, left0, left1,
/// climb].
pub const CHARACTER_FRAMES: usize = 5;
/// Sprite frame index of the climb pose.
pub const CLIMB_FRAME: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
}

/// One moving character (the gibbon or a guard): a logical cell plus the
/// previous cell, which lets the renderer interpolate between ticks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Actor {
    pub x: i32,
    pub y: i32,
    pub prev_x: i32,
    pub prev_y: i32,
    /// -1 facing left, +1 facing right.
    pub facing: i32,
    /// Counts successful moves; drives the walk-cycle animation.
    pub step_phase: usize,
}

impl Actor {
    fn at(x: i32, y: i32) -> Actor {
        Actor {
            x,
            y,
            prev_x: x,
            prev_y: y,
            facing: 1,
            step_phase: 0,
        }
    }

    fn move_to(&mut self, x: i32, y: i32, dir: Direction) {
        self.prev_x = self.x;
        self.prev_y = self.y;
        self.x = x;
        self.y = y;
        self.step_phase += 1;
        match dir {
            Direction::Left => self.facing = -1,
            Direction::Right => self.facing = 1,
            _ => {}
        }
    }
}

/// A dug-out wooden tile waiting to regrow.
struct Hole {
    x: i32,
    y: i32,
    ticks: usize,
}

/// The game's progression state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// The gibbon moves and the guards chase.
    Playing,
    /// Every fruit is taken; the timer counts down before the next level.
    Cleared,
    /// The gibbon was caught; the timer counts down before respawning.
    Dead,
    /// All lives are gone.
    GameOver,
    /// Every level was completed.
    Win,
}

/// Sprite sheets handed to [`Game::draw`], decoded by the playing scene.
pub struct GameSprites<'a> {
    pub fruit: &'a [RleSprite],
    pub gibbon: &'a [RleSprite],
    pub guard: &'a [RleSprite],
}

/// The game owns the level, the gibbon, the guards, the dug holes and the
/// shared tick clock.
pub struct Game {
    /// All embedded levels; the game advances through them in order.
    levels: Vec<Level>,
    pub level: Level,
    pub gibbon: Actor,
    pub guards: Vec<Actor>,
    /// The direction the player is currently holding.
    pub dir: Option<Direction>,
    holes: Vec<Hole>,
    rng: ThreadRng,
    /// Frame counter within a move (0..SIM_FRAMES), for interpolation.
    pub frame_cnt: usize,
    tick: usize,
    guard_tick: usize,
    pub state: State,
    state_timer: usize,
    pub lives: i32,
    pub level_index: usize,
    pub fruits_left: usize,
}

/// A minimal built-in level, used when no level files are embedded.
const FALLBACK: &str = "|..@...........@....\n\
                        |###################\n\
                        |...................\n\
                        |........|..........\n\
                        |........|.#########\n\
                        |........|..........\n\
                        |........|..........\n\
                        |.....|..|..........\n\
                        |.....|..|..........\n\
                        |.s...|..|....g....g\n\
                        ####################";

impl Game {
    /// Load every embedded level and start the first one.
    pub fn new() -> Game {
        let mut levels = crate::level::load_all();
        if levels.is_empty() {
            levels.push(crate::level::parse(FALLBACK).expect("fallback level parses"));
        }
        Game::from_levels(levels)
    }

    /// Build a game from an explicit level list (used by tests).
    pub fn from_levels(levels: Vec<Level>) -> Game {
        let mut game = Game {
            levels,
            level: Level::default(),
            gibbon: Actor::at(0, 0),
            guards: Vec::new(),
            dir: None,
            holes: Vec::new(),
            rng: rand::rng(),
            frame_cnt: 0,
            tick: 0,
            guard_tick: 0,
            state: State::Playing,
            state_timer: 0,
            lives: LIVES,
            level_index: 0,
            fruits_left: 0,
        };
        game.start_level(0);
        game
    }

    /// Restart the whole game from level one with full lives.
    pub fn restart(&mut self) {
        self.lives = LIVES;
        self.start_level(0);
    }

    /// Load `index` and reset the actors, holes and fruit counter.
    fn start_level(&mut self, index: usize) {
        let index = index.min(self.levels.len() - 1);
        self.level_index = index;
        self.level = self.levels[index].clone();
        self.gibbon = Actor::at(self.level.spawn.0 as i32, self.level.spawn.1 as i32);
        self.guards = self
            .level
            .guard_spawns
            .iter()
            .map(|&(x, y)| Actor::at(x as i32, y as i32))
            .collect();
        self.holes.clear();
        self.dir = None;
        self.tick = 0;
        self.guard_tick = 0;
        self.fruits_left = self.level.fruits;
        self.state = State::Playing;
        self.state_timer = 0;
    }

    fn next_level(&mut self) {
        if self.level_index + 1 < self.levels.len() {
            self.start_level(self.level_index + 1);
        } else {
            self.state = State::Win;
            self.state_timer = 0;
        }
    }

    /// Set the currently held direction (None when the player holds nothing).
    pub fn set_dir(&mut self, dir: Option<Direction>) {
        self.dir = dir;
    }

    /// Dig the wooden tile diagonally below-left (`side` -1) or below-right
    /// (`side` +1). Only possible while standing on solid ground, and only
    /// against wood. The tile stays open for 10 seconds, then regrows.
    pub fn dig(&mut self, side: i32) {
        if self.state != State::Playing {
            return;
        }
        let (x, y) = (self.gibbon.x, self.gibbon.y);
        if !self.tile(x, y + 1).is_solid() {
            return; // must stand on a floor to dig it
        }
        let (tx, ty) = (x + side, y + 1);
        if tx < 0 || tx >= GX as i32 || ty < 0 || ty >= GRID_Y as i32 {
            return;
        }
        if self.level.tile(tx as usize, ty as usize) != Tile::Wood {
            return;
        }
        self.level.set_tile(tx as usize, ty as usize, Tile::Empty);
        self.holes.push(Hole {
            x: tx,
            y: ty,
            ticks: DIG_TICKS,
        });
    }

    /// Advance one fixed simulation step.
    pub fn step(&mut self) {
        self.frame_cnt += 1;
        if self.frame_cnt >= SIM_FRAMES {
            self.frame_cnt = 0;
            self.sim_tick();
        }
    }

    fn sim_tick(&mut self) {
        self.tick += 1;
        self.update_holes();

        if self.state_timer > 0 {
            self.state_timer -= 1;
            if self.state_timer == 0 {
                match self.state {
                    State::Cleared => self.next_level(),
                    State::Dead => self.respawn(),
                    _ => {}
                }
                return;
            }
        }

        if self.state == State::Playing {
            self.tick_playing();
        }
    }

    fn tick_playing(&mut self) {
        // The gibbon moves on the held direction, then gravity applies. The
        // actor is a copy: the movement helpers borrow `self` immutably.
        let mut gibbon = self.gibbon;
        if let Some(dir) = self.dir {
            let target = self.target_of(gibbon, dir);
            if self.can_enter(target, dir) {
                gibbon.move_to(target.x, target.y, dir);
            }
        }
        self.step_gravity(&mut gibbon);
        self.gibbon = gibbon;
        self.collect_fruit();

        // Guards chase every few gibbon moves. Guard 0 minimises the vertical
        // distance, guard 1 the horizontal distance.
        self.guard_tick += 1;
        if self.guard_tick.is_multiple_of(GUARD_EVERY) {
            for i in 0..self.guards.len() {
                let mut guard = self.guards[i];
                let dir = self.guard_choose(guard, i % 2 == 0);
                if let Some(dir) = dir {
                    let target = self.target_of(guard, dir);
                    if self.can_enter(target, dir) {
                        guard.move_to(target.x, target.y, dir);
                    }
                }
                self.step_gravity(&mut guard);
                self.guards[i] = guard;
            }
        }

        // A guard that reaches the gibbon catches it.
        if self
            .guards
            .iter()
            .any(|g| g.x == self.gibbon.x && g.y == self.gibbon.y)
        {
            self.lives -= 1;
            if self.lives > 0 {
                self.state = State::Dead;
                self.state_timer = DEAD_TICKS;
            } else {
                self.state = State::GameOver;
                self.state_timer = 0;
            }
        }
    }

    /// Respawn the gibbon (and reset the guards) after being caught.
    fn respawn(&mut self) {
        self.start_level(self.level_index);
        self.lives = self.lives.max(1);
    }

    fn collect_fruit(&mut self) {
        let (x, y) = (self.gibbon.x as usize, self.gibbon.y as usize);
        if x < GRID_X && y < GRID_Y && self.level.tile(x, y) == Tile::Fruit {
            self.level.set_tile(x, y, Tile::Empty);
            self.fruits_left = self.fruits_left.saturating_sub(1);
            // A level clears only once every fruit is actually taken, so
            // fruitless test levels never auto-complete.
            if self.fruits_left == 0 {
                self.state = State::Cleared;
                self.state_timer = CLEAR_TICKS;
            }
        }
    }

    // --- Movement helpers ---------------------------------------------------

    /// The tile at `(x, y)`; out-of-bounds reads as empty.
    fn tile(&self, x: i32, y: i32) -> Tile {
        if x >= 0 && y >= 0 && x < GX as i32 && y < GRID_Y as i32 {
            self.level.tile(x as usize, y as usize)
        } else {
            Tile::Empty
        }
    }

    fn target_of(&self, actor: Actor, dir: Direction) -> Cell {
        match dir {
            Direction::Up => Cell {
                x: actor.x,
                y: actor.y - 1,
            },
            Direction::Down => Cell {
                x: actor.x,
                y: actor.y + 1,
            },
            Direction::Left => Cell {
                x: actor.x - 1,
                y: actor.y,
            },
            Direction::Right => Cell {
                x: actor.x + 1,
                y: actor.y,
            },
        }
    }

    /// Whether `dir` from `target` is a legal step. Up needs a ladder/railing
    /// above, Down needs one below, horizontal needs a non-solid target.
    fn can_enter(&self, target: Cell, dir: Direction) -> bool {
        match dir {
            Direction::Up => {
                target.y >= 0
                    && (self.tile(target.x, target.y) == Tile::Ladder
                        || self.tile(target.x, target.y) == Tile::Railing)
            }
            Direction::Down => {
                target.y < GRID_Y as i32
                    && (self.tile(target.x, target.y) == Tile::Ladder
                        || self.tile(target.x, target.y) == Tile::Railing)
            }
            Direction::Left | Direction::Right => {
                target.x >= 0
                    && target.x < GX as i32
                    && target.y >= 0
                    && target.y < GRID_Y as i32
                    && !self.tile(target.x, target.y).is_solid()
            }
        }
    }

    /// Whether an actor in cell `(x, y)` is supported: solid ground below,
    /// or something to hang on (a ladder or railing in the current cell or
    /// directly above it).
    fn supported(&self, x: i32, y: i32) -> bool {
        if self.tile(x, y + 1).is_solid() {
            return true;
        }
        matches!(self.tile(x, y), Tile::Ladder | Tile::Railing)
            || matches!(self.tile(x, y - 1), Tile::Ladder | Tile::Railing)
    }

    /// Apply one cell of gravity when the actor is unsupported.
    fn step_gravity(&self, actor: &mut Actor) {
        if actor.y >= GRID_Y as i32 - 1 {
            return; // the bottom row is the floor
        }
        if !self.supported(actor.x, actor.y) {
            actor.move_to(actor.x, actor.y + 1, Direction::Down);
        }
    }

    /// Pick the next guard move. The primary axis (vertical for guard 0,
    /// horizontal for guard 1) is tried first, then the secondary axis, then
    /// any legal move at random so a guard never freezes.
    fn guard_choose(&mut self, guard: Actor, primary_vertical: bool) -> Option<Direction> {
        let (dx, dy) = (self.gibbon.x - guard.x, self.gibbon.y - guard.y);
        let (p, s) = if primary_vertical { (dy, dx) } else { (dx, dy) };
        let primary = if p < 0 {
            Some(Direction::Up)
        } else if p > 0 {
            Some(Direction::Down)
        } else {
            None
        };
        if let Some(dir) = primary
            && self.can_enter(self.target_of(guard, dir), dir)
        {
            return Some(dir);
        }
        let secondary = if s < 0 {
            Some(Direction::Left)
        } else if s > 0 {
            Some(Direction::Right)
        } else {
            None
        };
        if let Some(dir) = secondary
            && self.can_enter(self.target_of(guard, dir), dir)
        {
            return Some(dir);
        }
        let options: Vec<Direction> = [
            Direction::Up,
            Direction::Down,
            Direction::Left,
            Direction::Right,
        ]
        .into_iter()
        .filter(|&dir| self.can_enter(self.target_of(guard, dir), dir))
        .collect();
        if options.is_empty() {
            None
        } else {
            Some(options[self.rng.random_range(0..options.len())])
        }
    }

    /// Regrow dug tiles whose timer expired, once their cell is free.
    fn update_holes(&mut self) {
        for hole in &mut self.holes {
            if hole.ticks > 0 {
                hole.ticks -= 1;
            }
        }
        self.holes.retain(|hole| {
            if hole.ticks > 0 {
                return true;
            }
            let occupied = (self.gibbon.x == hole.x && self.gibbon.y == hole.y)
                || self.guards.iter().any(|g| g.x == hole.x && g.y == hole.y);
            if occupied {
                return true; // wait for the cell to clear before regrowing
            }
            self.level
                .set_tile(hole.x as usize, hole.y as usize, Tile::Wood);
            false
        });
    }

    // --- Drawing ------------------------------------------------------------

    /// Draw the tiles, the fruits, the guards and the gibbon.
    pub fn draw(&self, r: &mut impl Renderer, frame: usize, sprites: &GameSprites) {
        self.draw_tiles(r);
        self.draw_fruits(r, sprites.fruit);
        for guard in &self.guards {
            self.draw_actor(r, *guard, frame, sprites.guard);
        }
        if self.state != State::Dead {
            self.draw_actor(r, self.gibbon, frame, sprites.gibbon);
        }
    }

    fn draw_tiles(&self, r: &mut impl Renderer) {
        for y in 0..GRID_Y {
            for x in 0..GRID_X {
                let (px, py) = cell_screen(x as i32, y as i32);
                match self.level.tile(x, y) {
                    Tile::Wood => draw_wood(r, px, py),
                    Tile::Brick => draw_brick(r, px, py),
                    Tile::Ladder => draw_ladder(r, px, py),
                    Tile::Railing => draw_railing(r, px, py),
                    Tile::Empty | Tile::Fruit => {}
                }
            }
        }
        for hole in &self.holes {
            draw_hole(r, hole.x, hole.y);
        }
    }

    fn draw_fruits(&self, r: &mut impl Renderer, fruit: &[RleSprite]) {
        if fruit.is_empty() {
            return;
        }
        let f = (self.tick / 2) % fruit.len();
        for y in 0..GRID_Y {
            for x in 0..GRID_X {
                if self.level.tile(x, y) == Tile::Fruit {
                    let (px, py) = cell_screen(x as i32, y as i32);
                    fruit[f].draw(r, px, py);
                }
            }
        }
    }

    fn draw_actor(&self, r: &mut impl Renderer, actor: Actor, frame: usize, sheet: &[RleSprite]) {
        if sheet.is_empty() {
            return;
        }
        let (px, py) = interpolate(actor, frame);
        let vertical = actor.prev_y != actor.y;
        let index = if vertical {
            CLIMB_FRAME
        } else {
            let phase = (actor.step_phase / 2) % 2;
            if actor.facing < 0 { 2 + phase } else { phase }
        };
        if index < sheet.len() {
            sheet[index].draw(r, px, py);
        }
    }
}

impl Default for Game {
    fn default() -> Game {
        Game::new()
    }
}

/// Interpolated screen top-left of an actor during a move tick.
fn interpolate(actor: Actor, frame: usize) -> (i32, i32) {
    let (cx, cy) = cell_screen(actor.x, actor.y);
    if actor.prev_x == actor.x && actor.prev_y == actor.y {
        return (cx, cy);
    }
    let (px, py) = cell_screen(actor.prev_x, actor.prev_y);
    (interp(px, cx, frame), interp(py, cy, frame))
}

/// Integer linear interpolation with rounding.
fn interp(a: i32, b: i32, frame: usize) -> i32 {
    let d = (b - a) as i64;
    (a as i64 + d * frame as i64 / SIM_FRAMES as i64) as i32
}

/// Screen pixel position of a cell's top-left corner (top-left origin).
fn cell_screen(x: i32, y: i32) -> (i32, i32) {
    (BOARD_X + x * CELL, BOARD_Y + y * CELL)
}

fn draw_wood(r: &mut impl Renderer, x: i32, y: i32) {
    r.fill_rect(x, y, CELL, CELL, WOOD);
    r.fill_rect(x, y, CELL, 2, WOOD_TOP);
    r.fill_rect(x, y + CELL - 2, CELL, 2, WOOD_DARK);
    r.fill_rect(x + 3, y + 5, CELL - 6, 1, WOOD_DARK);
    r.fill_rect(x + 5, y + 11, CELL - 10, 1, WOOD_DARK);
    r.fill_rect(x + 4, y + 17, CELL - 8, 1, WOOD_DARK);
}

fn draw_brick(r: &mut impl Renderer, x: i32, y: i32) {
    r.fill_rect(x, y, CELL, CELL, BRICK);
    r.fill_rect(x, y + 7, CELL, 1, BRICK_DARK);
    r.fill_rect(x, y + 15, CELL, 1, BRICK_DARK);
    r.fill_rect(x + 3, y, 1, 8, BRICK_DARK);
    r.fill_rect(x + 11, y, 1, 8, BRICK_DARK);
    r.fill_rect(x + 19, y, 1, 8, BRICK_DARK);
    r.fill_rect(x + 7, y + 8, 1, 8, BRICK_DARK);
    r.fill_rect(x + 15, y + 8, 1, 8, BRICK_DARK);
}

fn draw_ladder(r: &mut impl Renderer, x: i32, y: i32) {
    r.fill_rect(x + 6, y, 2, CELL, LADDER_DARK);
    r.fill_rect(x + CELL - 8, y, 2, CELL, LADDER_DARK);
    for i in 0..4 {
        let ry = y + 3 + i * 6;
        r.fill_rect(x + 5, ry, CELL - 10, 2, LADDER);
    }
}

fn draw_railing(r: &mut impl Renderer, x: i32, y: i32) {
    r.fill_rect(x, y + 1, CELL, 2, LADDER_DARK);
    r.fill_rect(x + 1, y, CELL - 2, 1, LADDER);
}

fn draw_hole(r: &mut impl Renderer, x: i32, y: i32) {
    let (px, py) = cell_screen(x, y);
    r.fill_rect(px, py + 3, CELL, CELL - 3, HOLE);
    r.fill_rect(px, py + 3, CELL, 1, HOLE_EDGE);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(text: &str) -> Level {
        crate::level::parse(text).expect("test level parses")
    }

    fn game(levels: Vec<Level>) -> Game {
        Game::from_levels(levels)
    }

    /// Advance `ticks` sim ticks.
    fn advance(game: &mut Game, ticks: usize) {
        for _ in 0..ticks * SIM_FRAMES {
            game.step();
        }
    }

    #[test]
    fn gibbon_falls_when_unsupported() {
        // Whole level empty: the gibbon drops to the bottom row.
        let mut game = game(vec![level(
            "s...................g\n\
             ....................",
        )]);
        assert_eq!(game.gibbon.y, 0);
        advance(&mut game, 20);
        assert_eq!(game.gibbon.y, GRID_Y as i32 - 1);
        assert_eq!(game.gibbon.x, 0);
    }

    #[test]
    fn gibbon_stops_on_solid_ground() {
        // A floor directly under the spawn: no fall.
        let mut game = game(vec![level(
            "s...................g\n\
             ##..................\n\
             ....................",
        )]);
        advance(&mut game, 10);
        assert_eq!(game.gibbon, Actor::at(0, 0));
    }

    #[test]
    fn gibbon_climbs_ladders() {
        // Ladder shaft at x0, spawn at the bottom.
        let mut game = game(vec![level(
            "|...................g\n\
             |...................g\n\
             |...................g\n\
             |...................g\n\
             |...................g\n\
             |...................g\n\
             |...................g\n\
             |...................g\n\
             |...................g\n\
             s...................g\n\
             ####################",
        )]);
        // Ladder goes all the way to the bottom row; spawn sits on it.
        game.set_dir(Some(Direction::Up));
        advance(&mut game, 12);
        assert_eq!(game.gibbon.y, 0);
        // Climbing back down. The spawn cell itself is empty (`s`), so the
        // bottom rung is the ladder cell just above it.
        game.set_dir(Some(Direction::Down));
        advance(&mut game, 12);
        assert_eq!(game.gibbon.y, 8);
    }

    #[test]
    fn gibbon_hangs_on_a_railing() {
        // Railing above the spawn, empty everywhere else: it must not fall.
        let mut game = game(vec![level(
            "-..................g\n\
             s...................g\n\
             ....................",
        )]);
        advance(&mut game, 10);
        assert_eq!(game.gibbon.y, 1);
        // Stepping off the end of the railing makes it fall.
        game.set_dir(Some(Direction::Right));
        advance(&mut game, 12);
        assert_eq!(game.gibbon.y, GRID_Y as i32 - 1);
    }

    #[test]
    fn climbing_up_onto_a_railing_works() {
        // Ladder below a railing: climbing up passes through the railing.
        let mut game = game(vec![level(
            "-..................g\n\
             |..................g\n\
             s..................g\n\
             ....................",
        )]);
        game.set_dir(Some(Direction::Up));
        advance(&mut game, 4);
        assert_eq!(game.gibbon.y, 0);
    }

    #[test]
    fn walls_block_horizontal_movement() {
        let mut game = game(vec![level(
            "s*..................g\n\
             ##..................\n\
             ....................",
        )]);
        game.set_dir(Some(Direction::Right));
        advance(&mut game, 2);
        assert_eq!(game.gibbon.x, 0); // blocked by the brick
    }

    #[test]
    fn digging_opens_a_hole_and_regrows_after_ten_seconds() {
        // Gibbon stands on a wooden floor and digs the tile down-left.
        let mut game = game(vec![level(
            ".s..................g\n\
             ##..................\n\
             ....................",
        )]);
        assert_eq!(game.level.tile(0, 1), Tile::Wood);
        game.dig(-1);
        assert_eq!(game.level.tile(0, 1), Tile::Empty);
        assert_eq!(game.holes.len(), 1);
        // After the full dig time the wood regrows.
        advance(&mut game, DIG_TICKS + 1);
        assert_eq!(game.level.tile(0, 1), Tile::Wood);
        assert!(game.holes.is_empty());
    }

    #[test]
    fn digging_requires_solid_ground() {
        // No floor under the gibbon: digging must fail.
        let mut game = game(vec![level(
            ".s..................g\n\
             ....................",
        )]);
        game.dig(-1);
        assert!(game.holes.is_empty());
    }

    #[test]
    fn digging_targets_only_wood() {
        // The diagonal below-left is brick: nothing to dig.
        let mut game = game(vec![level(
            ".s..................g\n\
             *#..................\n\
             ....................",
        )]);
        game.dig(-1);
        assert!(game.holes.is_empty());
        assert_eq!(game.level.tile(0, 1), Tile::Brick);
    }

    #[test]
    fn collecting_all_fruits_clears_the_level() {
        // Fruit sits one cell right of the spawn.
        let mut game = game(vec![level(
            "s@..................g\n\
             ##..................\n\
             ....................",
        )]);
        game.set_dir(Some(Direction::Right));
        advance(&mut game, 1);
        assert_eq!(game.fruits_left, 0);
        assert_eq!(game.state, State::Cleared);
    }

    #[test]
    fn guards_prioritise_their_axis() {
        // Ladder shaft at x5; gibbon above on the ladder, guard below.
        let mut game = game(vec![level(
            ".....|.............g\n\
             .....|.............g\n\
             .....|.............g\n\
             .....|.............g\n\
             .....|.............g\n\
             .....|.............g\n\
             .....|.............g\n\
             .....|.............g\n\
             .....|.............g\n\
             .....s............g\n\
             ####################",
        )]);
        game.gibbon = Actor::at(5, 0);
        game.guards = vec![Actor::at(5, 8), Actor::at(10, 8)];
        // Guard 0 (vertical priority): the gibbon is straight up a ladder.
        assert_eq!(game.guard_choose(game.guards[0], true), Some(Direction::Up));
        // Guard 1 (horizontal priority): the gibbon is to its left on a clear
        // row, so it walks left first.
        assert_eq!(
            game.guard_choose(game.guards[1], false),
            Some(Direction::Left)
        );
    }

    #[test]
    fn guards_catch_the_gibbon_and_it_respawns() {
        let mut game = game(vec![level(
            "s..................g\n\
             ##..................\n\
             ....................",
        )]);
        // Park a guard on the gibbon.
        game.guards = vec![Actor::at(0, 0)];
        game.state = State::Playing;
        advance(&mut game, 1);
        assert_eq!(game.lives, LIVES - 1);
        assert_eq!(game.state, State::Dead);
        // After the death timer the gibbon is back at the spawn, guards reset.
        advance(&mut game, DEAD_TICKS);
        assert_eq!(game.state, State::Playing);
        assert_eq!(game.gibbon.x, 0);
        assert_eq!(game.gibbon.y, 0);
        assert_eq!((game.guards[0].x, game.guards[0].y), (19, 0));
    }

    #[test]
    fn losing_all_lives_ends_the_game() {
        let mut game = game(vec![level(
            "s...................g\n\
             ##..................\n\
             ....................",
        )]);
        game.lives = 1;
        game.guards = vec![Actor::at(0, 0)];
        advance(&mut game, 1);
        assert_eq!(game.state, State::GameOver);
    }

    #[test]
    fn levels_advance_after_clear() {
        let a = level(
            "s@..................g\n\
             ##..................\n\
             ....................",
        );
        let b = level(
            "s@..................g\n\
             ##..................\n\
             ....................",
        );
        let mut game = game(vec![a, b]);
        game.set_dir(Some(Direction::Right));
        advance(&mut game, CLEAR_TICKS + 2);
        assert_eq!(game.level_index, 1);
        assert_eq!(game.state, State::Playing);
        assert_eq!(game.fruits_left, 1);
    }

    #[test]
    fn completing_the_last_level_wins() {
        let mut game = game(vec![level(
            "s@..................g\n\
             ##..................\n\
             ....................",
        )]);
        game.set_dir(Some(Direction::Right));
        advance(&mut game, CLEAR_TICKS + 2);
        assert_eq!(game.state, State::Win);
    }
}
