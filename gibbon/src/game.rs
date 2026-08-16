//! The gibbon game: a Lode Runner-style grid game. The player controls a
//! gibbon that runs across floors, climbs ladders, hangs from railings and
//! digs through wooden floors to collect fruits. Two guards with different
//! chase priorities try to catch it; a level is complete once every fruit is
//! taken.

pub use crate::level::{GRID_X, GRID_Y, Level, Tile};
pub use crate::palette::{
    BG, BRICK, BRICK_DARK, GUARD_BODY, GUARD_DARK, GUARD_EYE, GUARD_FACE, HOLE, HOLE_EDGE, HUD,
    LADDER, LADDER_DARK, PLAYER_BODY, PLAYER_DARK, PLAYER_EYE, PLAYER_FACE, PLAYER2_BODY,
    PLAYER2_DARK, PLAYER2_FACE, WOOD, WOOD_DARK, WOOD_TOP,
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

/// The tile stays open for DIG_TICKS sim ticks: one sim tick lasts
const DIG_TICKS: usize = 10;

/// Starting lives.
const LIVES: i32 = 3;

/// Sim ticks spent showing "LEVEL CLEAR" before the next level loads.
const CLEAR_TICKS: usize = 10;

/// Sim ticks spent showing the gibbon was caught before it respawns.
const DEAD_TICKS: usize = 10;

/// Sim ticks spent showing "GAME OVER" before the whole game restarts.
const GAME_OVER_TICKS: usize = 10;

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

/// Sprite frame count of the walking sheets (gibbon_move_right.png,
/// guard_move_right.png): the walking animation facing right. The left-facing
/// frames are the same sprites flipped horizontally at load time.
pub const WALK_FRAMES: usize = 6;
/// Sprite frame count of the standing sheets (gibbon.png, guard.png): a
/// single standing pose.
pub const STAND_FRAMES: usize = 1;

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

/// A gibbon picked as a chase target, together with its player index so a
/// guard that reaches it can mark exactly that gibbon as caught.
#[derive(Clone, Copy)]
struct Target {
    actor: Actor,
    player: usize,
}

/// Phase of a dug wooden tile.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HolePhase {
    /// The wood is being destroyed: the pit is already open and the tile
    /// sinks away over SIM_FRAMES frames.
    Digging,
    /// The pit stays open while the wood regrows.
    Regrowing,
    /// The wood is growing back over SIM_FRAMES frames.
    Restoring,
}

/// A dug-out wooden tile.
struct Hole {
    x: i32,
    y: i32,
    phase: HolePhase,
    /// Frames of the dig-destruction animation left.
    frames: usize,
    /// Sim ticks until the wood regrows once the animation has finished.
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

/// The animated frames of one character, borrowed from the owning
/// [`CharacterSheets`]. The left-facing frames are the right-facing art
/// flipped horizontally, so a single walking sheet covers both directions.
pub struct CharacterSprites<'a> {
    /// Standing pose, facing right.
    pub stand: &'a RleSprite,
    /// Standing pose, facing left.
    pub stand_left: &'a RleSprite,
    /// Walking animation frames, facing right.
    pub walk_right: &'a [RleSprite],
    /// Walking animation frames, facing left.
    pub walk_left: &'a [RleSprite],
    /// Climbing pose; falls back to the standing pose when absent (the gibbon
    /// sheets hold no climbing art).
    pub climb: Option<&'a RleSprite>,
}

/// The decoded sheets behind one character's animation. The gibbon's standing
/// pose comes from gibbon.png and the walking animation from
/// gibbon_move_right.png (flipped for the left-facing frames); the guard
/// still uses the old combined 5-frame sheet, split into the same poses here.
pub struct CharacterSheets {
    /// Standing pose, facing right.
    pub stand: Vec<RleSprite>,
    /// Standing pose, facing left.
    pub stand_left: Vec<RleSprite>,
    /// Walking animation frames, facing right.
    pub walk_right: Vec<RleSprite>,
    /// Walking animation frames, facing left.
    pub walk_left: Vec<RleSprite>,
    /// Climbing pose (empty for the gibbon sheets, so climbing falls back to
    /// the standing pose).
    pub climb: Vec<RleSprite>,
}

impl CharacterSheets {
    /// Borrow the sheets as a [`CharacterSprites`] view for drawing.
    pub fn sprites(&self) -> CharacterSprites<'_> {
        CharacterSprites {
            stand: self
                .stand
                .first()
                .expect("a standing frame is always loaded"),
            stand_left: self
                .stand_left
                .first()
                .expect("a standing frame is always loaded"),
            walk_right: &self.walk_right,
            walk_left: &self.walk_left,
            climb: self.climb.first(),
        }
    }
}

/// Sprite sheets handed to [`Game::draw`], decoded by the playing scene.
pub struct GameSprites<'a> {
    pub fruit: &'a [RleSprite],
    pub gibbon: CharacterSprites<'a>,
    /// The second player's gibbon (green).
    pub gibbon2: CharacterSprites<'a>,
    pub guard: CharacterSprites<'a>,
    /// Wood wall sheet: frame 0 is the intact wall, the last frame the
    /// completely destroyed wood left behind in the dug cell.
    pub wood: &'a [RleSprite],
    /// Ladder sheet: a single frame drawn for every ladder rung.
    pub ladder: &'a [RleSprite],
    /// Stone wall sheet: a single frame drawn for every unbreakable brick
    /// tile.
    pub stone: &'a [RleSprite],
}

/// The game owns the level, the gibbon, the guards, the dug holes and the
/// shared tick clock.
pub struct Game {
    /// All embedded levels; the game advances through them in order.
    levels: Vec<Level>,

    pub level: Level,
    /// Player one's gibbon. When caught it stops moving and is not drawn
    /// until the level is cleared or both gibbons are caught and a life is
    /// lost.
    pub gibbon: Actor,
    /// Player two's gibbon, with its own latched command.
    pub gibbon2: Actor,
    /// Whether each gibbon was caught by a guard this life. A single catch is
    /// not fatal: the remaining gibbon can still clear the level and both
    /// come back on the next level. A life is only lost when both are caught.
    pub caught: [bool; 2],
    pub guards: Vec<Actor>,

    /// The current command for player one. Movement latches: the gibbon
    /// keeps going in that direction until the way is blocked (a failed move
    /// clears it), so the key need not be held. A dig command fires once and
    /// resolves to `None`.
    pub action: Option<Action>,
    /// The current command for player two.
    pub action2: Option<Action>,

    holes: Vec<Hole>,

    /// Frame counter within a move (0..SIM_FRAMES), for interpolation.
    pub frame_cnt: usize,

    tick: usize,

    pub game_state: State,

    state_timer: usize,

    /// How many players are playing (1 or 2). With one player only player
    /// one's gibbon exists: the second gibbon does not move, collect fruit or
    /// get drawn, and being caught costs a life directly.
    pub players: usize,

    pub lives: i32,
    pub level_index: usize,
    pub fruits_left: usize,
}

/// A minimal built-in level, used when no level files are embedded.
const FALLBACK: &str = "|..@...........@....\n\
                        |===================\n\
                        |...................\n\
                        |........|..........\n\
                        |........|.=========\n\
                        |........|..........\n\
                        |........|..........\n\
                        |.....|..|..........\n\
                        |.....|..|..........\n\
                        ..s......|....g....g\n\
                        ====================";

impl Game {
    /// Load every embedded level and start the first one.
    /// Start the embedded levels with the given number of players (1 or 2).
    pub fn new(players: usize) -> Game {
        let mut levels = crate::level::load_all();
        if levels.is_empty() {
            levels.push(crate::level::parse(FALLBACK).expect("fallback level parses"));
        }
        let mut game = Game::from_levels(levels);
        game.players = players.clamp(1, 2);
        game
    }

