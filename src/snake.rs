use crate::{
    engine::render::{Color, Renderer},
    game::SIM_FRAMES,
};

/// The board is GRID_SIZE_X x GRID_SIZE_Y cells.
pub const GRID_SIZE_X: i32 = 20;
pub const GRID_SIZE_Y: i32 = 11;

/// Logical layout (480 x 270), top-left origin.
pub const CELL: i32 = 24;

pub const BOARD_PX: i32 = GRID_SIZE_X * CELL;
pub const BOARD_PY: i32 = GRID_SIZE_Y * CELL;

pub const HUD_H: i32 = 3;
pub const BOARD_X: i32 = 0;
pub const BOARD_Y: i32 = HUD_H;

/// Maximum number of direction changes queued between ticks.
pub const MAX_QUEUED_INPUTS: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn opposite(self) -> Direction {
        match self {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }

    /// Unit vector in screen coordinates (x right, y down).
    pub fn screen_vec(self) -> (i32, i32) {
        match self {
            Direction::Up => (0, -1),
            Direction::Down => (0, 1),
            Direction::Left => (-1, 0),
            Direction::Right => (1, 0),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Segment {
    /// Current logical cell.
    pub current: Cell,
    /// Cell the segment was in at the start of the current tick.
    pub previous: Cell,
}

impl Segment {
    /// Wrapped moves snap to the new cell instead of interpolating across the
    /// board edge.
    fn interpolates(&self) -> bool {
        self.current != self.previous
    }
}

/// A single snake. It knows nothing about other snakes, the food, or timing:
/// it only moves when asked (`move_tick`), wraps on the board, grows, and
/// draws itself. The game owns the shared food and the tick clock.
pub struct Snake {
    pub body: Vec<Segment>,
    pub direction: Direction,
    color: Color,
    // Ring buffer of pending direction changes entered between ticks.
    queue: [Option<Direction>; MAX_QUEUED_INPUTS],
    queue_len: usize,
    queue_read: usize,
    pub grew_last_tick: bool,
    // Placeholder face expressions driven by the gamepad (see engine::app).
    /// When true, the tongue is not drawn (gamepad A held).
    pub tongue_hidden: bool,
    /// When true, the eyes are not drawn (gamepad B held; blink placeholder).
    pub eyes_closed: bool,
}

impl Snake {
    /// Spawn a 4-cell snake with `head` at the front, `body` trailing behind
    /// in the opposite direction, and the given body color.
    pub fn spawn(color: Color, head: Cell, direction: Direction) -> Snake {
        let back = direction.opposite().screen_vec();
        let mut body = Vec::with_capacity(32);
        for i in 0..4 {
            let pos = Cell {
                x: head.x + back.0 * i,
                y: head.y + back.1 * i,
            };
            body.push(Segment {
                current: pos,
                previous: pos,
            });
        }
        Snake {
            body,
            direction,
            color,
            queue: [None; MAX_QUEUED_INPUTS],
            queue_len: 0,
            queue_read: 0,
            grew_last_tick: false,
            tongue_hidden: false,
            eyes_closed: false,
        }
    }

    pub fn head(&self) -> Cell {
        self.body[0].current
    }

    pub fn color(&self) -> Color {
        self.color
    }

    pub fn queue_direction(&mut self, dir: Direction) {
        if self.queue_len >= MAX_QUEUED_INPUTS {
            return;
        }
        let last = match self.queue_len {
            0 => self.direction,
            _ => {
                let idx = (self.queue_read + self.queue_len - 1) % MAX_QUEUED_INPUTS;
                self.queue[idx].unwrap()
            }
        };
        if dir == last || dir == last.opposite() {
            return;
        }
        let idx = (self.queue_read + self.queue_len) % MAX_QUEUED_INPUTS;
        self.queue[idx] = Some(dir);
        self.queue_len += 1;
    }

    pub fn set_direction(&mut self, dir: Direction) {
        if dir == self.direction || dir == self.direction.opposite() {
            return;
        }
        self.queue = [None; MAX_QUEUED_INPUTS];
        self.queue_len = 0;
        self.queue_read = 0;
        self.queue[0] = Some(dir);
        self.queue_len = 1;
    }

    /// Advance one snake move (one board tick). `foods` is the shared food
    /// pool: if the head reaches any of them the snake grows and
    /// `grew_last_tick` is set (the game removes the eaten food cell and
    /// respawns it). The snake wraps at the board edges.
    pub fn move_tick(&mut self, foods: &[Cell]) {
        // Apply the next buffered turn. A reversal can never be queued, but
        // guard against it here anyway.
        while self.queue_len > 0 {
            let dir = self.queue[self.queue_read].unwrap();
            self.queue[self.queue_read] = None;
            self.queue_read = (self.queue_read + 1) % MAX_QUEUED_INPUTS;
            self.queue_len -= 1;
            if dir != self.direction && dir != self.direction.opposite() {
                self.direction = dir;
                break;
            }
        }

        let mut head = self.head();
        match self.direction {
            Direction::Up => head.y -= 1,
            Direction::Down => head.y += 1,
            Direction::Left => head.x -= 1,
            Direction::Right => head.x += 1,
        }

        if head.x >= GRID_SIZE_X {
            head.x = 0;
        }
        if head.x < 0 {
            head.x = GRID_SIZE_X - 1;
        }
        if head.y >= GRID_SIZE_Y {
            head.y = 0;
        }
        if head.y < 0 {
            head.y = GRID_SIZE_Y - 1;
        }

        self.grew_last_tick = false;

        let ate_food = foods.contains(&head);

        // Move body (tail first).
        for i in (1..self.body.len()).rev() {
            let new_pos = self.body[i - 1].current;
            let wrapped = (self.body[i].current.x - new_pos.x).abs() > 1
                || (self.body[i].current.y - new_pos.y).abs() > 1;
            self.body[i] = if wrapped {
                Segment {
                    current: new_pos,
                    previous: new_pos,
                }
            } else {
                Segment {
                    current: new_pos,
                    previous: self.body[i].current,
                }
            };
        }

        // Move head.
        let wrapped = (self.body[0].current.x - head.x).abs() > 1
            || (self.body[0].current.y - head.y).abs() > 1;
        self.body[0] = if wrapped {
            Segment {
                current: head,
                previous: head,
            }
        } else {
            Segment {
                current: head,
                previous: self.body[0].current,
            }
        };

        // Eat food: grow from the tail. Respawn is left to the game.
        if ate_food {
            let tail = self.body[self.body.len() - 1];
            self.body.push(Segment {
                current: tail.current,
                previous: tail.previous,
            });
            self.grew_last_tick = true;
        }
    }

    /// Split the snake at body index `index` (the cell that was bitten). The
    /// bitten cell and everything behind it are severed; the head part
    /// `[0..index]` keeps moving. The severed cells are returned snapped
    /// static (they no longer move) so the game can turn them into food.
    /// The head always survives: a bite on the head (index 0) severs everything
    /// except the head, and an out-of-range index severs nothing.
    pub fn split_at(&mut self, index: usize) -> Vec<Segment> {
        let index = index.max(1).min(self.body.len());
        self.body
            .drain(index..)
            .map(|s| Segment {
                current: s.current,
                previous: s.current,
            })
            .collect()
    }

    /// Draw the snake body and head details into a logical-coordinate renderer.
    /// `alpha` is the fixed-point tick interpolation from the game.
    pub fn draw(&self, r: &mut impl Renderer, frame_cnt: usize) {
        for (i, segment) in self.body.iter().enumerate() {
            let (x, y) = segment_screen(*segment, frame_cnt);
            r.fill_rect(x, y, CELL, CELL, self.color);
            if i == 0 {
                draw_head_details(
                    r,
                    *segment,
                    self.direction,
                    frame_cnt,
                    self.tongue_hidden,
                    self.eyes_closed,
                );
            }
        }
    }
}

// Drawing helpers (integer pixel arithmetic).

const EYE_COLOR: Color = Color::rgb(13, 13, 13);
const TONGUE_COLOR: Color = Color::rgb(230, 77, 77);

/// Interpolated screen top-left of a segment during a tick.
fn segment_screen(segment: Segment, frame_cnt: usize) -> (i32, i32) {
    let (px, py) = cell_screen(segment.previous);
    let (cx, cy) = cell_screen(segment.current);
    if !segment.interpolates() {
        return (cx, cy);
    }
    (interp(px, cx, frame_cnt), interp(py, cy, frame_cnt))
}

/// Screen pixel position of a cell's top-left corner (top-left origin).
fn cell_screen(cell: Cell) -> (i32, i32) {
    (BOARD_X + cell.x * CELL, BOARD_Y + cell.y * CELL)
}

/// Integer linear interpolation with rounding.
fn interp(a: i32, b: i32, frame_cnt: usize) -> i32 {
    let d = (b - a) as i64;
    (a as i64 + (d * frame_cnt as i64 / SIM_FRAMES as i64)) as i32
}

/// Eyes and forked tongue on the interpolated head, pointing along the move
/// direction. Everything is integer pixel arithmetic.
fn draw_head_details(
    r: &mut impl Renderer,
    segment: Segment,
    direction: Direction,
    alpha: usize,
    tongue_hidden: bool,
    eyes_closed: bool,
) {
    let (hx, hy) = segment_screen(segment, alpha);
    let (cx, cy) = (hx + CELL / 2, hy + CELL / 2);
    let (dx, dy) = direction.screen_vec();
    // Front edge point of the head cell.
    let (fx, fy) = (cx + dx * (CELL / 2), cy + dy * (CELL / 2));
    // Perpendicular unit vector in screen space.
    let (px, py) = (-dy, dx);

    // Eyes: 2x2 dark squares near the front corners. Skipped while blinking
    // (gamepad B held).
    if !eyes_closed {
        for sign in [-1, 1] {
            let ex = fx + px * sign * 3 - dx * 2;
            let ey = fy + py * sign * 3 - dy * 2;
            r.fill_rect(ex - 1, ey - 1, 2, 2, EYE_COLOR);
        }
    }

    // Forked tongue: two 1px prongs diverging from the front of the mouth.
    // Hidden while gamepad A is held.
    if !tongue_hidden {
        for sign in [-1, 1] {
            for len in 1..=3 {
                let tx = fx + dx * len + px * sign * len;
                let ty = fy + dy * len + py * sign * len;
                r.fill_rect(tx, ty, 1, 1, TONGUE_COLOR);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::render::Framebuffer;

    fn snake() -> Snake {
        Snake::spawn(
            Color::rgb(51, 204, 51),
            Cell { x: 3, y: 0 },
            Direction::Right,
        )
    }

    #[test]
    fn initial_snake_layout() {
        let snake = snake();
        assert_eq!(snake.body.len(), 4);
        assert_eq!(snake.head(), Cell { x: 3, y: 0 });
        assert_eq!(snake.direction, Direction::Right);
        for (i, s) in snake.body.iter().enumerate() {
            assert_eq!(
                s.current,
                Cell {
                    x: 3 - i as i32,
                    y: 0
                }
            );
            assert_eq!(s.previous, s.current);
        }
    }

    #[test]
    fn spawn_mirrors_direction() {
        // A snake facing left at the bottom-right has its body trailing right.
        let snake = Snake::spawn(
            Color::rgb(77, 148, 255),
            Cell { x: 16, y: 10 },
            Direction::Left,
        );
        for (i, s) in snake.body.iter().enumerate() {
            assert_eq!(
                s.current,
                Cell {
                    x: 16 + i as i32,
                    y: 10
                }
            );
        }
        assert_eq!(snake.head(), Cell { x: 16, y: 10 });
    }

    #[test]
    fn wraps_at_board_edge() {
        let mut snake = snake();
        // Head starts at x = 3, moving right. The rightmost cell is x = 39; the
        // 37th move passes the edge and wraps to x = 0.
        for _ in 0..37 {
            snake.move_tick(&[]);
        }
        assert_eq!(snake.head(), Cell { x: 0, y: 0 });
    }

    #[test]
    fn queues_turns_between_ticks() {
        let mut snake = snake();
        snake.queue_direction(Direction::Down);
        snake.queue_direction(Direction::Left);
        snake.move_tick(&[]);
        // First tick applies Down, moving from (3, 0) to (3, 1).
        assert_eq!(snake.head(), Cell { x: 3, y: 1 });
        snake.move_tick(&[]);
        // Second tick applies Left -> (2, 1).
        assert_eq!(snake.head(), Cell { x: 2, y: 1 });
    }

    #[test]
    fn rejects_reversal_and_repeat() {
        let mut snake = snake();
        snake.queue_direction(Direction::Right); // repeat, ignored
        snake.queue_direction(Direction::Left); // reversal, ignored
        snake.queue_direction(Direction::Down);
        snake.move_tick(&[]);
        assert_eq!(snake.head(), Cell { x: 3, y: 1 });
    }

    #[test]
    fn grows_when_reaching_food() {
        let mut snake = snake();
        // Plant the food directly on the head's path.
        snake.move_tick(&[Cell { x: 4, y: 0 }]);
        assert!(snake.grew_last_tick);
        assert_eq!(snake.body.len(), 5);
        // A tick without food does not grow.
        snake.move_tick(&[]);
        assert!(!snake.grew_last_tick);
        assert_eq!(snake.body.len(), 5);
    }

    fn draw_head_pixels(snake: &Snake) -> Vec<[u8; 3]> {
        let mut fb = Framebuffer::new();
        fb.zero();
        snake.draw(&mut fb, 65536);
        let palette = fb.palette();
        fb.pixels()
            .iter()
            .map(|&i| {
                let c = palette.rgb(i);
                [c.r, c.g, c.b]
            })
            .collect()
    }

    fn has_pixel(pixels: &[[u8; 3]], color: Color) -> bool {
        pixels
            .iter()
            .any(|p| p[0] == color.r && p[1] == color.g && p[2] == color.b)
    }

    #[test]
    fn face_details_are_drawn_by_default() {
        let pixels = draw_head_pixels(&snake());
        assert!(has_pixel(&pixels, EYE_COLOR));
        assert!(has_pixel(&pixels, TONGUE_COLOR));
    }

    #[test]
    fn hiding_tongue_skips_only_the_tongue() {
        let mut snake = snake();
        snake.tongue_hidden = true;
        let pixels = draw_head_pixels(&snake);
        assert!(!has_pixel(&pixels, TONGUE_COLOR));
        assert!(has_pixel(&pixels, EYE_COLOR));
    }

    #[test]
    fn closing_eyes_skips_only_the_eyes() {
        let mut snake = snake();
        snake.eyes_closed = true;
        let pixels = draw_head_pixels(&snake);
        assert!(!has_pixel(&pixels, EYE_COLOR));
        assert!(has_pixel(&pixels, TONGUE_COLOR));
    }

    #[test]
    fn split_at_severs_the_bitten_cell_and_the_tail_behind_it() {
        let mut snake = snake();
        // Grow to 5 cells: (4,0),(3,0),(2,0),(1,0),(0,0).
        snake.move_tick(&[Cell { x: 4, y: 0 }]);
        assert_eq!(snake.body.len(), 5);
        // Bite the cell at index 2: keep [0..2], sever the bitten cell and the
        // two cells behind it.
        let severed = snake.split_at(2);
        assert_eq!(snake.body.len(), 2);
        assert_eq!(snake.head(), Cell { x: 4, y: 0 });
        assert_eq!(severed.len(), 3);
        // Severed segments are snapped static (no interpolation).
        for s in &severed {
            assert_eq!(s.current, s.previous);
        }
        assert_eq!(severed[0].current, Cell { x: 2, y: 0 });
    }

    #[test]
    fn split_at_clamps_and_handles_extremes() {
        // Biting the head (index 0) leaves a single head cell.
        let mut bitten = snake();
        let severed = bitten.split_at(0);
        assert_eq!(bitten.body.len(), 1);
        assert_eq!(bitten.head(), Cell { x: 3, y: 0 });
        assert_eq!(severed.len(), 3);
        for s in &severed {
            assert_eq!(s.current, s.previous);
        }
        // An out-of-range index severs nothing.
        let mut other = snake();
        let severed = other.split_at(99);
        assert_eq!(other.body.len(), 4);
        assert!(severed.is_empty());
    }

    #[test]
    fn interp_endpoints_and_midpoints() {
        // alpha = 0 stays at the start, alpha = 65536 reaches the end.
        assert_eq!(interp(240, 252, 0), 240);
        assert_eq!(interp(240, 252, 65536), 252);
        // Half way rounds to the midpoint.
        assert_eq!(interp(240, 252, 65536 / 2), 246);
        // Result depends on the anchor, not just the offset: screen coords in
        // 0..=480 must never collapse toward (0,0).
        assert_eq!(interp(276, 288, 2185), 276);
        assert_eq!(interp(0, 480, 32768), 240);
        // Negative offsets interpolate correctly too.
        assert_eq!(interp(288, 276, 65536 / 2), 282);
    }

    #[test]
    fn segment_screen_anchors_to_previous_cell() {
        let segment = Segment {
            current: Cell { x: 4, y: 0 },
            previous: Cell { x: 3, y: 0 },
        };
        let (x0, y0) = cell_screen(Cell { x: 3, y: 0 });
        let (x1, y1) = cell_screen(Cell { x: 4, y: 0 });
        // At the very start of a tick the segment still sits in its previous cell.
        assert_eq!(segment_screen(segment, 0), (x0, y0));
        // At the very end it reaches its current cell.
        assert_eq!(segment_screen(segment, 65536), (x1, y1));
        // Both anchors are on the board, not at the window origin.
        assert!(x0 >= BOARD_X && x1 >= BOARD_X);
        assert!(y0 >= BOARD_Y && y1 >= BOARD_Y);
    }
}
