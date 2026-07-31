use bevy::{prelude::*, window::WindowResolution};
use rand::RngExt;

const GRID_SIZE: i32 = 20;
const CELL_SIZE: f32 = 25.0;

#[derive(Component)]
struct SnakeSegment;

#[derive(Component)]
struct Food;

#[derive(Component, Clone, Copy, PartialEq)]
struct GridPosition {
    x: i32,
    y: i32,
}

#[derive(Component, Clone, Copy, PartialEq)]
struct FoodPosition {
    x: i32,
    y: i32,
}

#[derive(Resource)]
struct Snake {
    body: Vec<Entity>,
    direction: Direction,
    next_direction: Direction,
}

const BORDER_SIZE: f32 = GRID_SIZE as f32 * CELL_SIZE;

#[derive(Resource)]
struct MoveTimer(Timer);

#[derive(Clone, Copy, PartialEq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

pub fn run() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Snake".into(),
                resolution: WindowResolution::new(800, 600),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(Snake {
            body: Vec::new(),
            direction: Direction::Right,
            next_direction: Direction::Right,
        })
        .insert_resource(MoveTimer(Timer::from_seconds(0.15, TimerMode::Repeating)))
        .add_systems(Startup, setup)
        .add_systems(Update, (keyboard_input, move_snake))
        .run();
}

fn setup(mut commands: Commands, mut snake: ResMut<Snake>) {
    commands.spawn(Camera2d);
    spawn_border(&mut commands);

    let mut body = Vec::new();

    for x in (0..4).rev() {
        let entity = commands
            .spawn((
                Sprite::from_color(Color::srgb(0.2, 0.8, 0.2), Vec2::splat(CELL_SIZE)),
                Transform::from_xyz(x as f32 * CELL_SIZE, 0., 0.),
                SnakeSegment,
                GridPosition { x, y: 0 },
            ))
            .id();

        body.push(entity);
    }

    snake.body = body;

    spawn_food(&mut commands);
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

fn spawn_food(commands: &mut Commands) {
    let mut rng = rand::rng();

    let x = rng.random_range(-GRID_SIZE / 2..GRID_SIZE / 2);
    let y = rng.random_range(-GRID_SIZE / 2..GRID_SIZE / 2);

    commands.spawn((
        Food,
        FoodPosition { x, y },
        Sprite::from_color(Color::srgb(1.0, 0.0, 0.0), Vec2::splat(CELL_SIZE)),
        Transform::from_xyz(x as f32 * CELL_SIZE, y as f32 * CELL_SIZE, 0.),
    ));
}

fn move_snake(
    time: Res<Time>,
    mut timer: ResMut<MoveTimer>,
    mut commands: Commands,
    mut snake: ResMut<Snake>,
    mut positions: Query<&mut GridPosition>,
    mut transforms: Query<&mut Transform>,
    food_query: Query<(Entity, &FoodPosition), With<Food>>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }

    snake.direction = snake.next_direction;

    // Get old positions
    let mut old_positions = Vec::new();

    for entity in &snake.body {
        if let Ok(pos) = positions.get(*entity) {
            old_positions.push(*pos);
        }
    }

    if old_positions.is_empty() {
        return;
    }

    // Calculate new head position
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

    // Move existing segments
    for i in (1..snake.body.len()).rev() {
        let previous = old_positions[i - 1];

        if let Ok(mut pos) = positions.get_mut(snake.body[i]) {
            *pos = previous;
        }

        if let Ok(mut transform) = transforms.get_mut(snake.body[i]) {
            transform.translation.x = previous.x as f32 * CELL_SIZE;
            transform.translation.y = previous.y as f32 * CELL_SIZE;
        }
    }

    // Move head
    if let Ok(mut pos) = positions.get_mut(snake.body[0]) {
        *pos = head;
    }

    if let Ok(mut transform) = transforms.get_mut(snake.body[0]) {
        transform.translation.x = head.x as f32 * CELL_SIZE;
        transform.translation.y = head.y as f32 * CELL_SIZE;
    }

    // Eat food
    if let Some(food_entity) = ate_food {
        commands.entity(food_entity).despawn();

        let tail_position = old_positions.last().unwrap();

        let new_segment = commands
            .spawn((
                SnakeSegment,
                GridPosition {
                    x: tail_position.x,
                    y: tail_position.y,
                },
                Sprite {
                    color: Color::srgb(0.2, 0.8, 0.2),
                    custom_size: Some(Vec2::splat(CELL_SIZE)),
                    ..default()
                },
                Transform::from_xyz(
                    tail_position.x as f32 * CELL_SIZE,
                    tail_position.y as f32 * CELL_SIZE,
                    0.,
                ),
            ))
            .id();

        snake.body.push(new_segment);

        spawn_food(&mut commands);
    }
}
