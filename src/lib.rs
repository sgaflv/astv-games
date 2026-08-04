use bevy::camera::visibility::RenderLayers;
use bevy::camera::{
    Camera, ClearColorConfig, OrthographicProjection, Projection, RenderTarget, ScalingMode,
};
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::image::{Image, ImageSampler};
use bevy::render::render_resource::TextureFormat;
use bevy::window::{PrimaryWindow, WindowResolution};
use bevy::{prelude::*, window::Window};

use rand::RngExt;

use std::collections::VecDeque;

const GRID_SIZE: i32 = 20;
const CELL_SIZE: f32 = 25.0;
// Eye sprite size in world units; placed near the front corners of the head.
const EYE_SIZE: f32 = 7.0;
// Forked tongue: two thin prongs diverging from the front of the head, along
// the move direction. FORK_ANGLE is the half-angle between the prongs in rad.
const PRONG_LENGTH: f32 = 8.0;
const PRONG_THICKNESS: f32 = 5.0;
const FORK_ANGLE: f32 = 0.5;

// The game is rendered into an offscreen texture at this resolution and then
// upscaled with nearest sampling at an integer scale. 384x216 divides the
// target TV resolution 3840x2160 exactly (x10 both axes) and also 1920x1080
// exactly (x5), so the blit fills the whole screen edge-to-edge with every
// render pixel mapped 1:10 (or 1:5) onto physical pixels.
const RENDER_WIDTH: u32 = 384 * 2;
const RENDER_HEIGHT: u32 = 216 * 2;

// Local desktop window size (independent of the render target above).
const WINDOW_WIDTH: u32 = 840;
const WINDOW_HEIGHT: u32 = 560;

// Fixed world-unit height shown by the game camera. This keeps the board
// framing identical regardless of RENDER_WIDTH/HEIGHT (so lowering the render
// resolution only makes pixels bigger, it never crops the board).
// 540 is chosen so each cell is an EVEN whole number of render texels
// (CELL_SIZE * RENDER_HEIGHT / VIEW_HEIGHT = 25 * 216 / 540 = 10). Even means a
// cell's edges land exactly on texel boundaries (odd counts leave them at
// half-texels, which rasterize unevenly and make the grid lines look
// irregular). 2.5 world units per texel is also exact in f32.
const GAME_VIEW_HEIGHT: f32 = 540.0;
#[derive(Component, Clone, Copy, PartialEq)]
struct SnakeSegment {
    current: IVec2,  // current logical cell
    previous: IVec2, // previous logical cell
}

#[derive(Component)]
struct Food;

#[derive(Component)]
struct BlitSprite;

#[derive(Component, Clone, Copy, PartialEq)]
struct FoodPosition {
    x: i32,
    y: i32,
}

#[derive(Component)]
struct FpsText;

#[derive(Component)]
struct SnakeEye;

#[derive(Component)]
struct SnakeTongue;

#[derive(Resource)]
struct Snake {
    body: Vec<Entity>,
    direction: Direction,
    // Pending direction changes entered between ticks, applied one per tick.
    // A single slot drops the first press of a quick U-turn; the queue keeps
    // both so Up+Left entered before a tick both register.
    input_queue: VecDeque<Direction>,
}

const BORDER_SIZE: f32 = GRID_SIZE as f32 * CELL_SIZE;

const MOVE_INTERVAL: f32 = 0.5;

#[derive(Resource)]
struct MoveTimer(Timer);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn opposite(self) -> Self {
        match self {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }
}

#[bevy_main]
pub fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Snake".into(),
                resolution: WindowResolution::new(WINDOW_WIDTH, WINDOW_HEIGHT),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .insert_resource(Snake {
            body: Vec::new(),
            direction: Direction::Right,
            input_queue: VecDeque::new(),
        })
        .insert_resource(MoveTimer(Timer::from_seconds(
            MOVE_INTERVAL,
            TimerMode::Repeating,
        )))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                keyboard_input,
                move_snake,
                interpolate_snake,
                update_head,
                fps_system,
                update_blit_sprite,
            ),
        )
        .run();
}

