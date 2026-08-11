use crate::render::{Color, Renderer};

use rand::RngExt;
use rand::rngs::ThreadRng;

/// Fixed simulation timestep (Hz). The game simulation advances in fixed 1/60 s
/// steps, independently of the display refresh rate.
pub const SIM_STEP_HZ: u32 = 60;

/// Seconds between snake move ticks.
pub const MOVE_INTERVAL: f64 = 0.2;

/// Number of fixed simulation steps per move tick (0.5 s * 60 Hz).
pub const TICK_STEPS: u32 = (MOVE_INTERVAL * SIM_STEP_HZ as f64) as u32;

/// The board is GRID_SIZE x GRID_SIZE cells.
pub const GRID_SIZE: i32 = 20;
pub const HALF_GRID: i32 = GRID_SIZE / 2;

/// Logical layout (480 x 270), top-left origin.
pub const CELL: i32 = 12;
pub const BOARD_PX: i32 = GRID_SIZE * CELL;
pub const HUD_H: i32 = 30;
pub const BOARD_X: i32 = (480 - BOARD_PX) / 2;
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

pub struct Snake {
    pub body: Vec<Segment>,
    pub direction: Direction,
    food: Cell,
    rng: ThreadRng,
    steps_in_tick: u32,
    // Ring buffer of pending direction changes entered between ticks.
    queue: [Option<Direction>; MAX_QUEUED_INPUTS],
    queue_len: usize,
    queue_read: usize,
    pub grew_last_tick: bool,
}

impl Default for Snake {
    fn default() -> Snake {
        Snake::new()
    }
}

impl Snake {
    pub fn new() -> Snake {
        let rng = rand::rng();
        let mut body = Vec::with_capacity(32);
        // Head at (3, 0) moving right; body extends to x = 0.
        for x in (0..4).rev() {
            let pos = Cell { x, y: 0 };
            body.push(Segment {
                current: pos,
                previous: pos,
            });
        }
        let mut snake = Snake {
            body,
            direction: Direction::Right,
            food: Cell { x: 0, y: 0 },
            rng,
            steps_in_tick: 0,
            queue: [None; MAX_QUEUED_INPUTS],
            queue_len: 0,
            queue_read: 0,
            grew_last_tick: false,
        };
        snake.respawn_food();
        snake
    }

    pub fn food(&self) -> Cell {
        self.food
    }

    /// Advance one fixed simulation step. The snake moves one cell every
    /// TICK_STEPS steps.
    pub fn step(&mut self) {
        self.steps_in_tick += 1;
        if self.steps_in_tick >= TICK_STEPS {
            self.steps_in_tick = 0;
            self.move_tick();
        }
    }

