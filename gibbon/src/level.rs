//! Level loading: text levels (`assets/levels/lvlN.txt`) parsed into a
//! 20x11-cell [`Level`]. Levels are meant to be 20x11, but smaller ones are
//! padded with empty cells and bigger ones are clipped, so any map file is
//! playable. Invalid levels (a missing gibbon spawn or an unknown symbol) are
//! skipped by [`load_all`].

use crate::assets;

/// Width of the board in cells.
pub const GRID_X: usize = 20;
/// Height of the board in cells.
pub const GRID_Y: usize = 11;

/// One board cell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tile {
    /// Open space (also the `s` / `g` spawn markers).
    Empty,
    /// A solid, diggable yellow wood floor tile.
    Wood,
    /// A solid, unbreakable red brick tile.
    Brick,
    /// A vertical ladder: climbable, not solid.
    Ladder,
    /// A horizontal railing, drawn as a line at the top of the cell. The
    /// gibbon can grab it from the cell below (hang) and climb onto it.
    Railing,
    /// A fruit to collect; not solid.
    Fruit,
}

impl Tile {
    /// A tile that blocks actors and stops falls.
    pub fn is_solid(self) -> bool {
        matches!(self, Tile::Wood | Tile::Brick)
    }
}

/// A parsed level: a flat 20x11 cell grid plus the spawn points.
#[derive(Clone, Debug)]
pub struct Level {
    /// `GRID_X * GRID_Y` tiles, row-major.
    pub cells: Vec<Tile>,
    /// The gibbon spawn cell (`s`).
    pub spawn: (usize, usize),
    /// Every guard spawn cell (`g`), in map order.
    pub guard_spawns: Vec<(usize, usize)>,
    /// The total number of fruits on the level (`@`).
    pub fruits: usize,
}

impl Default for Level {
    fn default() -> Level {
        Level {
            cells: vec![Tile::Empty; GRID_X * GRID_Y],
            spawn: (0, 0),
            guard_spawns: vec![(GRID_X - 1, 0)],
            fruits: 0,
        }
    }
}

impl Level {
    /// The tile at `(x, y)` (0-based, top-left origin). Out-of-bounds cells
    /// read as empty.
    pub fn tile(&self, x: usize, y: usize) -> Tile {
        if x < GRID_X && y < GRID_Y {
            self.cells[y * GRID_X + x]
        } else {
            Tile::Empty
        }
    }

    /// Overwrite the tile at `(x, y)`. Bounds are assumed valid.
    pub fn set_tile(&mut self, x: usize, y: usize, tile: Tile) {
        self.cells[y * GRID_X + x] = tile;
    }
}

/// Parse one level file. `None` when the level lacks a gibbon spawn or
/// contains an unknown character: such levels are skipped. Rows shorter than
/// 20 columns are padded with empty cells; rows/columns beyond 20x11 are
/// clipped away.
pub fn parse(text: &str) -> Option<Level> {
    let mut level = Level::default();
    let mut fruits = 0usize;
    let mut spawn = None;
    let mut guard_spawns = Vec::new();

    for (y, line) in text.lines().enumerate() {
        if y >= GRID_Y {
            break; // taller than the board: clip
        }
        for (x, ch) in line.chars().enumerate() {
            if x >= GRID_X {
                break; // wider than the board: clip
            }
            let tile = match ch {
                '|' => Tile::Ladder,
                '#' => Tile::Wood,
                '*' => Tile::Brick,
                '-' => Tile::Railing,
                '@' => {
                    fruits += 1;
                    Tile::Fruit
                }
                's' => {
                    spawn = Some((x, y));
                    Tile::Empty
                }
                'g' => {
                    guard_spawns.push((x, y));
                    Tile::Empty
                }
                '.' | ' ' => Tile::Empty,
                _ => Tile::Empty,
            };
            level.set_tile(x, y, tile);
        }
    }

    level.fruits = fruits;
    level.spawn = spawn?;
    level.guard_spawns = guard_spawns;
    Some(level)
}

