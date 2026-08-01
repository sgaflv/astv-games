use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::{prelude::*, window::WindowResolution};

use rand::RngExt;

const GRID_SIZE: i32 = 20;
const CELL_SIZE: f32 = 25.0;

#[derive(Component, Clone, Copy, PartialEq)]
struct SnakeSegment {
    current: IVec2,  // current logical cell
    previous: IVec2, // previous logical cell
}

#[derive(Component)]
struct Food;

#[derive(Component, Clone, Copy, PartialEq)]
struct FoodPosition {
    x: i32,
    y: i32,
}

#[derive(Component)]
struct FpsText;

#[derive(Resource)]
struct Snake {
    body: Vec<Entity>,
    direction: Direction,
    next_direction: Direction,
}

const BORDER_SIZE: f32 = GRID_SIZE as f32 * CELL_SIZE;

const MOVE_INTERVAL: f32 = 0.5;

#[derive(Resource)]
struct MoveTimer(Timer);

#[derive(Clone, Copy, PartialEq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[bevy_main]
pub fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Snake".into(),
                resolution: WindowResolution::new(800, 600),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .insert_resource(Snake {
            body: Vec::new(),
            direction: Direction::Right,
            next_direction: Direction::Right,
        })
        .insert_resource(MoveTimer(Timer::from_seconds(
            MOVE_INTERVAL,
            TimerMode::Repeating,
        )))
        .add_systems(Startup, setup)
        .add_systems(Update, (keyboard_input, move_snake))
        .add_systems(Update, fps_system)
        .add_systems(
            Update,
            (keyboard_input, move_snake, interpolate_snake, fps_system),
        )
        .run();
}

fn interpolate_snake(
    time: Res<Time>,
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

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut snake: ResMut<Snake>,
) {
    commands.spawn(Camera2d);
    spawn_border(&mut commands);

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

fn fps_system(diagnostics: Res<DiagnosticsStore>, mut query: Query<&mut Text, With<FpsText>>) {
    if let Some(fps) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
    {
        for mut text in &mut query {
            text.0 = format!("FPS: {:.0}", fps);
        }
    }
}

fn keyboard_input(keys: Res<ButtonInput<KeyCode>>, mut snake: ResMut<Snake>) {
    if keys.just_pressed(KeyCode::ArrowUp) && snake.direction != Direction::Down {
        snake.next_direction = Direction::Up;
    }

    if keys.just_pressed(KeyCode::ArrowDown) && snake.direction != Direction::Up {
        snake.next_direction = Direction::Down;
    }

    if keys.just_pressed(KeyCode::ArrowLeft) && snake.direction != Direction::Right {
        snake.next_direction = Direction::Left;
    }

    if keys.just_pressed(KeyCode::ArrowRight) && snake.direction != Direction::Left {
        snake.next_direction = Direction::Right;
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
        MeshMaterial2d(materials.add(Color::srgb(1.0, 1.0, 0.0))),
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

    snake.direction = snake.next_direction;

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
