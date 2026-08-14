//! The gibbon game: a Lode Runner-style grid game. The player controls a
//! gibbon that runs across floors, climbs ladders, hangs from railings and
//! digs through wooden floors to collect fruits. Two guards with different
//! chase priorities try to catch it; a level is complete once every fruit is
//! taken.

pub use crate::level::{GRID_X, GRID_Y, Level, Tile};
pub use crate::palette::{
    BG, BRICK, BRICK_DARK, GUARD_BODY, GUARD_DARK, GUARD_EYE, GUARD_FACE, HOLE, HOLE_EDGE, HUD,
    LADDER, LADDER_DARK, PLAYER_BODY, PLAYER_DARK, PLAYER_EYE, PLAYER_FACE, WOOD, WOOD_DARK,
    WOOD_TOP,
};
use engine::render::Renderer;
use engine::sprites::RleSprite;

/// Fixed simulation timestep (Hz). The game advances in fixed 1/60 s steps,
/// independently of the display refresh rate.
pub const TARGET_FRAMES: usize = 60;

/// The gibbon and the guards act every SIM_FRAMES frames: each actor crosses
/// one board cell in SIM_FRAMES frames (6 cells per second), so a guard
/// chasing a running gibbon keeps a constant distance.
pub const SIM_FRAMES: usize = 24;

/// A dug wooden tile stays open for this many seconds.
const DIG_SECONDS: usize = 5;

/// The tile stays open for DIG_TICKS sim ticks: one sim tick lasts
/// SIM_FRAMES/TARGET_FRAMES seconds, so `DIG_SECONDS` is about 13 sim ticks.
const DIG_TICKS: usize = (DIG_SECONDS * TARGET_FRAMES + SIM_FRAMES / 2) / SIM_FRAMES;

/// Starting lives.
const LIVES: i32 = 3;

/// Sim ticks spent showing "LEVEL CLEAR" before the next level loads.
const CLEAR_TICKS: usize = 10;

/// Sim ticks spent showing the gibbon was caught before it respawns.
const DEAD_TICKS: usize = 10;

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
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    DigLeft,
    DigRight,
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
}

impl Actor {
    fn at(x: i32, y: i32) -> Actor {
        Actor {
            x,
            y,
            prev_x: x,
            prev_y: y,
            facing: 1,
        }
    }

    fn move_to(&mut self, x: i32, y: i32, dir: Action) {
        self.prev_x = self.x;
        self.prev_y = self.y;
        self.x = x;
        self.y = y;
        match dir {
            Action::Left => self.facing = -1,
            Action::Right => self.facing = 1,
            _ => {}
        }
    }

    /// Settle the actor in its current cell: with `prev` equal to the current
    /// cell the renderer stops interpolating. Called when a move was blocked
    /// or the actor otherwise rests between ticks.
    fn settle(&mut self) {
        self.prev_x = self.x;
        self.prev_y = self.y;
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

    /// The current command. Movement latches: the gibbon keeps going in that
    /// direction until the way is blocked (a failed move clears it), so the
    /// key need not be held. A dig command fires once and resolves to `None`.
    pub action: Option<Action>,

    holes: Vec<Hole>,

    /// Frame counter within a move (0..SIM_FRAMES), for interpolation.
    pub frame_cnt: usize,

    tick: usize,

    pub game_state: State,

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
                        ..s......|....g....g\n\
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
            action: None,
            holes: Vec::new(),
            frame_cnt: 0,
            tick: 0,
            game_state: State::Playing,
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
        self.action = None;
        self.tick = 0;
        self.fruits_left = self.level.fruits;
        self.game_state = State::Playing;
        self.state_timer = 0;
    }

    fn next_level(&mut self) {
        if self.level_index + 1 < self.levels.len() {
            self.start_level(self.level_index + 1);
        } else {
            self.game_state = State::Win;
            self.state_timer = 0;
        }
    }

    /// Set the currently held direction. The direction latches: once set, the
    /// gibbon keeps moving in it until the way is blocked (a failed move
    /// clears it again), so releasing the key does not stop it.
    pub fn set_action(&mut self, action: Option<Action>) {
        self.action = action;
    }