    /// Interpolation alpha for the current tick, fixed point in 0..=65536.
    /// `(steps + 1) / TICK_STEPS` keeps motion continuous across tick
    /// boundaries (a freshly moved segment starts just past its previous cell).
    pub fn alpha(&self) -> u32 {
        let n = (self.steps_in_tick + 1).min(TICK_STEPS);
        (n * 65536 + TICK_STEPS / 2) / TICK_STEPS
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

    fn move_tick(&mut self) {
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

        let mut head = self.body[0].current;
        match self.direction {
            Direction::Up => head.y += 1,
            Direction::Down => head.y -= 1,
            Direction::Left => head.x -= 1,
            Direction::Right => head.x += 1,
        }

        if head.x >= HALF_GRID {
            head.x = -HALF_GRID;
        }
        if head.x < -HALF_GRID {
            head.x = HALF_GRID - 1;
        }
        if head.y >= HALF_GRID {
            head.y = -HALF_GRID;
        }
        if head.y < -HALF_GRID {
            head.y = HALF_GRID - 1;
        }

        self.grew_last_tick = false;

        let ate_food = head == self.food;

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

        // Eat food: grow from the tail and respawn the food.
        if ate_food {
            let tail = self.body[self.body.len() - 1];
            self.body.push(Segment {
                current: tail.current,
                previous: tail.previous,
            });
            self.grew_last_tick = true;
            self.respawn_food();
        }
    }

    fn respawn_food(&mut self) {
        loop {
            let x = self.rng.random_range(-HALF_GRID..HALF_GRID);
            let y = self.rng.random_range(-HALF_GRID..HALF_GRID);
            let pos = Cell { x, y };
            let occupied = self.body.iter().any(|s| s.current == pos);
            if !occupied {
                self.food = pos;
                break;
            }
        }
    }

    /// Screen pixel position of a cell's top-left corner (top-left origin).
    fn cell_screen(cell: Cell) -> (i32, i32) {
        (
            BOARD_X + (cell.x + HALF_GRID) * CELL,
            BOARD_Y + (HALF_GRID - 1 - cell.y) * CELL,
        )
    }

    /// Draw the board, snake and food into a logical-coordinate renderer.
    pub fn draw(&self, r: &mut impl Renderer, alpha: u32) {
        draw_grid(r);
        draw_border(r);
        self.draw_snake(r, alpha);
        self.draw_food(r);
    }

    fn draw_snake(&self, r: &mut impl Renderer, alpha: u32) {
        for (i, segment) in self.body.iter().enumerate() {
            let (x, y) = segment_screen(*segment, alpha);
            r.fill_rect(x, y, CELL, CELL, SNAKE_COLOR);
            if i == 0 {
                draw_head_details(r, *segment, self.direction, alpha);
            }
        }
    }

    fn draw_food(&self, r: &mut impl Renderer) {
        let (cx, cy) = Self::cell_screen(self.food);
        r.fill_circle(cx + CELL / 2, cy + CELL / 2, FOOD_RADIUS, FOOD_COLOR);
    }
}

/// Interpolated screen top-left of a segment during a tick.
fn segment_screen(segment: Segment, alpha: u32) -> (i32, i32) {
    let (px, py) = Snake::cell_screen(segment.previous);
    let (cx, cy) = Snake::cell_screen(segment.current);
    if !segment.interpolates() {
        return (cx, cy);
    }
    (interp(px, cx, alpha), interp(py, cy, alpha))
}

/// Integer linear interpolation with rounding. `alpha` is fixed point 0..=65536.
fn interp(a: i32, b: i32, alpha: u32) -> i32 {
    let d = (b - a) as i64;
    (a as i64 + ((d * alpha as i64 + 32768) >> 16)) as i32
}

// Palette (matches the original Bevy rendering as closely as practical).
const BG_COLOR: Color = Color::rgb(13, 13, 18);
const GRID_COLOR: Color = Color::rgb(38, 38, 46);
const BORDER_COLOR: Color = Color::rgb(255, 255, 255);
const SNAKE_COLOR: Color = Color::rgb(51, 204, 51);
const EYE_COLOR: Color = Color::rgb(13, 13, 13);
const TONGUE_COLOR: Color = Color::rgb(230, 77, 77);
const FOOD_COLOR: Color = Color::rgb(230, 26, 26);

const BORDER_THICKNESS: i32 = 2;
const FOOD_RADIUS: i32 = 5;

/// 1px dark grid lines between the cells, drawn behind the snake so the snake
/// covers them as it passes (same layering as the original).
fn draw_grid(r: &mut impl Renderer) {
    for i in 1..GRID_SIZE {
        let x = BOARD_X + i * CELL;
        r.fill_rect(x, BOARD_Y, 1, BOARD_PX, GRID_COLOR);
        let y = BOARD_Y + i * CELL;
        r.fill_rect(BOARD_X, y, BOARD_PX, 1, GRID_COLOR);
    }
}

fn draw_border(r: &mut impl Renderer) {
    let t = BORDER_THICKNESS;
    let size = BOARD_PX + 2 * t;
    let x = BOARD_X - t;
    let y = BOARD_Y - t;
    r.fill_rect(x, y, size, t, BORDER_COLOR);
    r.fill_rect(x, y + size - t, size, t, BORDER_COLOR);
    r.fill_rect(x, y, t, size, BORDER_COLOR);
    r.fill_rect(x + size - t, y, t, size, BORDER_COLOR);
}

/// Eyes and forked tongue on the interpolated head, pointing along the move
/// direction. Everything is integer pixel arithmetic.
fn draw_head_details(r: &mut impl Renderer, segment: Segment, direction: Direction, alpha: u32) {
    let (hx, hy) = segment_screen(segment, alpha);
    let (cx, cy) = (hx + CELL / 2, hy + CELL / 2);
    let (dx, dy) = direction.screen_vec();
    // Front edge point of the head cell.
    let (fx, fy) = (cx + dx * (CELL / 2), cy + dy * (CELL / 2));
    // Perpendicular unit vector in screen space.
    let (px, py) = (-dy, dx);

    // Eyes: 2x2 dark squares near the front corners.
    for sign in [-1, 1] {
        let ex = fx + px * sign * 3 - dx * 2;
        let ey = fy + py * sign * 3 - dy * 2;
        r.fill_rect(ex - 1, ey - 1, 2, 2, EYE_COLOR);
    }

    // Forked tongue: two 1px prongs diverging from the front of the mouth.
    for sign in [-1, 1] {
        for len in 1..=3 {
            let tx = fx + dx * len + px * sign * len;
            let ty = fy + dy * len + py * sign * len;
            r.fill_rect(tx, ty, 1, 1, TONGUE_COLOR);
        }
    }
}

pub const fn bg_color() -> Color {
    BG_COLOR
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(snake: &Snake) -> Cell {
        snake.body[0].current
    }

    #[test]
    fn initial_snake_layout() {
        let snake = Snake::new();
        assert_eq!(snake.body.len(), 4);
        assert_eq!(head(&snake), Cell { x: 3, y: 0 });
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
    fn moves_right_without_input() {
        let mut snake = Snake::new();
        for _ in 0..TICK_STEPS {
            snake.step();
        }
        assert_eq!(head(&snake), Cell { x: 4, y: 0 });
    }

    #[test]
    fn wraps_at_board_edge() {
        let mut snake = Snake::new();
        // Head starts at x = 3, moving right. The rightmost cell is x = 9; the
        // 7th tick moves past the edge and wraps to x = -10.
        for _ in 0..(TICK_STEPS * 7) {
            snake.step();
        }
        assert_eq!(head(&snake), Cell { x: -10, y: 0 });
    }

    #[test]
    fn queues_turns_between_ticks() {
        let mut snake = Snake::new();
        snake.queue_direction(Direction::Up);
        snake.queue_direction(Direction::Left);
        for _ in 0..TICK_STEPS {
            snake.step();
        }
        // First tick applies Up, moving from (3, 0) to (3, 1).
        assert_eq!(head(&snake), Cell { x: 3, y: 1 });
        for _ in 0..TICK_STEPS {
            snake.step();
        }
        // Second tick applies Left -> (2, 1).
        assert_eq!(head(&snake), Cell { x: 2, y: 1 });
    }

    #[test]
    fn rejects_reversal_and_repeat() {
        let mut snake = Snake::new();
        snake.queue_direction(Direction::Right); // repeat, ignored
        snake.queue_direction(Direction::Left); // reversal, ignored
        snake.queue_direction(Direction::Up);
        for _ in 0..TICK_STEPS {
            snake.step();
        }
        assert_eq!(head(&snake), Cell { x: 3, y: 1 });
    }

    #[test]
    fn grows_when_eating_food() {
        let mut snake = Snake::new();
        // Force the food onto the head's path by fixing RNG? Not accessible, so
        // instead run until the snake grows: move straight, food is random.
        let mut grew = false;
        let mut steps = 0;
        while steps < TICK_STEPS * 200 {
            // Steer towards the food.
            let fx = snake.food().x;
            let fy = snake.food().y;
            let h = head(&snake);
            let dir = if h.y == fy {
                if fx > h.x {
                    Direction::Right
                } else {
                    Direction::Left
                }
            } else if fy > h.y {
                Direction::Up
            } else {
                Direction::Down
            };
            snake.set_direction(dir);
            snake.step();
            steps += 1;
            if snake.grew_last_tick {
                grew = true;
                break;
            }
        }
        assert!(grew, "snake never reached the food in 200 ticks");
        assert_eq!(snake.body.len(), 5);
    }

    #[test]
    fn alpha_is_strictly_increasing_and_wraps() {
        let mut snake = Snake::new();
        let mut prev = 0u32;
        for _ in 0..TICK_STEPS {
            let a = snake.alpha();
            assert!(a > prev, "alpha must increase during a tick");
            assert!(a <= 65536);
            prev = a;
            snake.step();
        }
        // After a tick the alpha wraps back down to just above 0.
        let wrapped = snake.alpha();
        assert!(wrapped < prev);
        assert!(wrapped > 0);
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
        let (x0, y0) = Snake::cell_screen(Cell { x: 3, y: 0 });
        let (x1, y1) = Snake::cell_screen(Cell { x: 4, y: 0 });
        // At the very start of a tick the segment still sits in its previous cell.
        assert_eq!(segment_screen(segment, 0), (x0, y0));
        // At the very end it reaches its current cell.
        assert_eq!(segment_screen(segment, 65536), (x1, y1));
        // Both anchors are on the board, not at the window origin.
        assert!(x0 >= BOARD_X && x1 >= BOARD_X);
        assert!(y0 >= BOARD_Y && y1 >= BOARD_Y);
    }
}