fn interpolate_snake(
    //time: Res<Time>,
    timer: Res<MoveTimer>,
    mut query: Query<(&SnakeSegment, &mut Transform)>,
) {
    let alpha = timer.0.fraction();

    for (segment, mut transform) in &mut query {
        let previous = Vec2::new(segment.previous.x as f32, segment.previous.y as f32);

        let current = Vec2::new(segment.current.x as f32, segment.current.y as f32);

        let position = previous.lerp(current, alpha);

        transform.translation.x = position.x * CELL_SIZE;
        transform.translation.y = position.y * CELL_SIZE;
    }
}

// Places the head's eyes and tongue toward the current move direction. Eyes
// and tongue are children of the head, so this only sets local transforms and
// they follow the head as it moves and interpolates.
fn update_head(
    snake: Res<Snake>,
    mut eyes: Query<&mut Transform, (With<SnakeEye>, Without<SnakeTongue>)>,
    mut tongue: Query<&mut Transform, (With<SnakeTongue>, Without<SnakeEye>)>,
) {
    let (fx, fy) = match snake.direction {
        Direction::Up => (0.0, 1.0),
        Direction::Down => (0.0, -1.0),
        Direction::Left => (-1.0, 0.0),
        Direction::Right => (1.0, 0.0),
    };
    let (sx, sy) = (-fy, fx);

    let angle = match snake.direction {
        Direction::Up => std::f32::consts::FRAC_PI_2,
        Direction::Down => -std::f32::consts::FRAC_PI_2,
        Direction::Left => std::f32::consts::PI,
        Direction::Right => 0.0,
    };

    let positions = [
        Vec3::new(
            fx * EYE_SIZE + sx * EYE_SIZE,
            fy * EYE_SIZE + sy * EYE_SIZE,
            0.1,
        ),
        Vec3::new(
            fx * EYE_SIZE - sx * EYE_SIZE,
            fy * EYE_SIZE - sy * EYE_SIZE,
            0.1,
        ),
    ];

    for (i, mut transform) in eyes.iter_mut().enumerate() {
        if let Some(position) = positions.get(i) {
            transform.translation = *position;
        }
    }

    // Forked tongue: two prongs starting at the front of the mouth and
    // diverging by +/-FORK_ANGLE from the move direction.
    let base_offset = CELL_SIZE / 2.0;
    for (i, mut transform) in tongue.iter_mut().enumerate() {
        let sign = if i == 0 { 1.0 } else { -1.0 };
        let prong_angle = angle + sign * FORK_ANGLE;
        let (sin, cos) = prong_angle.sin_cos();

        transform.translation = Vec3::new(
            fx * base_offset + cos * PRONG_LENGTH / 2.0,
            fy * base_offset + sin * PRONG_LENGTH / 2.0,
            0.1,
        );
        transform.rotation = Quat::from_rotation_z(prong_angle);
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut snake: ResMut<Snake>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    // Offscreen low-resolution render target. Nearest sampling makes the
    // upscaled pixels crisp instead of blurry.
    let mut target = Image::new_target_texture(
        RENDER_WIDTH,
        RENDER_HEIGHT,
        TextureFormat::Rgba8UnormSrgb,
        None,
    );
    target.sampler = ImageSampler::nearest();
    let target_handle = images.add(target);

    // Camera 1 (order 0): renders the game scene into the low-res texture.
    // The projection shows a fixed world-unit area so the board always fits.
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: GAME_VIEW_HEIGHT,
            },
            ..OrthographicProjection::default_2d()
        }),
        Camera {
            order: 0,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.05, 0.05, 0.07)),
            ..default()
        },
        RenderTarget::Image(target_handle.clone().into()),
        RenderLayers::layer(0),
    ));

    // Camera 2 (order 1): renders to the window, only the blit sprite. Cleared
    // with the same color as the game so letterbox bars blend into the frame.
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.05, 0.05, 0.07)),
            ..default()
        },
        RenderLayers::layer(1),
    ));

    // Fullscreen sprite showing the upscaled low-res texture at an integer
    // scale (see integer_blit_scale / update_blit_sprite).
    let window_size = windows
        .single()
        .map(|window| window.resolution.size())
        .unwrap_or(Vec2::new(RENDER_WIDTH as f32, RENDER_HEIGHT as f32));

    let initial_size = Vec2::splat(integer_blit_scale(window_size))
        * Vec2::new(RENDER_WIDTH as f32, RENDER_HEIGHT as f32);

    commands.spawn((
        Sprite {
            image: target_handle,
            custom_size: Some(initial_size),
            ..default()
        },
        Transform::default(),
        RenderLayers::layer(1),
        BlitSprite,
    ));

    spawn_border(&mut commands);
    spawn_grid(&mut commands);

    let mut body = Vec::new();

    for x in (0..4).rev() {
        let pos = IVec2::new(x, 0);

        let entity = commands
            .spawn((
                Sprite::from_color(Color::srgb(0.2, 0.8, 0.2), Vec2::splat(CELL_SIZE)),
                Transform::from_xyz(pos.x as f32 * CELL_SIZE, pos.y as f32 * CELL_SIZE, 0.),
                SnakeSegment {
                    previous: pos,
                    current: pos,
                },
            ))
            .id();

        body.push(entity);
    }

    snake.body = body;

    // Two dark eyes and a forked tongue (two prongs) parented to the head,
    // positioned each frame by update_head toward the current move direction.
    let head = snake.body[0];

    for _ in 0..2 {
        let eye = commands
            .spawn((
                Sprite::from_color(Color::srgb(0.05, 0.05, 0.05), Vec2::splat(EYE_SIZE)),
                Transform::from_xyz(0.0, 0.0, 0.1),
                SnakeEye,
            ))
            .id();
        commands.entity(head).add_child(eye);
    }

    for _ in 0..2 {
        let prong = commands
            .spawn((
                Sprite::from_color(
                    Color::srgb(0.9, 0.3, 0.3),
                    Vec2::new(PRONG_LENGTH, PRONG_THICKNESS),
                ),
                Transform::from_xyz(0.0, 0.0, 0.1),
                SnakeTongue,
            ))
            .id();
        commands.entity(head).add_child(prong);
    }

    commands.spawn((
        Text::new("FPS: 0"),
        Node {
            position_type: PositionType::Absolute,
            top: px(10.0),
            left: px(10.0),
            ..default()
        },
        FpsText,
    ));

    spawn_food(&mut commands, &mut meshes, &mut materials);
}