    /// Build a game from an explicit level list (used by tests); defaults to
    /// two players.
    pub fn from_levels(levels: Vec<Level>) -> Game {
        let mut game = Game {
            levels,
            level: Level::default(),
            gibbon: Actor::at(0, 0),
            gibbon2: Actor::at(0, 0),
            caught: [false; 2],
            guards: Vec::new(),
            action: None,
            action2: None,
            holes: Vec::new(),
            frame_cnt: 0,
            tick: 0,
            game_state: State::Playing,
            state_timer: 0,
            players: 2,
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
        self.gibbon2 = Actor::at(self.level.spawn.0 as i32, self.level.spawn.1 as i32);
        self.caught = [false; 2];
        let mut guards: Vec<Actor> = self
            .level
            .guard_spawns
            .iter()
            .map(|&(x, y)| Actor::at(x as i32, y as i32))
            .collect();
        // The chase logic assumes two guards; with a single spawn point both
        // guards start from that cell.
        if guards.len() == 1 {
            guards.push(guards[0]);
        }
        self.guards = guards;
        self.holes.clear();
        self.action = None;
        self.action2 = None;
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

    /// Set player two's held direction, mirroring [`Game::set_action`].
    pub fn set_action2(&mut self, action: Option<Action>) {
        self.action2 = action;
    }

    /// Perform a queued dig from `actor`'s cell, returning whether a hole was
    /// actually dug.
    fn perform_dig(&mut self, side: i32, actor: Actor) -> bool {
        let (x, y) = (actor.x, actor.y);
        let (tx, ty) = (x + side, y + 1);
        if tx < 0 || tx >= GRID_X as i32 || ty < 0 || ty >= GRID_Y as i32 {
            return false;
        }
        // A wall or wood right above the target blocks the dig.
        if self.tile(x + side, y).is_solid() {
            return false;
        }
        if self.level.tile(tx as usize, ty as usize) != Tile::Wood {
            return false;
        }
        self.level.set_tile(tx as usize, ty as usize, Tile::Empty);
        self.holes.push(Hole {
            x: tx,
            y: ty,
            phase: HolePhase::Digging,
            frames: SIM_FRAMES,
            ticks: DIG_TICKS,
        });
        true
    }

    /// Advance the hole animations by one frame: the dug tile sinks away over
    /// SIM_FRAMES frames, and later the regrown wood rises back over the same
    /// number of frames. Only once the restoration is complete is the cell
    /// turned back into wood and anyone standing on it crushed.
    fn animate_digs(&mut self) {
        let mut regrown = Vec::new();
        for (i, hole) in self.holes.iter_mut().enumerate() {
            match hole.phase {
                HolePhase::Digging => {
                    hole.frames -= 1;
                    if hole.frames == 0 {
                        hole.phase = HolePhase::Regrowing;
                    }
                }
                HolePhase::Restoring => {
                    hole.frames -= 1;
                    if hole.frames == 0 {
                        regrown.push((i, hole.x, hole.y));
                    }
                }
                HolePhase::Regrowing => {}
            }
        }

        for &(i, _, _) in regrown.iter().rev() {
            self.holes.remove(i);
        }

        for &(_, x, y) in &regrown {
            // A gibbon standing on the cell when the wood closes back in is
            // crushed: this is an instant death, unlike a guard catch, so it
            // always costs a life.
            if self.game_state == State::Playing
                && ((self.gibbon.x == x && self.gibbon.y == y)
                    || (self.players == 2 && self.gibbon2.x == x && self.gibbon2.y == y))
            {
                self.lose_life();
            }
            for i in 0..self.guards.len() {
                if self.guards[i].x == x && self.guards[i].y == y {
                    let (sx, sy) = self.level.guard_spawns[i % self.level.guard_spawns.len()];
                    self.guards[i] = Actor::at(sx as i32, sy as i32);
                }
            }
            self.level.set_tile(x as usize, y as usize, Tile::Wood);
        }
    }

    /// Advance one fixed simulation step.
    pub fn step(&mut self) {
        self.frame_cnt += 1;
        self.animate_digs();

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
                    State::GameOver => self.restart(),
                    State::Playing | State::Win => {}
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
            if self.players == 2 {
                self.gibbon2.settle();
            }
            for guard in &mut self.guards {
                guard.settle();
            }
        }
    }

    /// Advance one gibbon for its latched command, returning it together with
    /// the command it latches for the next tick. The movement rules apply to
    /// both gibbons identically.
    fn step_gibbon(
        &mut self,
        mut gibbon: Actor,
        mut action: Option<Action>,
    ) -> (Actor, Option<Action>) {
        gibbon.settle();

        match action {
            // A dig command fires from the current cell even while airborne
            // (stepping off a tile lets you dig it). Only a successful dig
            // spends the whole tick and holds the fall back; with nothing to
            // dig it is a no-op and the fall continues. It is not latched
            // like movement, so it resolves back to no action.
            Some(Action::DigLeft) => {
                if !self.perform_dig(-1, gibbon) {
                    self.step_gravity(&mut gibbon);
                }
                action = None;
            }
            Some(Action::DigRight) => {
                if !self.perform_dig(1, gibbon) {
                    self.step_gravity(&mut gibbon);
                }
                action = None;
            }
            Some(cmd) => {
                let falling = self.step_gravity(&mut gibbon);

                if !falling {
                    let target = self.target_of(gibbon, cmd);

                    if self.can_enter(
                        Cell {
                            x: gibbon.x,
                            y: gibbon.y,
                        },
                        target,
                        cmd,
                    ) {
                        gibbon.move_to(target.x, target.y, cmd);
                        // Stepping down off a ladder onto a railing hangs the
                        // gibbon: it stops there instead of climbing straight
                        // past it.
                        if cmd == Action::Down && self.tile(target.x, target.y) == Tile::Railing {
                            action = None;
                        }
                    } else {
                        // The held direction can't be followed (a wall ahead,
                        // no ladder above/below): the gibbon stops and the
                        // player must press a direction again.
                        action = None;
                    }
                }
            }
            None => {
                self.step_gravity(&mut gibbon);
            }
        }

        (gibbon, action)
    }

    fn tick_playing(&mut self) {
        // Each player moves their gibbon; a caught gibbon is out for this
        // life and neither moves nor collects fruit.
        if !self.caught[0] {
            let (gibbon, action) = self.step_gibbon(self.gibbon, self.action);
            self.gibbon = gibbon;
            self.action = action;
        }
        if self.players == 2 && !self.caught[1] {
            let (gibbon, action) = self.step_gibbon(self.gibbon2, self.action2);
            self.gibbon2 = gibbon;
            self.action2 = action;
        }

        self.collect_fruit();

        // Guards chase every sim tick, at the same constant speed as the
        // gibbon (one cell per SIM_FRAMES frames), so a guard never closes the
        // gap on a gibbon running away from it. Each guard heads for the
        // closest gibbon that is still free; guard 0 minimises the vertical
        // distance, guard 1 the horizontal distance.
        for i in 0..self.guards.len() {
            let mut guard = self.guards[i];
            guard.settle();

            let falling = self.step_gravity(&mut guard);

            if !falling
                && let Some(target) = self.closest_gibbon(guard)
                && let Some(dir) = self.guard_action(guard, i % 2 == 0, target.actor)
            {
                let target_cell = self.target_of(guard, dir);
                guard.move_to(target_cell.x, target_cell.y, dir);
                // A guard reaching its target's cell catches that gibbon.
                if guard.x == target.actor.x && guard.y == target.actor.y {
                    self.caught[target.player] = true;
                }
            }

            self.guards[i] = guard;
        }

        // A guard that lands on a gibbon (e.g. one dropping from a ladder)
        // catches it. With two players a life is only lost when both gibbons
        // are caught; with one player the single gibbon costs a life on its
        // own.
        for p in 0..2 {
            if !self.gibbon_active(p) || self.caught[p] {
                continue;
            }
            let gibbon = self.gibbon_of(p);
            if self
                .guards
                .iter()
                .any(|g| g.x == gibbon.x && g.y == gibbon.y)
            {
                self.caught[p] = true;
            }
        }
        if self.caught[0] && (self.players == 1 || self.caught[1]) {
            self.lose_life();
        }
    }

    /// The gibbon was caught or crushed: lose a life, pausing the action
    /// until the respawn timer runs out (or restarting the whole game when
    /// lives run out).
    fn lose_life(&mut self) {
        self.lives -= 1;
        if self.lives > 0 {
            self.game_state = State::Dead;
            self.state_timer = DEAD_TICKS;
        } else {
            self.game_state = State::GameOver;
            self.state_timer = GAME_OVER_TICKS;
        }
    }

    /// Respawn the gibbon (and reset the guards) after being caught.
    fn respawn(&mut self) {
        self.start_level(self.level_index);
        self.lives = self.lives.max(1);
    }

    /// Whether a gibbon takes part in this game: player one always does, the
    /// second only in a two-player game.
    fn gibbon_active(&self, player: usize) -> bool {
        player == 0 || self.players == 2
    }

    fn collect_fruit(&mut self) {
        for p in 0..2 {
            if !self.gibbon_active(p) || self.caught[p] {
                continue;
            }
            let gibbon = self.gibbon_of(p);
            let (x, y) = (gibbon.x as usize, gibbon.y as usize);
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
                // Climbing up needs a ladder or railing in the current cell.
                // The cell above decides what is possible: a ladder rung is
                // always climbable, a railing only when it continues as a
                // ladder above (climbing up onto a bare railing is
                // impossible), open air only from the top of a ladder (a bare
                // railing is not enough to pull up into the air), and a solid
                // cell (wood or brick) above always blocks the climb.
                target.y >= 0
                    && matches!(current, Tile::Ladder | Tile::Railing)
                    && !target_tile.is_solid()
                    && if target_tile == Tile::Ladder {
                        true
                    } else if target_tile == Tile::Railing {
                        self.tile(target.x, target.y - 1) == Tile::Ladder
                    } else {
                        current == Tile::Ladder
                    }
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
    /// ladder directly below (standing on a ladder's top), or a ladder or
    /// railing in the current cell (hanging from a railing or standing in a
    /// ladder). Nothing above provides support: the gibbon does not hang from
    /// a ladder or railing in the cell above it, so it starts falling.
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
        // Falling off the bottom row wraps around to the top of the board.
        if actor.y >= GRID_Y as i32 - 1 {
            if !self.supported(actor.prev_x, actor.prev_y) && !self.supported(actor.x, actor.y) {
                actor.move_to(actor.x, 0, Action::Down);
                return true;
            }
            return false; // resting on the bottom row
        }

        if !self.supported(actor.prev_x, actor.prev_y) && !self.supported(actor.x, actor.y) {
            actor.move_to(actor.x, actor.y + 1, Action::Down);
            return true;
        }
        false
    }

    /// Pick the next guard move: minimise the distance to `target` along the
    /// primary axis first, then along the other. Only moves that can actually
    /// be entered are considered, so a guard below the gibbon with no ladder
    /// to climb keeps closing the horizontal gap instead of freezing (a step
    /// up through plain air is not enterable).
    fn guard_action(&self, guard: Actor, primary_vertical: bool, target: Actor) -> Option<Action> {
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
            if guard.y < target.prev_y && can_down {
                Some(Action::Down)
            } else if guard.y > target.prev_y && can_up {
                Some(Action::Up)
            } else if guard.x < target.prev_x && can_right {
                Some(Action::Right)
            } else if guard.x > target.prev_x && can_left {
                Some(Action::Left)
            } else {
                None
            }
        } else {
            if guard.x < target.prev_x && can_right {
                Some(Action::Right)
            } else if guard.x > target.prev_x && can_left {
                Some(Action::Left)
            } else if guard.y < target.prev_y && can_down {
                Some(Action::Down)
            } else if guard.y > target.prev_y && can_up {
                Some(Action::Up)
            } else {
                None
            }
        }
    }

    /// The gibbon for a player index, or the first gibbon for any other index
    /// (used by the crush and catch checks).
    fn gibbon_of(&self, player: usize) -> Actor {
        if player == 0 {
            self.gibbon
        } else {
            self.gibbon2
        }
    }

    /// The closest gibbon that is not yet caught, with its player index, so
    /// both guards can be told which target to chase. `None` when both are
    /// caught (the game is about to lose a life anyway).
    fn closest_gibbon(&self, guard: Actor) -> Option<Target> {
        let targets = [
            Target {
                actor: self.gibbon,
                player: 0,
            },
            Target {
                actor: self.gibbon2,
                player: 1,
            },
        ];
        targets
            .into_iter()
            .filter(|t| self.gibbon_active(t.player) && !self.caught[t.player])
            .min_by_key(|t| (t.actor.x - guard.x).abs() + (t.actor.y - guard.y).abs())
    }

    /// Regrow dug tiles whose timer expired, crushing anyone standing on the
    /// cell: the gibbon loses a life, a guard is sent back to its starting
    /// position.
    /// Count down the regrow timer: once it expires the wood starts growing
    /// back over SIM_FRAMES frames (the crush happens when that finishes, in
    /// [`Game::animate_digs`]).
    fn update_holes(&mut self) {
        for hole in &mut self.holes {
            if hole.phase == HolePhase::Regrowing && hole.ticks > 0 {
                hole.ticks -= 1;
                if hole.ticks == 0 {
                    hole.phase = HolePhase::Restoring;
                    hole.frames = SIM_FRAMES;
                }
            }
        }
    }

    // --- Drawing ------------------------------------------------------------

    /// Draw the tiles, the fruits, the guards and the gibbon.
    pub fn draw(&self, r: &mut impl Renderer, frame: usize, sprites: &GameSprites) {
        self.draw_tiles(r, sprites.wood, sprites.ladder, sprites.stone);
        self.draw_fruits(r, sprites.fruit);

        for guard in &self.guards {
            self.draw_actor(r, *guard, frame, &sprites.guard);
        }

        if self.game_state != State::Dead {
            if !self.caught[0] {
                self.draw_actor(r, self.gibbon, frame, &sprites.gibbon);
            }
            if self.players == 2 && !self.caught[1] {
                self.draw_actor(r, self.gibbon2, frame, &sprites.gibbon2);
            }
        }
    }

    fn draw_tiles(
        &self,
        r: &mut impl Renderer,
        wood: &[RleSprite],
        ladder: &[RleSprite],
        stone: &[RleSprite],
    ) {
        for y in 0..GRID_Y {
            for x in 0..GRID_X {
                let (px, py) = cell_screen(x as i32, y as i32);
                match self.level.tile(x, y) {
                    Tile::Wood => draw_wood_frame(r, wood, px, py, 0),
                    Tile::Brick => match stone.first() {
                        Some(frame) => frame.draw(r, px, py),
                        None => draw_brick(r, px, py),
                    },
                    Tile::Ladder => match ladder.first() {
                        Some(frame) => frame.draw(r, px, py),
                        None => draw_ladder(r, px, py),
                    },
                    Tile::Railing => draw_railing(r, px, py),
                    Tile::Empty | Tile::Fruit => {}
                }
            }
        }
        for hole in &self.holes {
            let (px, py) = cell_screen(hole.x, hole.y);
            let index = match hole.phase {
                HolePhase::Digging => destroy_frame(hole.frames, wood.len()),
                HolePhase::Restoring => restore_frame(hole.frames, wood.len()),
                HolePhase::Regrowing => wood.len().saturating_sub(1),
            };
            if wood.is_empty() {
                // Procedural fallback: the pit opens from the top of the cell
                // and the remaining wood drops down, then rises back.
                match hole.phase {
                    HolePhase::Digging => {
                        let k = (SIM_FRAMES - hole.frames) * CELL as usize / SIM_FRAMES;
                        draw_digging(r, px, py, k as i32);
                    }
                    HolePhase::Restoring => {
                        let k = hole.frames * CELL as usize / SIM_FRAMES;
                        draw_digging(r, px, py, k as i32);
                    }
                    HolePhase::Regrowing => draw_hole(r, hole.x, hole.y),
                }
            } else {
                // The completely destroyed wood stays visible in the place
                // where it was dug, with the open pit showing beneath it.
                draw_hole(r, hole.x, hole.y);
                wood[index].draw(r, px, py);
            }
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

    fn draw_actor(
        &self,
        r: &mut impl Renderer,
        actor: Actor,
        frame: usize,
        sprites: &CharacterSprites,
    ) {
        let (px, py) = interpolate(actor, frame);
        let sprite = actor_sprite(actor, frame, sprites);
        sprite.draw(r, px, py);
        // Wrapping from the bottom row to the top: while the first copy
        // slides out past the bottom edge, draw it again at the top of
        // the board, shifted by one board height.
        if actor.prev_y == GRID_Y as i32 - 1 && actor.y == 0 {
            sprite.draw(r, px, py - BOARD_PY);
        }
    }
}

/// The sprite to draw for `actor` at interpolation frame `frame`: the
/// climbing pose during vertical movement, a walking-animation frame while
/// walking horizontally, and the standing pose when resting.
fn actor_sprite<'a>(
    actor: Actor,
    frame: usize,
    sprites: &'a CharacterSprites<'a>,
) -> &'a RleSprite {
    if actor.prev_y != actor.y {
        sprites
            .climb
            .unwrap_or_else(|| facing_stand(actor, sprites))
    } else if actor.prev_x != actor.x {
        let frames = if actor.facing < 0 {
            sprites.walk_left
        } else {
            sprites.walk_right
        };
        if frames.is_empty() {
            facing_stand(actor, sprites)
        } else {
            &frames[walk_frame(frame, frames.len())]
        }
    } else {
        facing_stand(actor, sprites)
    }
}

/// The standing pose for the actor's facing direction.
fn facing_stand<'a>(actor: Actor, sprites: &'a CharacterSprites<'a>) -> &'a RleSprite {
    if actor.facing < 0 {
        sprites.stand_left
    } else {
        sprites.stand
    }
}

/// Walking-animation frame index for interpolation frame `frame`: the
/// animation cycles once per cell crossed, spreading its frames evenly across
/// the SIM_FRAMES-frame move.
fn walk_frame(frame: usize, count: usize) -> usize {
    frame * count / SIM_FRAMES
}

impl Default for Game {
    fn default() -> Game {
        Game::new(2)
    }
}

/// Interpolated screen top-left of an actor during a move tick.
fn interpolate(actor: Actor, frame: usize) -> (i32, i32) {
    let (cx, cy) = cell_screen(actor.x, actor.y);

    if actor.prev_x == actor.x && actor.prev_y == actor.y {
        return (cx, cy);
    }

    let (px, py) = cell_screen(actor.prev_x, actor.prev_y);

    // Falling off the bottom row wraps to the top: slide the first copy out
    // past the bottom edge instead of snapping back to the top, and let the
    // renderer draw a second copy entering at the top.
    if actor.prev_y == GRID_Y as i32 - 1 && actor.y == 0 {
        return (interp(px, cx, frame), interp(py, BOARD_Y + BOARD_PY, frame));
    }

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

/// Draw the wood sprite at `index` of the sheet; with an empty sheet falls
/// back to the procedural wood tile.
fn draw_wood_frame(r: &mut impl Renderer, wood: &[RleSprite], x: i32, y: i32, index: usize) {
    match wood.get(index) {
        Some(frame) => frame.draw(r, x, y),
        None => draw_wood(r, x, y),
    }
}

/// Wood-sheet index for the dig animation: frame 0 (the intact wall) is shown
/// first and the last frame (the completely destroyed wood) last, as the
/// SIM_FRAMES destruction frames elapse.
fn destroy_frame(frames_left: usize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let elapsed = SIM_FRAMES.saturating_sub(frames_left).min(SIM_FRAMES);
    elapsed * (count - 1) / SIM_FRAMES
}

/// Wood-sheet index for the regrow animation: the destroyed last frame is
/// shown first and the intact frame 0 last, as the wood grows back.
fn restore_frame(frames_left: usize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    frames_left.min(SIM_FRAMES) * (count - 1) / SIM_FRAMES
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

/// Draw a wooden tile sinking away: the pit opens from the top of the cell
/// and the remaining wood drops down over the dig animation.
fn draw_digging(r: &mut impl Renderer, x: i32, y: i32, k: i32) {
    draw_wood(r, x, y);
    r.fill_rect(x, y, CELL, k, HOLE);
    r.fill_rect(x, y + k, CELL, 1, HOLE_EDGE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::color::{Color, Palette};
    use engine::render::Framebuffer;
    use engine::sprites::SpriteSheet;

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
        // Whole level empty: the gibbon drops to the bottom row, then wraps
        // around to the top and keeps falling.
        let mut game = game(vec![level(
            "s...................g\n\
             ....................",
        )]);
        assert_eq!(game.gibbon.y, 0);
        advance(&mut game, 10);
        assert_eq!(game.gibbon.y, GRID_Y as i32 - 1);
        assert_eq!(game.gibbon.x, 0);
        // One more tick wraps it around to the top of the board.
        advance(&mut game, 1);
        assert_eq!(game.gibbon.y, 0);
        assert_eq!(game.gibbon.prev_y, GRID_Y as i32 - 1);
        // And it keeps falling down from the top.
        advance(&mut game, 1);
        assert_eq!(game.gibbon.y, 1);
    }

    #[test]
    fn falling_off_the_bottom_wraps_to_the_top() {
        // An open column with no ground: the gibbon drops off the bottom row
        // and re-enters at the top, one cell per sim tick.
        let mut game = game(vec![level(
            ".s..................\n\
             ....................",
        )]);
        assert_eq!(game.gibbon.y, 0);
        advance(&mut game, GRID_Y as usize - 1);
        assert_eq!(game.gibbon.y, GRID_Y as i32 - 1);
        // The wrap tick re-enters at the top, keeping the bottom row as the
        // previous cell so the renderer can slide it across the seam.
        advance(&mut game, 1);
        assert_eq!(game.gibbon.y, 0);
        assert_eq!(game.gibbon.prev_y, GRID_Y as i32 - 1);
        assert_eq!(game.gibbon.x, 1);
    }

    #[test]
    fn a_guard_falling_off_the_bottom_wraps_to_the_top() {
        // Guards wrap the same way the gibbon does: falling off the bottom
        // row re-enters them at the top.
        let mut game = game(vec![level(
            "s..................g\n\
             ....................",
        )]);
        advance(&mut game, GRID_Y as usize - 1);
        assert_eq!(game.guards[0].y, GRID_Y as i32 - 1);
        advance(&mut game, 1);
        assert_eq!(game.guards[0].y, 0);
        assert_eq!(game.guards[0].prev_y, GRID_Y as i32 - 1);
    }

    #[test]
    fn wrapping_interpolation_exits_the_bottom_edge() {
        // During a wrap move the first copy slides out past the bottom edge
        // instead of snapping back to the top row.
        let actor = Actor {
            x: 1,
            y: 0,
            prev_x: 1,
            prev_y: GRID_Y as i32 - 1,
            facing: 1,
        };
        let (_, bottom) = cell_screen(1, GRID_Y as i32 - 1);
        assert_eq!(interpolate(actor, 0), (cell_screen(1, 0).0, bottom));

        let (px, py) = interpolate(actor, SIM_FRAMES - 1);
        assert_eq!(px, cell_screen(1, 0).0);
        assert!(py > bottom, "the exiting copy slides past the bottom edge");
        // The entering copy sits one board height above, starting above the
        // top row.
        assert!(py - BOARD_PY < cell_screen(1, 0).1);
    }

    #[test]
    fn gibbon_stops_on_solid_ground() {
        // A floor directly under the spawn: no fall.
        let mut game = game(vec![level(
            "s...................g\n\
             ==..................\n\
             ....................",
        )]);
        advance(&mut game, 10);
        assert_eq!(game.gibbon, Actor::at(0, 0));
    }

    #[test]
    fn gibbon_climbs_ladders() {
        // Ladder shaft at x0, spawn beside the bottom rung.
        let mut game = game(vec![level(
            "|....................\n\
             |....................\n\
             |....................\n\
             |....................\n\
             |....................\n\
             |....................\n\
             |....................\n\
             |....................\n\
             |....................\n\
             |s...................\n\
             ====================",
        )]);
        // Step onto the ladder, then climb up to the top.
        game.set_action(Some(Action::Left));
        advance(&mut game, 1);
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 9));
        game.set_action(Some(Action::Up));
        advance(&mut game, 12);
        assert_eq!(game.gibbon.y, 0);
        // Climbing back down: the floor below the bottom rung stops it on the
        // ladder, and stepping off brings it back onto the spawn cell.
        game.set_action(Some(Action::Down));
        advance(&mut game, 12);
        assert_eq!(game.gibbon.y, 9);
        game.set_action(Some(Action::Right));
        advance(&mut game, 1);
        assert_eq!(game.gibbon.x, 1);
    }

    #[test]
    fn a_wood_cap_above_the_ladder_blocks_climbing() {
        // A solid wood cell directly above the ladder's top rung plugs the
        // shaft: the gibbon cannot climb up into it.
        let mut game = game(vec![level(
            "=....................\n\
             |....................\n\
             |....................\n\
             |....................\n\
             |....................\n\
             |....................\n\
             |....................\n\
             |....................\n\
             |....................\n\
             |s...................\n\
             ====================",
        )]);
        game.set_action(Some(Action::Left));
        advance(&mut game, 1);
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 9));
        game.set_action(Some(Action::Up));
        advance(&mut game, 20);
        // It stops on the rung just below the wood instead of climbing into
        // the solid cell.
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 1));
        assert_eq!(game.action, None);
    }

    #[test]
    fn gibbon_hangs_on_a_railing() {
        // Railing next to the gibbon.
        let mut game = game(vec![level(
            "....................\n\
             s-..................\n\
             *...................",
        )]);
        // move onto the railing to hang on there
        game.set_action(Some(Action::Right));
        advance(&mut game, 1);
        game.set_action(None);
        // we must still be on the railing
        advance(&mut game, 10);

        assert_eq!(game.gibbon.y, 1);

        // Stepping off the end of the railing makes it fall; ten ticks land it
        // on the bottom row just before the wrap tick loops it back to the top.
        game.set_action(Some(Action::Right));
        advance(&mut game, 10);
        assert_eq!(game.gibbon.y, GRID_Y as i32 - 1);
    }

    #[test]
    fn gibbon_cannot_climb_up_railing_without_ladder() {
        // Railing next to the gibbon.
        let mut game = game(vec![level(
            "....................\n\
             s-..................\n\
             *...................",
        )]);

        // move onto the railing to hang on there
        game.set_action(Some(Action::Right));
        advance(&mut game, 1);
        game.set_action(None);
        // we must still be on the railing
        advance(&mut game, 10);

        assert_eq!(game.gibbon.y, 1);

        // try climbing up fails
        game.set_action(Some(Action::Up));
        advance(&mut game, 1);

        assert_eq!(game.gibbon.y, 1);
    }

    #[test]
    fn gibbon_does_not_hang_on_a_railing_that_is_above_him() {
        // Railing above the gibbon.
        let mut game = game(vec![level(
            "-................\n\
             s................\n\
             .................\n\
             *................\n\
             .*...............",
        )]);
        assert_eq!(game.gibbon.y, 1);

        game.set_action(None);
        advance(&mut game, 10);

        assert_eq!(game.gibbon.y, 2);
    }

    #[test]
    fn climbing_up_onto_a_bare_railing_is_allowed() {
        // Ladder below a railing with nothing above it: the gibbon climbs to
        // the top rung but cannot climb up onto the railing.
        let mut game = game(vec![level(
            "-..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |s.................g\n\
             ====================",
        )]);
        game.set_action(Some(Action::Left));
        advance(&mut game, 1);
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 9));

        game.set_action(Some(Action::Up));
        advance(&mut game, 12);

        // It stops on the top rung: the railing above it has no ladder above.
        assert_eq!(game.gibbon.y, 0);
        assert_eq!(game.gibbon.x, 0);
        assert_eq!(game.action, None);
    }

    #[test]
    fn climbing_up_through_a_railing_needs_a_ladder_above() {
        // Ladder shaft with a railing halfway up: a ladder above the railing
        // lets the gibbon climb straight through it to the top.
        let mut game = game(vec![level(
            "|..................g\n\
             |..................g\n\
             -..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             |s.................g\n\
             ====================",
        )]);
        game.set_action(Some(Action::Left));
        advance(&mut game, 1);
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 9));
        game.set_action(Some(Action::Up));
        advance(&mut game, 12);
        // It climbs through the railing (which has a ladder above it) to the
        // top of the shaft.
        assert_eq!(game.gibbon.y, 0);
        assert_eq!(game.gibbon.x, 0);
        assert_eq!(game.action, None);
    }

    #[test]
    fn walls_block_horizontal_movement() {
        let mut game = game(vec![level(
            "s*..................g\n\
             ==..................\n\
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
             ====================\n\
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
             ====================\n\
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
             ====================\n\
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
             ==..................\n\
             ....................",
        )]);
        advance(&mut game, 3);
        assert_eq!(game.guards.len(), 2);
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
             ==..................\n\
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
             ==..................\n\
             ....................",
        )]);
        game.set_action(Some(Action::Up));
        advance(&mut game, 1);
        assert_eq!(game.gibbon.y, 0);
        assert_eq!(game.action, None);
    }

    #[test]
    fn climbing_past_the_top_rung_stops() {
        // Ladder shaft at x0 from the floor to the ceiling; the gibbon steps
        // onto the ladder, and the rung above the top is out of bounds, so
        // the Up latch clears at the top.
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
             |s.................g\n\
             ====================",
        )]);
        game.set_action(Some(Action::Left));
        advance(&mut game, 1);
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 9));
        game.set_action(Some(Action::Up));
        advance(&mut game, 12);
        assert_eq!(game.gibbon.y, 0);
        assert_eq!(game.action, None);
    }

    #[test]
    fn climbing_up_onto_the_top_of_the_ladder() {
        // Ladder at x0 with open space above its top rung: the gibbon steps
        // onto the ladder, climbs up and stands on top of it (the open cell
        // above the top rung) since nothing blocks it.
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
             |s.................g\n\
             ====================",
        )]);
        game.set_action(Some(Action::Left));
        advance(&mut game, 1);
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 9));
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
            "...................g\n\
             s-.................g\n\
             =...................",
        )]);
        game.set_action(Some(Action::Right));
        advance(&mut game, 4);
        assert_eq!(game.gibbon.x, 2);
        assert!(game.gibbon.y > 1);
        // Its direction survives the fall: it resumes once it lands.
        assert_eq!(game.action, Some(Action::Right));
    }

    #[test]
    fn pressing_down_jumps_off_a_ladder_into_open_air() {
        // The bottom rung of a ladder hangs over open air: Down steps off it
        // and the gibbon keeps falling until it lands on the floor.
        let mut game = game(vec![level(
            "|..................g\n\
             |..................g\n\
             |..................g\n\
             |..................g\n\
             s..................g\n\
             ....................\n\
             ....................\n\
             ....................\n\
             ....................\n\
             ....................\n\
             ====================",
        )]);
        game.gibbon = Actor::at(0, 3); // the bottom rung, open air below
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
    fn gibbon_does_not_hang_on_a_ladder_that_is_above_him() {
        // A ladder ends above the spawn with open air below: nothing holds the
        // gibbon, so it starts falling instead of hanging from the bottom rung.
        let mut game = game(vec![level(
            "|..................g\n\
             |..................g\n\
             |..................g\n\
             s..................g\n\
             ....................\n\
             *....................",
        )]);
        assert_eq!(game.gibbon.y, 3);

        game.set_action(None);
        advance(&mut game, 1);
        // One tick and it is already falling past the spawn cell.
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 4));
        // It lands on the brick below and rests there.
        advance(&mut game, 2);
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 4));
    }

    #[test]
    fn pressing_down_jumps_off_a_railing() {
        // Hanging on a railing with open air below, Down lets the gibbon drop.
        let mut game = game(vec![level(
            "...................g\n\
             s-.................g\n\
             *...................\n\
             ....................\n\
             ....................\n\
             ....................\n\
             =...================",
        )]);
        game.set_action(Some(Action::Right));
        advance(&mut game, 1);
        game.set_action(None);
        advance(&mut game, 2);
        assert_eq!((game.gibbon.x, game.gibbon.y), (1, 1)); // hanging

        game.set_action(Some(Action::Down));
        advance(&mut game, 2);
        assert_eq!(game.gibbon.x, 1);
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
             ====================",
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
             ====================",
        )]);
        game.gibbon = Actor::at(0, 5);
        game.gibbon2 = Actor::at(0, 5);
        game.guards = vec![Actor::at(0, 3)]; // bottom rung, open air below
        assert_eq!(
            game.guard_action(game.guards[0], true, game.gibbon),
            Some(Action::Down)
        );
        advance(&mut game, 1);
        assert_eq!((game.guards[0].x, game.guards[0].y), (0, 4));
    }

    #[test]
    fn digging_opens_a_hole_and_regrows_after_dig_ticks() {
        // Gibbon stands on a wooden floor and digs the tile down-left.
        let mut game = game(vec![level(
            ".s..................g\n\
             ==..................\n\
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
             ==..................=\n\
             ====================",
        )]);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        assert_eq!(game.level.tile(0, 1), Tile::Empty);

        // Move the timer to just before regrow, then drop the gibbon into the
        // hole (it is supported by the floor row below it).
        advance(&mut game, DIG_TICKS - 1);
        game.gibbon = Actor::at(0, 1);

        let lives = game.lives;
        // One tick ends the countdown and starts the wood rising back; the
        // following tick completes the restoration and crushes the gibbon.
        advance(&mut game, 2);
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
             ==..................=\n\
             ====================",
        )]);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        assert_eq!(game.level.tile(0, 1), Tile::Empty);
        // One tick before the wood regrows, stand a guard in the hole. The
        // next update ends the countdown and starts the restoration.
        advance(&mut game, DIG_TICKS - 1);
        game.guards = vec![Actor::at(0, 1)];
        game.update_holes();
        assert!(matches!(game.holes[0].phase, HolePhase::Restoring));
        assert_eq!(game.level.tile(0, 1), Tile::Empty);
        // Complete the restoration: only then is the guard sent back.
        for _ in 0..SIM_FRAMES {
            game.animate_digs();
        }
        let (sx, sy) = game.level.guard_spawns[0];
        assert_eq!((game.guards[0].x, game.guards[0].y), (sx as i32, sy as i32));
        assert_eq!(game.level.tile(0, 1), Tile::Wood);
        assert!(game.holes.is_empty());
    }

    #[test]
    fn dug_wood_destroys_over_one_cell_time() {
        // The pit is open the moment the dig fires, but the tile sinks away
        // over exactly SIM_FRAMES frames (the time to travel one cell).
        let mut game = game(vec![level(
            ".s..................g\n\
             ==..................\n\
             ....................",
        )]);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        assert_eq!(game.holes.len(), 1);
        assert!(matches!(game.holes[0].phase, HolePhase::Digging));
        assert_eq!(game.holes[0].frames, SIM_FRAMES);
        assert_eq!(game.level.tile(0, 1), Tile::Empty);
        // Still sinking for the first SIM_FRAMES - 1 frames...
        for _ in 0..SIM_FRAMES - 1 {
            game.step();
            assert!(matches!(game.holes[0].phase, HolePhase::Digging));
        }
        // ...and fully gone on the SIM_FRAMES-th frame.
        game.step();
        assert!(matches!(game.holes[0].phase, HolePhase::Regrowing));
    }

    #[test]
    fn wood_restores_over_one_cell_time() {
        // After the regrow countdown the wood grows back over exactly
        // SIM_FRAMES frames; the cell stays a usable pit until the end.
        let mut game = game(vec![level(
            ".s..................g\n\
             ==..................\n\
             ....................",
        )]);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        // Dig animation (1 tick) plus the full regrow countdown.
        advance(&mut game, DIG_TICKS);
        assert!(matches!(game.holes[0].phase, HolePhase::Restoring));
        assert_eq!(game.holes[0].frames, SIM_FRAMES);
        assert_eq!(game.level.tile(0, 1), Tile::Empty);
        // Still restoring for the first SIM_FRAMES - 1 frames...
        for _ in 0..SIM_FRAMES - 1 {
            game.step();
            assert!(matches!(game.holes[0].phase, HolePhase::Restoring));
            assert_eq!(game.level.tile(0, 1), Tile::Empty);
        }
        // ...and fully back on the SIM_FRAMES-th frame.
        game.step();
        assert!(game.holes.is_empty());
        assert_eq!(game.level.tile(0, 1), Tile::Wood);
    }

    #[test]
    fn destroy_and_restore_frame_indices_span_the_sheet() {
        // 12-frame sheet: the dig animation runs from the intact first frame
        // to the destroyed last frame as the SIM_FRAMES destruction frames
        // elapse, and the regrow animation runs back the other way.
        assert_eq!(destroy_frame(SIM_FRAMES, 12), 0);
        assert_eq!(destroy_frame(SIM_FRAMES / 2, 12), 5);
        assert_eq!(destroy_frame(0, 12), 11);
        assert_eq!(restore_frame(SIM_FRAMES, 12), 11);
        assert_eq!(restore_frame(SIM_FRAMES / 2, 12), 5);
        assert_eq!(restore_frame(0, 12), 0);
        // Empty or single-frame sheets never pick an out-of-range index.
        assert_eq!(destroy_frame(0, 0), 0);
        assert_eq!(restore_frame(0, 0), 0);
        assert_eq!(destroy_frame(0, 1), 0);
        assert_eq!(restore_frame(0, 1), 0);
    }

    /// The palette-index pixels of one board cell of a framebuffer.
    fn cell_pixels(fb: &Framebuffer, x: i32, y: i32) -> Vec<u8> {
        let (px, py) = cell_screen(x, y);
        let width = fb.width() as i32;
        let mut out = Vec::with_capacity((CELL * CELL) as usize);
        for row in 0..CELL {
            let o = ((py + row) * width + px) as usize;
            out.extend_from_slice(&fb.pixels()[o..o + CELL as usize]);
        }
        out
    }

    /// Decode the embedded wood sheet as the game would, so rendering can be
    /// compared against its frames.
    fn wood_sheet() -> Vec<RleSprite> {
        let data = crate::assets::load("wood.png").expect("wood sheet embedded");
        let mut palette = Palette::default();
        SpriteSheet::from_png(data, &mut palette, CELL as usize, CELL as usize, 12)
            .expect("wood sheet decodes")
            .to_rle()
            .expect("wood sheet encodes")
    }

    /// Decode the embedded ladder sheet as the game would.
    fn ladder_sheet() -> Vec<RleSprite> {
        let data = crate::assets::load("ladder.png").expect("ladder sheet embedded");
        let mut palette = Palette::default();
        SpriteSheet::from_png(data, &mut palette, CELL as usize, CELL as usize, 1)
            .expect("ladder sheet decodes")
            .to_rle()
            .expect("ladder sheet encodes")
    }

    /// Decode the embedded stone wall sheet as the game would.
    fn stone_sheet() -> Vec<RleSprite> {
        let data = crate::assets::load("stone.png").expect("stone sheet embedded");
        let mut palette = Palette::default();
        SpriteSheet::from_png(data, &mut palette, CELL as usize, CELL as usize, 1)
            .expect("stone sheet decodes")
            .to_rle()
            .expect("stone sheet encodes")
    }

    /// Decode one embedded sheet against the given palette, so its pixels
    /// land on the exact palette indices; `flipped` mirrors every frame
    /// horizontally first.
    fn decode_sheet(
        name: &str,
        palette: &mut Palette,
        frames: usize,
        flipped: bool,
    ) -> SpriteSheet {
        let data = crate::assets::load(name).expect("sprite sheet embedded");
        let sheet = SpriteSheet::from_png(data, palette, CELL as usize, CELL as usize, frames)
            .expect("sprite sheet decodes");
        if flipped {
            sheet.flipped_horizontal()
        } else {
            sheet
        }
    }

    /// Build a character's sheets like the game does: the standing pose from
    /// `stand_name` (with `stand_frames` frames) and the walking animation
    /// from `walk_name`, flipped for the left-facing frames. When `recolor`
    /// is set, every pixel is run through it first (player two's green tint).
    fn character_sheets(
        palette: &mut Palette,
        stand_name: &str,
        stand_frames: usize,
        walk_name: &str,
        recolor: Option<fn(Color) -> Color>,
    ) -> CharacterSheets {
        let mut decode = |name: &str, frames: usize, flip: bool| -> Vec<RleSprite> {
            let sheet = decode_sheet(name, palette, frames, flip);
            match recolor {
                Some(mapping) => sheet
                    .recolored(palette, mapping)
                    .to_rle()
                    .expect("sprite sheet encodes"),
                None => sheet.to_rle().expect("sprite sheet encodes"),
            }
        };
        CharacterSheets {
            stand: decode(stand_name, stand_frames, false),
            stand_left: decode(stand_name, stand_frames, true),
            walk_right: decode(walk_name, WALK_FRAMES, false),
            walk_left: decode(walk_name, WALK_FRAMES, true),
            climb: Vec::new(),
        }
    }

    /// The gibbon's sheets from the real embedded art.
    fn gibbon_sheets() -> CharacterSheets {
        let mut palette = crate::palette::palette();
        character_sheets(
            &mut palette,
            "gibbon.png",
            STAND_FRAMES,
            "gibbon_move_right.png",
            None,
        )
    }

    /// The same art recolored green, as player two loads it.
    fn gibbon2_sheets() -> CharacterSheets {
        let mut palette = crate::palette::palette();
        character_sheets(
            &mut palette,
            "gibbon.png",
            STAND_FRAMES,
            "gibbon_move_right.png",
            Some(crate::play::player2_color),
        )
    }

    fn draw_game(game: &Game, wood: &[RleSprite], ladder: &[RleSprite]) -> Framebuffer {
        let mut fb = Framebuffer::new();
        game.draw(
            &mut fb,
            0,
            &GameSprites {
                fruit: &[],
                gibbon: gibbon_sheets().sprites(),
                gibbon2: gibbon_sheets().sprites(),
                guard: gibbon_sheets().sprites(),
                wood,
                ladder,
                stone: &[],
            },
        );
        fb
    }

    #[test]
    fn wood_tiles_render_from_the_intact_frame() {
        // A static wood tile is drawn from the intact first frame of the
        // sheet rather than procedurally.
        let wood = wood_sheet();
        assert_eq!(wood.len(), 12);

        let mut game = game(vec![level(
            "=s...................\n\
             ....................",
        )]);
        game.gibbon = Actor::at(19, 11); // off-screen
        game.guards = vec![];
        let fb = draw_game(&game, &wood, &[]);

        let mut expected = Framebuffer::new();
        let (px, py) = cell_screen(0, 0);
        wood[0].draw(&mut expected, px, py);
        assert_eq!(cell_pixels(&fb, 0, 0), cell_pixels(&expected, 0, 0));
    }

    #[test]
    fn destroyed_wood_stays_visible_in_the_dug_cell() {
        // Once the dig animation finishes, the completely destroyed wood (the
        // sheet's last frame) is still drawn in the place where it was dug,
        // over the open pit.
        let wood = wood_sheet();
        let mut game = game(vec![level(
            ".s.................g\n\
             =...................\n\
             ....................",
        )]);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 2); // 1 tick digs, the next finishes the animation
        assert!(matches!(game.holes[0].phase, HolePhase::Regrowing));
        assert_eq!(game.level.tile(0, 1), Tile::Empty);
        game.gibbon = Actor::at(19, 11); // off-screen
        game.guards = vec![];
        let fb = draw_game(&game, &wood, &[]);

        let mut expected = Framebuffer::new();
        let (px, py) = cell_screen(0, 1);
        draw_hole(&mut expected, 0, 1);
        wood[wood.len() - 1].draw(&mut expected, px, py);
        assert_eq!(cell_pixels(&fb, 0, 1), cell_pixels(&expected, 0, 1));
    }

    #[test]
    fn ladders_render_from_the_sheet() {
        // A ladder rung is drawn from ladder.png's single frame rather than
        // procedurally.
        let ladder = ladder_sheet();
        assert_eq!(ladder.len(), 1);

        let mut game = game(vec![level(
            "|s...................\n\
             ....................",
        )]);
        game.gibbon = Actor::at(19, 11); // off-screen
        game.guards = vec![];
        let fb = draw_game(&game, &[], &ladder);

        let mut expected = Framebuffer::new();
        let (px, py) = cell_screen(0, 0);
        ladder[0].draw(&mut expected, px, py);
        assert_eq!(cell_pixels(&fb, 0, 0), cell_pixels(&expected, 0, 0));
    }

    #[test]
    fn stone_walls_render_from_the_sheet() {
        // An unbreakable brick tile is drawn from stone.png's single frame
        // rather than procedurally.
        let stone = stone_sheet();
        assert_eq!(stone.len(), 1);

        let mut game = game(vec![level(
            "*s...................\n\
             ....................",
        )]);
        game.gibbon = Actor::at(19, 11); // off-screen
        game.guards = vec![];
        let mut fb = Framebuffer::new();
        game.draw(
            &mut fb,
            0,
            &GameSprites {
                fruit: &[],
                gibbon: gibbon_sheets().sprites(),
                gibbon2: gibbon_sheets().sprites(),
                guard: gibbon_sheets().sprites(),
                wood: &[],
                ladder: &[],
                stone: &stone,
            },
        );

        let mut expected = Framebuffer::new();
        let (px, py) = cell_screen(0, 0);
        stone[0].draw(&mut expected, px, py);
        assert_eq!(cell_pixels(&fb, 0, 0), cell_pixels(&expected, 0, 0));
    }

    #[test]
    fn the_second_player_gibbon_is_green() {
        // Player two reuses the player gibbon's art recolored green at load
        // time, so the two gibbons are distinguishable while idle.
        let orange = gibbon_sheets();
        let green = gibbon2_sheets();
        assert_eq!(orange.stand.len(), STAND_FRAMES);
        assert_eq!(green.stand.len(), STAND_FRAMES);

        let mut game = game(vec![level(
            "s..................g\n\
             ....................",
        )]);
        game.gibbon = Actor::at(19, 11); // off-screen
        game.gibbon2 = Actor::at(2, 0);
        game.guards = vec![];
        let mut fb = Framebuffer::new();
        game.draw(
            &mut fb,
            0,
            &GameSprites {
                fruit: &[],
                gibbon: orange.sprites(),
                gibbon2: green.sprites(),
                guard: gibbon_sheets().sprites(),
                wood: &[],
                ladder: &[],
                stone: &[],
            },
        );

        let body = crate::palette::palette().index_of(PLAYER2_BODY);
        let orange_body = crate::palette::palette().index_of(PLAYER_BODY);
        let pixels = cell_pixels(&fb, 2, 0);
        assert!(pixels.contains(&body), "player two is drawn in green");
        assert!(!pixels.contains(&orange_body), "player two is not orange");
    }

    #[test]
    fn digging_works_while_airborne() {
        // No solid ground under the gibbon: it can still dig a wooden tile
        // diagonally below, spending the tick on digging instead of falling.
        let mut game = game(vec![level(
            ".s.................g\n\
             =.=...................",
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
             ====................\n\
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
             *=..................\n\
             ....................",
        )]);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        assert!(game.holes.is_empty());
        assert_eq!(game.level.tile(0, 1), Tile::Brick);
    }

    #[test]
    fn a_failed_dig_does_not_hold_the_fall() {
        // Nothing wooden to dig: the command is a no-op and the fall goes on.
        let mut game = game(vec![level(
            ".s.................g\n\
             ....................",
        )]);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        assert!(game.holes.is_empty());
        assert_eq!((game.gibbon.x, game.gibbon.y), (1, 1));
    }

    #[test]
    fn digging_is_blocked_by_a_solid_cell_above_the_target() {
        // The tile right above the target is wood: the dig is blocked.
        let mut game = game(vec![level(
            "=s...................\n\
             =.=...................",
        )]);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        assert!(game.holes.is_empty());
        assert_eq!(game.level.tile(0, 1), Tile::Wood);
    }

    #[test]
    fn digging_is_blocked_by_a_wall_above_the_target() {
        // The tile right above the target is brick: the dig is blocked.
        let mut game = game(vec![level(
            "*s...................\n\
             =.=...................",
        )]);
        game.set_action(Some(Action::DigLeft));
        advance(&mut game, 1);
        assert!(game.holes.is_empty());
        assert_eq!(game.level.tile(0, 1), Tile::Wood);
    }

    #[test]
    fn digging_replaces_the_movement_command_and_stops() {
        // Moving left and digging: the dig command takes over from the held
        // movement direction, fires once from the current cell (down-left of
        // the landing cell) and resolves back to no action.
        let mut game = game(vec![level(
            "....s.................g\n\
             ====================\n\
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
             ==..................\n\
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
             ====================\n\
             ....................",
        )]);
        assert_eq!(game.gibbon.x, 4);
        assert_eq!(game.guards.len(), 2);
        assert_eq!(game.guards[0].x, 0);
        assert_eq!(game.guards[1].x, 0);
        // Both gibbons run right together, so whichever the guard picks it
        // keeps the same constant gap to the leader.
        game.set_action(Some(Action::Right));
        game.set_action2(Some(Action::Right));
        for _ in 0..12 {
            let gap_before = game.gibbon.x - game.guards[0].x;
            advance(&mut game, 1);
            let gap_after = game.gibbon.x - game.guards[0].x;
            assert_eq!(gap_before, gap_after, "the guard keeps the same distance");
            assert_eq!(game.gibbon.x - 4, game.guards[0].x, "both gained one cell");
        }
    }

    #[test]
    fn a_single_guard_spawn_places_both_guards_there() {
        // With only one 'g' in the level, both guards start on that cell and
        // chase with different priorities (guard 0 vertical, guard 1
        // horizontal).
        let game = game(vec![level(
            "s..................g\n\
             ....................",
        )]);
        assert_eq!(game.guards.len(), 2);
        assert_eq!((game.guards[0].x, game.guards[0].y), (19, 0));
        assert_eq!((game.guards[1].x, game.guards[1].y), (19, 0));
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
             ==..................\n\
             ....................\n\
             ....................\n\
             ....................\n\
             ....................\n\
             ....................\n\
             ....................\n\
             ..==................",
        )]);
        game.gibbon = Actor::at(5, 0);
        game.gibbon2 = Actor::at(5, 0);
        game.guards = vec![Actor::at(0, 2)];
        assert_eq!(
            game.guard_action(game.guards[0], true, game.gibbon),
            Some(Action::Right)
        );
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
             ====================",
        )]);
        game.gibbon = Actor::at(5, 0);
        game.gibbon2 = Actor::at(5, 0);
        game.guards = vec![Actor::at(5, 8), Actor::at(10, 8)];
        // Guard 0 (vertical priority): the gibbon is straight up a ladder.
        assert_eq!(
            game.guard_action(game.guards[0], true, game.gibbon),
            Some(Action::Up)
        );
        // Guard 1 (horizontal priority): the gibbon is to its left on a clear
        // row, so it walks left first.
        assert_eq!(
            game.guard_action(game.guards[1], false, game.gibbon),
            Some(Action::Left)
        );
    }

    #[test]
    fn guards_catch_the_gibbon_and_it_respawns() {
        let mut game = game(vec![level(
            "s..................g\n\
             ==..................\n\
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
             ==..................\n\
             ....................",
        )]);
        game.lives = 1;
        game.guards = vec![Actor::at(1, 0)];
        advance(&mut game, 1);
        assert_eq!(game.game_state, State::GameOver);
        // After the game-over timeout the whole game restarts from level one
        // with full lives.
        advance(&mut game, GAME_OVER_TICKS);
        assert_eq!(game.game_state, State::Playing);
        assert_eq!(game.level_index, 0);
        assert_eq!(game.lives, LIVES);
        assert_eq!(game.gibbon.x, 0);
        assert_eq!(game.gibbon.y, 0);
    }

    #[test]
    fn player_two_controls_the_second_gibbon() {
        // Player two's input drives gibbon2 with its own independent latch,
        // leaving player one's gibbon alone.
        let mut game = game(vec![level(
            "s..................g\n\
             ==..................\n\
             ....................",
        )]);
        game.set_action2(Some(Action::Right));
        advance(&mut game, 1);
        assert_eq!((game.gibbon.x, game.gibbon.y), (0, 0));
        assert_eq!((game.gibbon2.x, game.gibbon2.y), (1, 0));
        assert_eq!(game.action, None);
        assert_eq!(game.action2, Some(Action::Right));
        // The latch keeps gibbon2 walking without the key being held.
        advance(&mut game, 1);
        assert_eq!(game.gibbon2.x, 2);
    }

    #[test]
    fn guards_chase_the_closer_gibbon() {
        // A guard between two gibbons heads for the nearer one.
        let mut game = game(vec![level(
            "s..................g\n\
             ====================\n\
             ....................",
        )]);
        game.gibbon = Actor::at(8, 0);
        game.gibbon2 = Actor::at(1, 0);
        game.guards = vec![Actor::at(5, 0)];
        advance(&mut game, 1);
        // Distance to gibbon1 is 3, to gibbon2 is 4: it walks right.
        assert_eq!(game.guards[0].x, 6);
        assert_eq!(game.caught, [false, false]);
    }

    #[test]
    fn a_single_catch_continues_without_losing_a_life() {
        // The guard reaches the nearer gibbon (player two) and catches it,
        // but the game keeps running: no life is lost and the guard turns to
        // chase the remaining gibbon.
        let mut game = game(vec![level(
            "s..................g\n\
             ==..................\n\
             ....................",
        )]);
        game.gibbon = Actor::at(5, 0);
        game.gibbon2 = Actor::at(0, 0);
        game.guards = vec![Actor::at(1, 0)];
        advance(&mut game, 1);
        assert_eq!(game.caught, [false, true]);
        assert_eq!(game.lives, LIVES);
        assert_eq!(game.game_state, State::Playing);
        // The caught gibbon is out for this life and the guard now targets
        // the free one.
        advance(&mut game, 1);
        assert_eq!((game.gibbon2.x, game.gibbon2.y), (0, 0));
        assert_eq!(game.guards[0].x, 1);
        assert_eq!(game.game_state, State::Playing);
    }

    #[test]
    fn both_gibbons_caught_costs_a_life() {
        // Both gibbons share the spawn, so a guard that reaches it catches
        // both at once and a life is lost.
        let mut game = game(vec![level(
            "s..................g\n\
             ==..................\n\
             ....................",
        )]);
        game.guards = vec![Actor::at(1, 0)];
        advance(&mut game, 1);
        assert_eq!(game.caught, [true, true]);
        assert_eq!(game.lives, LIVES - 1);
        assert_eq!(game.game_state, State::Dead);
    }

    #[test]
    fn the_remaining_gibbon_can_still_clear_the_level() {
        // With player one already caught, player two collects the last fruit:
        // the level clears and the next level starts with both gibbons free.
        let a = level(
            "s@..................g\n\
             ==..................\n\
             ....................",
        );
        let b = level(
            "s@..................g\n\
             ==..................\n\
             ....................",
        );
        let mut game = game(vec![a, b]);
        game.caught = [true, false];
        game.set_action2(Some(Action::Right));
        advance(&mut game, 1);
        assert_eq!(game.game_state, State::Cleared);
        assert_eq!(game.lives, LIVES);
        advance(&mut game, CLEAR_TICKS);
        assert_eq!(game.level_index, 1);
        assert_eq!(game.caught, [false, false]);
        assert_eq!(game.gibbon, Actor::at(0, 0));
        assert_eq!(game.gibbon2, Actor::at(0, 0));
        assert_eq!(game.game_state, State::Playing);
    }

    #[test]
    fn one_player_mode_ignores_the_second_gibbon() {
        // With a single player the second gibbon neither moves nor collects
        // fruit, and the guards chase player one even when the idle gibbon
        // sits closer to them.
        let mut game = game(vec![level(
            "s..@.............g.\n\
             ====================",
        )]);
        game.players = 1;
        game.gibbon = Actor::at(15, 0);
        game.gibbon2 = Actor::at(3, 0);
        game.guards = vec![Actor::at(8, 0)];
        game.set_action2(Some(Action::Right));
        advance(&mut game, 1);
        // The guard heads right toward player one (at 15), not left toward the
        // idle second gibbon (at 3).
        assert_eq!(game.guards[0].x, 9);
        // The idle gibbon stays put despite its latched command, and does not
        // take the fruit on its cell.
        assert_eq!(game.gibbon2, Actor::at(3, 0));
        assert_eq!(game.level.tile(3, 0), Tile::Fruit);
        assert_eq!(game.fruits_left, 1);
    }

    #[test]
    fn one_player_catch_costs_a_life_directly() {
        // A single gibbon has no partner to share the life with: being caught
        // loses a life right away, like the classic single-player game.
        let mut game = game(vec![level(
            "s..................g\n\
             ==..................\n\
             ....................",
        )]);
        game.players = 1;
        game.guards = vec![Actor::at(1, 0)];
        advance(&mut game, 1);
        assert_eq!(game.caught[0], true);
        assert_eq!(game.lives, LIVES - 1);
        assert_eq!(game.game_state, State::Dead);
    }

    #[test]
    fn levels_advance_after_clear() {
        let a = level(
            "s@..................g\n\
             ==..................\n\
             ....................",
        );
        let b = level(
            "s@..................g\n\
             ==..................\n\
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
             ==..................\n\
             ....................",
        )]);
        game.set_action(Some(Action::Right));
        advance(&mut game, CLEAR_TICKS + 2);
        assert_eq!(game.game_state, State::Win);
    }
}