/// Load every embedded level, sorted by level number, skipping invalid ones.
pub fn load_all() -> Vec<Level> {
    let mut levels: Vec<(usize, Level)> = assets::names()
        .filter(|name| name.starts_with("lvl") && name.ends_with(".txt"))
        .filter_map(|name| {
            let num: usize = name[3..name.len() - 4].parse().ok()?;
            let bytes = assets::load(name)?;
            let text = std::str::from_utf8(bytes).ok()?;
            let level = parse(text)?;
            Some((num, level))
        })
        .collect();
    levels.sort_by_key(|(num, _)| *num);
    levels.into_iter().map(|(_, level)| level).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashSet, VecDeque};

    // --- Fruit-reachability model -------------------------------------------
    // A BFS that mirrors the movement rules in `game.rs` (horizontal steps
    // into any non-solid cell, ladder/railing climbing, gravity one cell per
    // tick, diggable only while standing on solid ground) and verifies that
    // every fruit of every embedded level can actually be reached, digging
    // wooden tiles where needed. Fruits collected while falling through a
    // dug hole count as reachable.

    /// The game's `tile()`: out-of-bounds reads as empty.
    fn tile_at(tiles: &[Tile], x: i32, y: i32) -> Option<Tile> {
        if x < 0 || y < 0 || x >= GRID_X as i32 || y >= GRID_Y as i32 {
            None
        } else {
            Some(tiles[y as usize * GRID_X + x as usize])
        }
    }

    fn is_solid(tiles: &[Tile], x: i32, y: i32) -> bool {
        matches!(tile_at(tiles, x, y), Some(t) if t.is_solid())
    }

    fn is_lr(tiles: &[Tile], x: i32, y: i32) -> bool {
        matches!(
            tile_at(tiles, x, y),
            Some(Tile::Ladder) | Some(Tile::Railing)
        )
    }

    fn is_ladder(tiles: &[Tile], x: i32, y: i32) -> bool {
        tile_at(tiles, x, y) == Some(Tile::Ladder)
    }

    /// Supported like `Game::supported`: solid ground below, a ladder directly
    /// below (standing on a ladder's top), or a ladder or railing in the
    /// current cell or the one above.
    fn supported(tiles: &[Tile], x: i32, y: i32) -> bool {
        is_solid(tiles, x, y + 1)
            || is_lr(tiles, x, y)
            || is_lr(tiles, x, y - 1)
            || is_ladder(tiles, x, y + 1)
    }

    /// Drop one cell at a time until supported or on the bottom row, exactly
    /// like `Game::step_gravity` applied repeatedly.
    fn fall(tiles: &[Tile], x: i32, y: i32) -> i32 {
        let mut y = y;
        while y < GRID_Y as i32 - 1 && !supported(tiles, x, y) {
            y += 1;
        }
        y
    }

    /// `(rest cells, cells ever occupied)`, returned by the BFS below.
    type Reach = (HashSet<(i32, i32)>, HashSet<(i32, i32)>);

    /// All reachable rest cells plus every cell the actor ever occupies
    /// (rest positions and cells passed through while falling).
    fn closure(tiles: &[Tile], spawn: (i32, i32)) -> Reach {
        let (sx, sy) = spawn;
        let sy = fall(tiles, sx, sy);
        let mut seen = HashSet::new();
        let mut visited = HashSet::new();
        seen.insert((sx, sy));
        visited.insert((sx, sy));
        let mut q = VecDeque::from([(sx, sy)]);
        while let Some((x, y)) = q.pop_front() {
            // Climb up into a ladder/railing above. Climbing stops at the top
            // rung: the actor never leaves the ladder into the open cell above
            // it.
            if y > 0 && is_lr(tiles, x, y - 1) {
                let nxt = (x, y - 1);
                if seen.insert(nxt) {
                    visited.insert(nxt);
                    q.push_back(nxt);
                }
            }
            if is_lr(tiles, x, y + 1) {
                let nxt = (x, y + 1);
                if seen.insert(nxt) {
                    visited.insert(nxt);
                    q.push_back(nxt);
                }
            }
            for tx in [x - 1, x + 1] {
                if tx < 0 || tx >= GRID_X as i32 || is_solid(tiles, tx, y) {
                    continue;
                }
                let fy = fall(tiles, tx, y);
                for ty in y..=fy {
                    visited.insert((tx, ty));
                }
                if seen.insert((tx, fy)) {
                    q.push_back((tx, fy));
                }
            }
        }
        (seen, visited)
    }

    /// Greedy digging: repeatedly dig the undug wood tile that brings the
    /// most unreached fruits within reach (cell gain breaks ties), mirroring
    /// `Game::dig`. Returns the set of cells ever occupied.
    fn reachable_with_dig(level: &Level) -> HashSet<(i32, i32)> {
        let spawn = (level.spawn.0 as i32, level.spawn.1 as i32);
        let fruits: Vec<(i32, i32)> = level
            .cells
            .iter()
            .enumerate()
            .filter(|(_, t)| **t == Tile::Fruit)
            .map(|(i, _)| ((i % GRID_X) as i32, (i / GRID_X) as i32))
            .collect();
        let mut tiles = level.cells.clone();
        let (mut seen, mut visited) = closure(&tiles, spawn);
        loop {
            let mut best: Option<(i32, i32)> = None;
            let mut best_gain = (0usize, 0usize);
            let mut best_seen = seen.clone();
            let mut best_visited = visited.clone();
            for &(x, y) in &seen {
                if !(y + 1 < GRID_Y as i32 && is_solid(&tiles, x, y + 1)) {
                    continue; // must stand on a floor to dig
                }
                for side in [1, -1] {
                    let (tx, ty) = (x + side, y + 1);
                    if tx < 0 || tx >= GRID_X as i32 || ty < 0 || ty >= GRID_Y as i32 {
                        continue;
                    }
                    let idx = ty as usize * GRID_X + tx as usize;
                    if tiles[idx] != Tile::Wood {
                        continue;
                    }
                    tiles[idx] = Tile::Empty;
                    let (ns, nv) = closure(&tiles, spawn);
                    tiles[idx] = Tile::Wood;
                    let gain = (
                        fruits
                            .iter()
                            .filter(|f| nv.contains(f) && !visited.contains(f))
                            .count(),
                        nv.difference(&visited).count(),
                    );
                    if gain > best_gain {
                        best_gain = gain;
                        best = Some((tx, ty));
                        best_seen = ns;
                        best_visited = nv;
                    }
                }
            }
            let Some((tx, ty)) = best else { break };
            tiles[ty as usize * GRID_X + tx as usize] = Tile::Empty;
            seen = best_seen;
            // Union, never replace: cells reached before a dig stay reachable
            // (the player could have collected their fruits already).
            visited.extend(best_visited);
        }
        visited
    }

    #[test]
    fn every_embedded_level_has_all_fruits_reachable() {
        let levels = crate::level::load_all();
        assert!(!levels.is_empty(), "no levels are embedded");
        for level in &levels {
            let visited = reachable_with_dig(level);
            let unreachable: Vec<(usize, usize)> = level
                .cells
                .iter()
                .enumerate()
                .filter(|(i, t)| {
                    **t == Tile::Fruit
                        && !visited.contains(&((i % GRID_X) as i32, (i / GRID_X) as i32))
                })
                .map(|(i, _)| (i % GRID_X, i / GRID_X))
                .collect();
            assert!(
                unreachable.is_empty(),
                "level with spawn {:?}: unreachable fruits {unreachable:?}",
                level.spawn
            );
        }
    }

    fn level() -> Level {
        parse(
            "-..@.........@......\n\
             |###################\n\
             |...................\n\
             |.........@.........\n\
             |.........###|######\n\
             |.............|.....\n\
             |.........@...|.....\n\
             |#####|##########|##\n\
             |.....|..........|..\n\
             |.s...|....@.....|.g\n\
             ####################",
        )
        .expect("the sample level parses")
    }

    #[test]
    fn parses_spawns_fruits_railings_and_ladders() {
        let level = level();
        assert_eq!(level.spawn, (2, 9));
        assert_eq!(level.guard_spawns, vec![(19, 9)]);
        assert_eq!(level.fruits, 5);
        assert_eq!(level.tile(0, 0), Tile::Railing);
        assert_eq!(level.tile(0, 1), Tile::Ladder);
        assert_eq!(level.tile(1, 1), Tile::Wood);
        assert_eq!(level.tile(8, 10), Tile::Wood);
        assert_eq!(level.tile(2, 9), Tile::Empty);
        assert_eq!(level.tile(3, 0), Tile::Fruit);
    }

    #[test]
    fn multiple_guards_are_all_kept() {
        let level = parse("s........g.....g..g").expect("level parses");
        assert_eq!(level.guard_spawns, vec![(9, 0), (15, 0), (18, 0)]);
    }

    #[test]
    fn short_rows_are_padded_with_empty() {
        let level = parse("s.......g").expect("short level parses");
        assert_eq!(level.spawn, (0, 0));
        assert_eq!(level.guard_spawns, vec![(8, 0)]);
        // Only 9 columns defined; the rest are empty.
        assert_eq!(level.tile(9, 0), Tile::Empty);
        assert_eq!(level.tile(19, 0), Tile::Empty);
        // Only one row defined; rows below are empty.
        assert_eq!(level.tile(0, 10), Tile::Empty);
    }

    #[test]
    fn too_wide_and_too_tall_levels_are_clipped_not_rejected() {
        // Wider than the board: the trailing cells are clipped, so the spawn
        // in the last column survives while the extra wood is dropped.
        let text = format!("{}s##", ".".repeat(GRID_X - 1));
        let level = parse(&text).expect("clipped width still parses");
        assert_eq!(level.spawn, (GRID_X - 1, 0));
        assert_eq!(level.tile(GRID_X - 1, 0), Tile::Empty);
        assert_eq!(level.tile(GRID_X - 2, 0), Tile::Empty);
        // Taller than the board: the extra rows are clipped. Only the first
        // row holds the spawn and a guard so they are not overwritten.
        let mut text = String::from("s.................g\n");
        for _ in 0..GRID_Y + 3 {
            text.push_str("....................");
            text.push('\n');
        }
        let level = parse(&text).expect("clipped height still parses");
        assert_eq!(level.spawn, (0, 0));
        assert_eq!(level.guard_spawns, vec![(18, 0)]);
    }

    #[test]
    fn levels_without_spawns_are_skipped() {
        assert!(parse("....................").is_none());
        assert!(parse("s...................g").is_some());
    }

    #[test]
    fn unknown_characters_are_skipped() {
        assert!(parse("s............q......g").is_none());
    }

    #[test]
    fn solids_are_wood_and_brick() {
        assert!(Tile::Wood.is_solid());
        assert!(Tile::Brick.is_solid());
        assert!(!Tile::Empty.is_solid());
        assert!(!Tile::Ladder.is_solid());
        assert!(!Tile::Railing.is_solid());
        assert!(!Tile::Fruit.is_solid());
    }
}