fn fps_system(
    diagnostics: Res<DiagnosticsStore>,
    mut query: Query<&mut Text, With<FpsText>>,
    mut frame_counter: Local<u32>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    // Only rewrite + re-layout the text every 30th frame; per-frame text
    // re-layout is surprisingly expensive on weak TV CPUs.
    *frame_counter = (*frame_counter + 1) % 30;

    if *frame_counter != 0 {
        return;
    }

    // Report the actual on-screen pixel size, not the render target size.
    let resolution = windows
        .single()
        .ok()
        .map(|window| window.resolution.physical_size())
        .unwrap_or(UVec2::new(RENDER_WIDTH, RENDER_HEIGHT));

    // Integer upscale factor: each render texel becomes `scale` physical pixels.
    let scale = integer_blit_scale(resolution.as_vec2());

    if let Some(fps) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
    {
        for mut text in &mut query {
            text.0 = format!(
                "FPS: {:.0}  screen {}x{}  render {}x{}  pixel {:.0}px",
                fps, resolution.x, resolution.y, RENDER_WIDTH, RENDER_HEIGHT, scale
            );
        }
    }
}

// Largest integer scale factor that fits the window. The blit sprite must be
// shown at an exact integer scale with nearest sampling, otherwise a render
// pixel can straddle two physical pixels and you get aliased "half" pixels.
// On a 3840x2160 TV the 384x216 frame is shown at scale 10 -> 3840x2160 exactly,
// filling the whole screen. Both scaled dimensions are even, so a centered
// sprite's edges land exactly on physical pixel boundaries.
fn integer_blit_scale(size: Vec2) -> f32 {
    (size.x / RENDER_WIDTH as f32)
        .floor()
        .min((size.y / RENDER_HEIGHT as f32).floor())
        .max(1.0)
}