    /// Perform a queued dig from `actor`'s cell.
    fn perform_dig(&mut self, side: i32, actor: Actor) {
        let (x, y) = (actor.x, actor.y);
        let (tx, ty) = (x + side, y + 1);
        if tx < 0 || tx >= GRID_X as i32 || ty < 0 || ty >= GRID_Y as i32 {
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
                match self.game_state {
                    State::Cleared => self.next_level(),
                    State::Dead => self.respawn(),
                    _ => {}
                }
                return;
            }
        }

        if self.game_state == State::Playing {
            self.tick_playing();
        } else {
            // Frozen between rounds: snap the actors to their cells so they
            // do not keep interpolating from their last move.
            self.gibbon.settle();
            for guard in &mut self.guards {
                guard.settle();
            }
        }
    }

    fn tick_playing(&mut self) {
        let mut gibbon = self.gibbon;

        gibbon.settle();

        match self.action {
            // A dig command spends the whole tick: it fires from the current
            // cell even while airborne (stepping off a tile lets you dig it)
            // and holds the fall back for that tick. It is not latched like
            // movement, so it resolves back to no action.
            Some(Action::DigLeft) => {
                self.perform_dig(-1, gibbon);
                self.action = None;
            }
            Some(Action::DigRight) => {
                self.perform_dig(1, gibbon);
                self.action = None;
            }
            Some(action) => {
                let falling = self.step_gravity(&mut gibbon);

                if !falling {
                    let target = self.target_of(gibbon, action);

                    if self.can_enter(
                        Cell {
                            x: gibbon.x,
                            y: gibbon.y,
                        },
                        target,
                        action,
                    ) {
                        gibbon.move_to(target.x, target.y, action);
                        // Stepping down off a ladder onto a railing hangs the
                        // gibbon: it stops there instead of climbing straight
                        // past it.
                        if action == Action::Down && self.tile(target.x, target.y) == Tile::Railing
                        {
                            self.action = None;
                        }
                    } else {
                        // The held direction can't be followed (a wall ahead,
                        // no ladder above/below): the gibbon stops and the
                        // player must press a direction again.
                        self.action = None;
                    }
                }
            }
            None => {
                self.step_gravity(&mut gibbon);
            }
        }

        self.gibbon = gibbon;
        self.collect_fruit();

        // Guards chase every sim tick, at the same constant speed as the
        // gibbon (one cell per SIM_FRAMES frames), so a guard never closes the
        // gap on a gibbon running away from it. Guard 0 minimises the vertical
        // distance, guard 1 the horizontal distance.
        for i in 0..self.guards.len() {
            let mut guard = self.guards[i];
            guard.settle();

            let falling = self.step_gravity(&mut guard);

            if !falling && let Some(dir) = self.guard_action(guard, i % 2 == 0) {
                let target = self.target_of(guard, dir);
                guard.move_to(target.x, target.y, dir);
            }

            self.guards[i] = guard;
        }

        // A guard that reaches the gibbon catches it.
        if self
            .guards
            .iter()
            .any(|g| g.x == self.gibbon.x && g.y == self.gibbon.y)
        {
            self.lose_life();
        }
    }

    /// The gibbon was caught or crushed: lose a life, pausing the action
    /// until the respawn timer runs out (or ending the run when lives run
    /// out).
    fn lose_life(&mut self) {
        self.lives -= 1;
        if self.lives > 0 {
            self.game_state = State::Dead;
            self.state_timer = DEAD_TICKS;
        } else {
            self.game_state = State::GameOver;
            self.state_timer = 0;
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
                self.game_state = State::Cleared;
                self.state_timer = CLEAR_TICKS;
            }
        }
    }

    // --- Movement helpers ---------------------------------------------------

    /// The tile at `(x, y)`; out-of-bounds reads as empty.
    fn tile(&self, x: i32, y: i32) -> Tile {
        if x >= 0 && y >= 0 && x < GRID_X as i32 && y < GRID_Y as i32 {
            self.level.tile(x as usize, y as usize)
        } else {
            Tile::Empty
        }
    }

    fn target_of(&self, actor: Actor, action: Action) -> Cell {
        match action {
            Action::Up => Cell {
                x: actor.x,
                y: actor.y - 1,
            },
            Action::Down => Cell {
                x: actor.x,
                y: actor.y + 1,
            },
            Action::Left => Cell {
                x: actor.x - 1,
                y: actor.y,
            },
            Action::Right => Cell {
                x: actor.x + 1,
                y: actor.y,
            },
            Action::DigLeft => Cell {
                x: actor.x,
                y: actor.y,
            },
            Action::DigRight => Cell {
                x: actor.x,
                y: actor.y,
            },
        }
    }

    /// Whether an actor can step from `actor` into `target`.
    fn can_enter(&self, actor: Cell, target: Cell, action: Action) -> bool {
        let current = self.tile(actor.x, actor.y);
        let target_tile = self.tile(target.x, target.y);
        match action {
            Action::Up => {
                target.y >= 0
                    && (target_tile == Tile::Ladder
                        || current == Tile::Ladder && !target_tile.is_solid())
            }
            Action::Down => target.y < GRID_Y as i32 && !target_tile.is_solid(),
            Action::Left | Action::Right => {
                target.x >= 0
                    && target.x < GRID_X as i32
                    && target.y >= 0
                    && target.y < GRID_Y as i32
                    && !target_tile.is_solid()
            }
            Action::DigLeft => true,
            Action::DigRight => true,
        }
    }

    /// Whether an actor in cell `(x, y)` is supported: solid ground below, a
    /// ladder directly below (standing on a ladder's top), or something to    
    /// hang from (railing in the current cell)
    fn supported(&self, x: i32, y: i32) -> bool {
        if self.tile(x, y + 1).is_solid() {
            return true;
        }

        matches!(self.tile(x, y), Tile::Ladder | Tile::Railing)
            || self.tile(x, y + 1) == Tile::Ladder
    }

    /// Apply one cell of gravity when the actor is unsupported. Returns
    /// whether the actor moved (fell).
    fn step_gravity(&self, actor: &mut Actor) -> bool {
        if actor.y >= GRID_Y as i32 - 1 {
            return false; // the bottom row is the floor
        }

        if !self.supported(actor.prev_x, actor.prev_y) && !self.supported(actor.x, actor.y) {
            actor.move_to(actor.x, actor.y + 1, Action::Down);
            return true;
        }
        false
    }

    /// Pick the next guard move: minimise the distance to the gibbon along
    /// the primary axis first, then along the other. Only moves that can
    /// actually be entered are considered, so a guard below the gibbon with
    /// no ladder to climb keeps closing the horizontal gap instead of
    /// freezing (a step up through plain air is not enterable).
    fn guard_action(&mut self, guard: Actor, primary_vertical: bool) -> Option<Action> {
        let pos = Cell {
            x: guard.x,
            y: guard.y,
        };

        let can_down = self.can_enter(pos, self.target_of(guard, Action::Down), Action::Down);
        let can_up = self.can_enter(pos, self.target_of(guard, Action::Up), Action::Up);
        let can_left = self.can_enter(pos, self.target_of(guard, Action::Left), Action::Left);
        let can_right = self.can_enter(pos, self.target_of(guard, Action::Right), Action::Right);

        if primary_vertical {
            // first minimize vertical
            if guard.y < self.gibbon.prev_y && can_down {
                Some(Action::Down)
            } else if guard.y > self.gibbon.prev_y && can_up {
                Some(Action::Up)
            } else if guard.x < self.gibbon.prev_x && can_right {
                Some(Action::Right)
            } else if guard.x > self.gibbon.prev_x && can_left {
                Some(Action::Left)
            } else {
                None
            }
        } else {
            if guard.x < self.gibbon.prev_x && can_right {
                Some(Action::Right)
            } else if guard.x > self.gibbon.prev_x && can_left {
                Some(Action::Left)
            } else if guard.y < self.gibbon.prev_y && can_down {
                Some(Action::Down)
            } else if guard.y > self.gibbon.prev_y && can_up {
                Some(Action::Up)
            } else {
                None
            }
        }
    }

    /// Regrow dug tiles whose timer expired, crushing anyone standing on the
    /// cell: the gibbon loses a life, a guard is sent back to its starting
    /// position.
    fn update_holes(&mut self) {
        for hole in &mut self.holes {
            if hole.ticks > 0 {
                hole.ticks -= 1;
            }
        }

        let mut regrown = Vec::new();
        self.holes.retain(|hole| {
            if hole.ticks > 0 {
                return true;
            }
            regrown.push((hole.x, hole.y));
            false
        });

        for &(x, y) in &regrown {
            if self.gibbon.x == x && self.gibbon.y == y {
                self.lose_life();
            }
            for i in 0..self.guards.len() {
                if self.guards[i].x == x && self.guards[i].y == y {
                    let (sx, sy) = self.level.guard_spawns[i];
                    self.guards[i] = Actor::at(sx as i32, sy as i32);
                }
            }
            self.level.set_tile(x as usize, y as usize, Tile::Wood);
        }
    }

    // --- Drawing ------------------------------------------------------------

    /// Draw the tiles, the fruits, the guards and the gibbon.
    pub fn draw(&self, r: &mut impl Renderer, frame: usize, sprites: &GameSprites) {
        self.draw_tiles(r);
        self.draw_fruits(r, sprites.fruit);

        for guard in &self.guards {
            self.draw_actor(r, *guard, frame, sprites.guard);
        }

        if self.game_state != State::Dead {
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
            if actor.facing < 0 { 2 } else { 0 }
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
        game.set_action(Some(Action::Up));
        advance(&mut game, 12);
        assert_eq!(game.gibbon.y, 0);
        // Climbing back down. The spawn cell itself is empty (`s`) and the
        // floor is below it, so Down steps off the bottom rung (y=8) onto the
        // spawn cell (y=9) standing on the floor and stops.
        game.set_action(Some(Action::Down));
        advance(&mut game, 12);
        assert_eq!(game.gibbon.y, 9);
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
        game.set_action(Some(Action::Right));
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
        game.set_action(Some(Action::Up));
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
        game.set_action(Some(Action::Right));
        advance(&mut game, 2);
        assert_eq!(game.gibbon.x, 0); // blocked by the brick
        assert_eq!(game.action, None); // blocked moves clear the direction
    }

    #[test]
    fn direction_latches_until_blocked() {
        // One Right press walks the gibbon all the way across the empty row
        // until the brick wall; no key needs to be held along the way.
        let mut game = game(vec![level(
            "s.................*.\n\
             ####################\n\
             ....................",
        )]);
        game.set_action(Some(Action::Right));
        advance(&mut game, 40);
        assert_eq!(game.gibbon.x, 17);
        assert_eq!(game.action, None); // stopped at the wall
    }

    #[test]
    fn moving_gibbon_keeps_interpolating() {
        // While running, the previous cell differs from the current one so
        // the renderer slides the gibbon between them.
        let mut game = game(vec![level(
            "s.................*.\n\
             ####################\n\
             ....................",
        )]);
        game.set_action(Some(Action::Right));
        advance(&mut game, 5);
        assert!(game.gibbon.x > 0);
        assert_ne!(
            (game.gibbon.prev_x, game.gibbon.prev_y),
            (game.gibbon.x, game.gibbon.y)
        );
    }

    #[test]
    fn stopped_gibbon_does_not_keep_interpolating() {
        // Once the gibbon is stopped, its previous cell equals its current
        // one, so the renderer shows it standing still.
        let mut game = game(vec![level(
            "s.................*.\n\
             ####################\n\
             ....................",
        )]);
        game.set_action(Some(Action::Right));
        advance(&mut game, 40);
        assert_eq!(game.gibbon.x, 17);
        assert_eq!((game.gibbon.prev_x, game.gibbon.prev_y), (17, 0));
    }

    #[test]
    fn chasing_guards_keep_interpolating() {
        // Guards move every sim tick, at the same speed as the gibbon, so
        // right after a tick a moving guard's previous cell differs from its
        // current one and the renderer slides it between them.
        let mut game = game(vec![level(
            "s..................g\n\
             ##..................\n\
             ....................",
        )]);
        advance(&mut game, 3);
        assert_eq!(game.guards.len(), 1);
        for guard in &game.guards {
            assert_ne!(
                (guard.prev_x, guard.prev_y),
                (guard.x, guard.y),
                "a chasing guard is mid-move, so it must keep interpolating"
            );
        }
    }

    #[test]
    fn pressing_down_without_a_ladder_stops() {
        // Solid floor below the spawn, no ladder: Down is blocked, so the
        // gibbon stops and the direction clears.
        let mut game = game(vec![level(
            "s...................g\n\
             ##..................\n\
             ....................",
        )]);
        game.set_action(Some(Action::Down));
        advance(&mut game, 1);
        assert_eq!(game.gibbon.y, 0);
        assert_eq!(game.action, None);
    }

    #[test]
    fn pressing_up_without_a_ladder_stops() {
        // Nothing climbable above the spawn: Up is blocked and clears.
        let mut game = game(vec![level(
            "s...................g\n\
             ##..................\n\
             ....................",
        )]);
        game.set_action(Some(Action::Up));
        advance(&mut game, 1);
        assert_eq!(game.gibbon.y, 0);
        assert_eq!(game.action, None);
    }

    #[test]
    fn climbing_past_the_top_rung_stops() {
        // Ladder shaft at x0 from the floor to the ceiling; the rung above
        // the top is out of bounds, so the Up latch clears at the top.
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
        game.set_action(Some(Action::Up));
        advance(&mut game, 12);
        assert_eq!(game.gibbon.y, 0);
        assert_eq!(game.action, None);
    }

    #[test]
    fn climbing_up_onto_the_top_of_the_ladder() {
        // Ladder at x0 with open space above its top rung: the gibbon climbs
        // up and stands on top of the ladder (the open cell above the top
        // rung) since nothing blocks it.
        let mut game = game(vec![level(
            "...................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             s...................g\n\
             ####################",
        )]);
        game.set_action(Some(Action::Up));
        advance(&mut game, 12);
        assert_eq!(game.gibbon.y, 0); // the open cell on top of the ladder
        assert_eq!(game.gibbon.x, 0);
        assert_eq!(game.action, None); // nothing climbable above
        // Supported by the ladder below: it does not fall back down.
        advance(&mut game, 5);
        assert_eq!(game.gibbon.y, 0);
        // It can still step off the top onto a neighbouring cell: with
        // nothing below the neighbour it falls, exactly like walking off any
        // other open edge.
        game.set_action(Some(Action::Right));
        advance(&mut game, 1);
        assert_eq!(game.gibbon.x, 1);
        // One tick later the fall is under way (the gravity check happens at
        // the start of the following tick, once the move has settled).
        advance(&mut game, 1);
        assert!(game.gibbon.y > 0);
    }

    #[test]
    fn falling_gibbon_does_not_drift_horizontally() {
        // Running right off the end of a railing, the gibbon falls straight
        // down at the same column instead of continuing to move right.
        let mut game = game(vec![level(
            "-..................g\n\
             s...................g\n\
             ....................",
        )]);
        game.set_action(Some(Action::Right));
        advance(&mut game, 4);
        assert_eq!(game.gibbon.x, 1);
        assert!(game.gibbon.y > 1);
        // Its direction survives the fall: it resumes once it lands.
        assert_eq!(game.action, Some(Action::Right));
    }

    #[test]
    fn pressing_down_jumps_off_a_ladder_into_open_air() {
        // Ladder ends above open air: Down leaves the ladder and the gibbon
        // keeps falling until it lands on the floor.
        let mut game = game(vec![level(
            "|..................g\n\
             |..................g\n\
             |..................g\n\
             s..................g\n\
             ....................\n\
             ....................\n\
             ....................\n\
             ....................\n\
             ....................\n\
             ....................\n\
             ####################",
        )]);
        // The spawn hangs from the ladder bottom (the ladder is right above).
        game.set_action(Some(Action::Down));
        advance(&mut game, 1);
        // It stepped off the ladder into the empty cell below it.
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 4));
        // No obstacle below: it keeps falling until the floor.
        advance(&mut game, 10);
        assert_eq!(game.gibbon.x, 0);
        assert_eq!(game.gibbon.y, GRID_Y as i32 - 2);
    }

    #[test]
    fn pressing_down_jumps_off_a_railing() {
        // Hanging on a railing with open air below, Down lets the gibbon drop.
        let mut game = game(vec![level(
            "-..................g\n\
             s..................g\n\
             ....................\n\
             ....................\n\
             ....................\n\
             ....................\n\
             ####################",
        )]);
        advance(&mut game, 1);
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 1)); // hanging
        game.set_action(Some(Action::Down));
        advance(&mut game, 2);
        assert_eq!(game.gibbon.x, 0);
        assert!(game.gibbon.y > 1, "it drops off the railing");
    }

    #[test]
    fn climbing_down_a_ladder_onto_a_railing_hangs_on_it() {
        // A railing below the ladder: pressing Down steps off the bottom rung
        // onto the railing and hangs there instead of climbing past it.
        let mut game = game(vec![level(
            "|..................g\n\
             |..................g\n\
             -..................g\n\
             ....................\n\
             ........s..........\n\
             ....................\n\
             ####################",
        )]);
        game.gibbon = Actor::at(0, 1); // bottom rung, railing below
        game.set_action(Some(Action::Down));
        advance(&mut game, 1);
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 2));
        assert_eq!(game.action, None);
        // It hangs on the railing and does not fall.
        advance(&mut game, 5);
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 2));
    }

    #[test]
    fn guards_step_off_a_ladder_bottom_to_chase_below() {
        // A guard on a ladder whose bottom rung is above open air now steps
        // down off it (Down is no longer blocked by plain air) to chase the
        // gibbon below.
        let mut game = game(vec![level(
            "|..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             ....................\n\
             s..................g\n\
             ####################",
        )]);
        game.gibbon = Actor::at(0, 5);
        game.guards = vec![Actor::at(0, 3)]; // bottom rung, open air below
        assert_eq!(game.guard_action(game.guards[0], true), Some(Action::Down));
        advance(&mut game, 1);
        assert_eq!((game.guards[0].x, game.guards[0].y), (0, 4));
    }

    #[test]
    fn digging_opens_a_hole_and_regrows_after_five_seconds() {
        // Gibbon stands on a wooden floor and digs the tile down-left.
        let mut game = game(vec![level(
            ".s..................g\n\
             ##..................\n\
             ....................",
        )]);
        assert_eq!(game.level.tile(0, 1), Tile::Wood);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        assert_eq!(game.level.tile(0, 1), Tile::Empty);
        assert_eq!(game.holes.len(), 1);
        // After the full dig time the wood regrows.
        advance(&mut game, DIG_TICKS + 1);
        assert_eq!(game.level.tile(0, 1), Tile::Wood);
        assert!(game.holes.is_empty());
    }

    #[test]
    fn regrowing_wood_crushes_a_gibbon_standing_in_it() {
        // A gibbon trapped in a dug cell when the wood regrows loses a life
        // and the level restarts (like being caught by a guard).
        let mut game = game(vec![level(
            ".s.................g\n\
             ##..................#\n\
             ####################",
        )]);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        assert_eq!(game.level.tile(0, 1), Tile::Empty);
        // Move the timer to just before regrow, then drop the gibbon into the
        // hole (it is supported by the floor row below it).
        advance(&mut game, DIG_TICKS - 1);
        game.gibbon = Actor::at(0, 1);
        let lives = game.lives;
        advance(&mut game, 1);
        assert_eq!(game.lives, lives - 1);
        assert_eq!(game.game_state, State::Dead);
        assert_eq!(game.level.tile(0, 1), Tile::Wood);
    }

    #[test]
    fn regrowing_wood_respawns_a_guard_standing_in_it() {
        // A guard standing in a dug cell when the wood regrows is sent back
        // to its starting position immediately.
        let mut game = game(vec![level(
            ".s.................g\n\
             ##..................#\n\
             ####################",
        )]);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        assert_eq!(game.level.tile(0, 1), Tile::Empty);
        // One tick before the wood regrows, stand a guard in the hole.
        advance(&mut game, DIG_TICKS - 1);
        game.guards = vec![Actor::at(0, 1)];
        game.update_holes();
        let (sx, sy) = game.level.guard_spawns[0];
        assert_eq!((game.guards[0].x, game.guards[0].y), (sx as i32, sy as i32));
        assert_eq!(game.level.tile(0, 1), Tile::Wood);
        assert!(game.holes.is_empty());
    }

    #[test]
    fn digging_works_while_airborne() {
        // No solid ground under the gibbon: it can still dig a wooden tile
        // diagonally below, spending the tick on digging instead of falling.
        let mut game = game(vec![level(
            ".s.................g\n\
             #.#...................",
        )]);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        assert_eq!(game.level.tile(0, 1), Tile::Empty);
        assert_eq!(game.holes.len(), 1);
        // The dig tick holds the fall: it has not left the start cell yet.
        assert_eq!((game.gibbon.x, game.gibbon.y), (1, 0));
    }

    #[test]
    fn digging_off_a_tile_digs_it_and_holds_the_fall() {
        // Stepping right off a wooden floor and digging left digs the tile
        // just left behind, even though the gibbon is airborne; the dig spends
        // the tick, so the fall only starts afterwards.
        let mut game = game(vec![level(
            "...s................\n\
             ####................\n\
             ....................",
        )]);
        // Step right off the wood at x3 onto the open cell at x4.
        game.set_action(Some(Action::Right));
        advance(&mut game, 1);
        assert_eq!((game.gibbon.x, game.gibbon.y), (4, 0));
        // Dig left while airborne: the wood just left behind (3,1) is dug.
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        assert_eq!(game.level.tile(3, 1), Tile::Empty);
        assert_eq!(game.holes.len(), 1);
        // It spent the tick digging, so it is still airborne at (4,0).
        assert_eq!((game.gibbon.x, game.gibbon.y), (4, 0));
        // With nothing to do, the fall resumes.
        advance(&mut game, 1);
        assert_eq!((game.gibbon.x, game.gibbon.y), (4, 1));
    }

    #[test]
    fn digging_targets_only_wood() {
        // The diagonal below-left is brick: nothing to dig.
        let mut game = game(vec![level(
            ".s..................g\n\
             *#..................\n\
             ....................",
        )]);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        assert!(game.holes.is_empty());
        assert_eq!(game.level.tile(0, 1), Tile::Brick);
    }

    #[test]
    fn digging_replaces_the_movement_command_and_stops() {
        // Moving left and digging: the dig command takes over from the held
        // movement direction, fires once from the current cell (down-left of
        // the landing cell) and resolves back to no action.
        let mut game = game(vec![level(
            "....s.................g\n\
             ####################\n\
             ....................",
        )]);
        game.set_action(Some(Action::Left));
        advance(&mut game, 2);
        assert_eq!(game.gibbon.x, 2);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        // It dug the tile down-left from (2,0) and stopped.
        assert_eq!((game.gibbon.x, game.gibbon.y), (2, 0));
        assert_eq!(game.level.tile(1, 1), Tile::Empty);
        assert_eq!(game.action, None);
        advance(&mut game, 2);
        assert_eq!((game.gibbon.x, game.gibbon.y), (2, 0));
    }

    #[test]
    fn collecting_all_fruits_clears_the_level() {
        // Fruit sits one cell right of the spawn.
        let mut game = game(vec![level(
            "s@..................g\n\
             ##..................\n\
             ....................",
        )]);
        game.set_action(Some(Action::Right));
        advance(&mut game, 1);
        assert_eq!(game.fruits_left, 0);
        assert_eq!(game.game_state, State::Cleared);
    }

    #[test]
    fn guards_keep_constant_distance_when_the_gibbon_runs_away() {
        // Gibbon at x4, guard at x0 on the same floor. Both move one cell per
        // sim tick (the same constant speed), so while the gibbon runs right
        // the guard never closes the gap.
        let mut game = game(vec![level(
            "g...s..............\n\
             ####################\n\
             ....................",
        )]);
        assert_eq!(game.gibbon.x, 4);
        assert_eq!(game.guards.len(), 1);
        assert_eq!(game.guards[0].x, 0);
        game.set_action(Some(Action::Right));
        for _ in 0..12 {
            let gap_before = game.gibbon.x - game.guards[0].x;
            advance(&mut game, 1);
            let gap_after = game.gibbon.x - game.guards[0].x;
            assert_eq!(gap_before, gap_after, "the guard keeps the same distance");
            assert_eq!(game.gibbon.x - 4, game.guards[0].x, "both gained one cell");
        }
    }

    #[test]
    fn guard_below_the_gibbon_walks_sideways_when_it_cannot_climb() {
        // A guard that fell below the gibbon with only air above it must not
        // freeze waiting for the gibbon to come down: it keeps minimising the
        // horizontal distance and walks toward the gibbon.
        let mut game = game(vec![level(
            "s..................g\n\
             ....................\n\
             ....................\n\
             ##..................\n\
             ....................\n",
        )]);
        game.gibbon = Actor::at(5, 0);
        game.guards = vec![Actor::at(0, 2)];
        assert_eq!(game.guard_action(game.guards[0], true), Some(Action::Right));
        // Walks right across the wood floor to the edge of it.
        advance(&mut game, 2);
        assert_eq!((game.guards[0].x, game.guards[0].y), (2, 2));
        // Past the edge there is nothing to stand on: gravity wins and it
        // drops straight down instead of drifting right.
        advance(&mut game, 2);
        assert_eq!(game.guards[0].x, 2);
        assert!(game.guards[0].y > 2);
        // Once it lands on the bottom row it resumes closing the horizontal
        // gap toward the gibbon.
        advance(&mut game, 8);
        assert_eq!(game.guards[0].y, GRID_Y as i32 - 1);
        assert!(game.guards[0].x > 2);
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
        assert_eq!(game.guard_action(game.guards[0], true), Some(Action::Up));
        // Guard 1 (horizontal priority): the gibbon is to its left on a clear
        // row, so it walks left first.
        assert_eq!(game.guard_action(game.guards[1], false), Some(Action::Left));
    }

    #[test]
    fn guards_catch_the_gibbon_and_it_respawns() {
        let mut game = game(vec![level(
            "s..................g\n\
             ##..................\n\
             ....................",
        )]);
        // A guard one cell right of the gibbon walks onto it and catches it.
        game.guards = vec![Actor::at(1, 0)];
        game.game_state = State::Playing;
        advance(&mut game, 1);
        assert_eq!(game.lives, LIVES - 1);
        assert_eq!(game.game_state, State::Dead);
        // After the death timer the gibbon is back at the spawn, guards reset.
        advance(&mut game, DEAD_TICKS);
        assert_eq!(game.game_state, State::Playing);
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
        game.guards = vec![Actor::at(1, 0)];
        advance(&mut game, 1);
        assert_eq!(game.game_state, State::GameOver);
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
        game.set_action(Some(Action::Right));
        advance(&mut game, CLEAR_TICKS + 2);
        assert_eq!(game.level_index, 1);
        assert_eq!(game.game_state, State::Playing);
        assert_eq!(game.fruits_left, 1);
    }

    #[test]
    fn completing_the_last_level_wins() {
        let mut game = game(vec![level(
            "s@..................g\n\
             ##..................\n\
             ....................",
        )]);
        game.set_action(Some(Action::Right));
        advance(&mut game, CLEAR_TICKS + 2);
        assert_eq!(game.game_state, State::Win);
    }
}