fn update_blit_sprite(
    mut query: Query<(&mut Sprite, &mut Transform), With<BlitSprite>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let size = window.resolution.size();
    let scale = integer_blit_scale(size);

    // Scaled dims are even (384x216 * integer scale), so a centered sprite
    // maps each render texel onto exactly `scale` physical pixels with no
    // half-pixel seams.
    let scaled = Vec2::new(RENDER_WIDTH as f32 * scale, RENDER_HEIGHT as f32 * scale);

    for (mut sprite, mut transform) in &mut query {
        sprite.custom_size = Some(scaled);
        transform.translation = Vec3::new(0.0, 0.0, 0.0);
    }
}

fn keyboard_input(keys: Res<ButtonInput<KeyCode>>, mut snake: ResMut<Snake>) {
    let dir = if keys.just_pressed(KeyCode::ArrowUp) {
        Direction::Up
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        Direction::Down
    } else if keys.just_pressed(KeyCode::ArrowLeft) {
        Direction::Left
    } else if keys.just_pressed(KeyCode::ArrowRight) {
        Direction::Right
    } else {
        return;
    };

    // Validate against the most recent direction the snake will move in (last
    // buffered turn, or the current one if nothing is buffered). Ignore exact
    // repeats and 180-degree reversals so a queued Up can't be immediately
    // cancelled by a Down.
    let last = snake.input_queue.back().copied().unwrap_or(snake.direction);

    if dir == last || dir == last.opposite() {
        return;
    }

    if snake.input_queue.len() < 3 {
        snake.input_queue.push_back(dir);
    }
}

fn spawn_border(commands: &mut Commands) {
    let thickness = 5.0;
    let size = BORDER_SIZE + thickness;

    // Top border
    commands.spawn((
        Sprite::from_color(Color::WHITE, Vec2::new(size, thickness)),
        Transform::from_xyz(0.0, BORDER_SIZE / 2.0, -1.0),
    ));

    // Bottom border
    commands.spawn((
        Sprite::from_color(Color::WHITE, Vec2::new(size, thickness)),
        Transform::from_xyz(0.0, -BORDER_SIZE / 2.0, -1.0),
    ));

    // Left border
    commands.spawn((
        Sprite::from_color(Color::WHITE, Vec2::new(thickness, size)),
        Transform::from_xyz(-BORDER_SIZE / 2.0, 0.0, -1.0),
    ));

    // Right border
    commands.spawn((
        Sprite::from_color(Color::WHITE, Vec2::new(thickness, size)),
        Transform::from_xyz(BORDER_SIZE / 2.0, 0.0, -1.0),
    ));
}

// One render texel in world units (GAME_VIEW_HEIGHT spans RENDER_HEIGHT texels).
fn texel_world() -> f32 {
    GAME_VIEW_HEIGHT / RENDER_HEIGHT as f32
}

// Dark grey grid lines between the cells. These are plain geometry on the
// game camera's layer (0), so they are rendered into the low-res texture and
// get the same crisp integer-scaled pixelation as everything else. Each line
// is exactly one texel wide: centered half a texel off the cell boundary it
// covers a single render-texel column/row (a cell is exactly 10 texels), so
// the lines are uniform and hard-edged after upscaling.
fn spawn_grid(commands: &mut Commands) {
    let width = texel_world();
    let offset = width * 0.5;
    let color = Color::srgb(0.15, 0.15, 0.18);

    // Internal boundaries run at world x/y = i * CELL_SIZE for i in -(N-1)..=(N-1).
    let first = -(GRID_SIZE / 2 - 1);
    let last = GRID_SIZE / 2 - 1;

    for i in first..=last {
        let x = i as f32 * CELL_SIZE + offset;
        let y = i as f32 * CELL_SIZE + offset;

        commands.spawn((
            Sprite::from_color(color, Vec2::new(width, BORDER_SIZE)),
            Transform::from_xyz(x, 0.0, -2.0),
        ));

        commands.spawn((
            Sprite::from_color(color, Vec2::new(BORDER_SIZE, width)),
            Transform::from_xyz(0.0, y, -2.0),
        ));
    }
}

fn spawn_food(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<ColorMaterial>>,
) {
    let mut rng = rand::rng();

    let x = rng.random_range(-GRID_SIZE / 2..GRID_SIZE / 2);
    let y = rng.random_range(-GRID_SIZE / 2..GRID_SIZE / 2);

    commands.spawn((
        Food,
        FoodPosition { x, y },
        Mesh2d(meshes.add(Circle::new(CELL_SIZE * 0.4))),
        MeshMaterial2d(materials.add(Color::srgb(0.9, 0.1, 0.1))),
        Transform::from_xyz(x as f32 * CELL_SIZE, y as f32 * CELL_SIZE, 0.),
    ));
}
fn move_snake(
    time: Res<Time>,
    mut timer: ResMut<MoveTimer>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut snake: ResMut<Snake>,
    mut positions: Query<&mut SnakeSegment>,
    food_query: Query<(Entity, &FoodPosition), With<Food>>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }

    // Apply the next buffered turn. A reversal can never be queued (see
    // keyboard_input), but guard against it here anyway.
    while let Some(dir) = snake.input_queue.pop_front() {
        if dir != snake.direction && dir != snake.direction.opposite() {
            snake.direction = dir;
            break;
        }
    }

    // Snapshot current grid positions
    let mut old_positions = Vec::new();

    for entity in &snake.body {
        if let Ok(segment) = positions.get(*entity) {
            old_positions.push(segment.current);
        }
    }

    if old_positions.is_empty() {
        return;
    }

    // Compute new head position
    let mut head = old_positions[0];

    match snake.direction {
        Direction::Up => head.y += 1,
        Direction::Down => head.y -= 1,
        Direction::Left => head.x -= 1,
        Direction::Right => head.x += 1,
    }

    let half_grid = GRID_SIZE / 2;

    if head.x >= half_grid {
        head.x = -half_grid;
    }

    if head.x < -half_grid {
        head.x = half_grid - 1;
    }

    if head.y >= half_grid {
        head.y = -half_grid;
    }

    if head.y < -half_grid {
        head.y = half_grid - 1;
    }

    let ate_food = food_query
        .iter()
        .find(|(_, food_pos)| food_pos.x == head.x && food_pos.y == head.y)
        .map(|(entity, _)| entity);

    // Move body
    for i in (1..snake.body.len()).rev() {
        let new_position = old_positions[i - 1];

        if let Ok(mut segment) = positions.get_mut(snake.body[i]) {
            let wrapped = (segment.current.x - new_position.x).abs() > 1
                || (segment.current.y - new_position.y).abs() > 1;

            if wrapped {
                segment.previous = new_position;
                segment.current = new_position;
            } else {
                segment.previous = segment.current;
                segment.current = new_position;
            }
        }
    }

    // Move head
    if let Ok(mut segment) = positions.get_mut(snake.body[0]) {
        let wrapped =
            (segment.current.x - head.x).abs() > 1 || (segment.current.y - head.y).abs() > 1;

        if wrapped {
            segment.previous = head;
            segment.current = head;
        } else {
            segment.previous = segment.current;
            segment.current = head;
        }
    }

    // Eat food
    if let Some(food_entity) = ate_food {
        commands.entity(food_entity).despawn();

        let tail = positions.get(snake.body.last().copied().unwrap()).unwrap();

        let new_segment = commands
            .spawn((
                SnakeSegment {
                    previous: tail.previous,
                    current: tail.current,
                },
                Sprite {
                    color: Color::srgb(0.2, 0.8, 0.2),
                    custom_size: Some(Vec2::splat(CELL_SIZE)),
                    ..default()
                },
                Transform::from_xyz(
                    tail.current.x as f32 * CELL_SIZE,
                    tail.current.y as f32 * CELL_SIZE,
                    0.,
                ),
            ))
            .id();

        snake.body.push(new_segment);

        spawn_food(&mut commands, &mut meshes, &mut materials);
    }
}
