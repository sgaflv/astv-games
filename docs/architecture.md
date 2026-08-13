# Architecture

The workspace holds two lightweight, integer-based 2D games — a two-player
snake and a single-player Lode Runner-style gibbon — rendered into a 480x270
CPU framebuffer and presented via miniquad/OpenGL ES with integer
nearest-neighbour scaling. Bevy has been removed completely.

## Pipeline

```text
Game logic (integer coords, fixed 60 Hz timestep)
      │
      ▼
Renderer trait (clear / fill_rect / fill_circle / draw_text / draw_rle_image)
      │
      ▼
480×270 indexed CPU framebuffer (129,600 B ≈ 127 KiB, one palette index/pixel)
      │
      ▼
one index texture upload + one fullscreen quad; the fragment shader looks
each index up in a 256×1 palette texture (nearest-neighbour, integer scale)
      │
      ▼
physical display (1920×1080 ×4, 3840×2160 ×8, or letterboxed)
```

## Crate layout

The project is a Cargo workspace with four packages: a reusable `engine`, the
two games (`snake`, `gibbon`), and the `app` package that selects and launches
them.

| Path                          | Responsibility                                              |
| ----------------------------- | ----------------------------------------------------------- |
| `Cargo.toml`                  | Workspace root: members `engine` + `snake` + `gibbon` + `app`, shared profiles |
| `engine/Cargo.toml`           | `engine` package manifest (miniquad, font8x8, png)          |
| `engine/build_assets.rs`      | Shared build-script helper: embeds a crate's own `assets/`; `include!`-ed by each crate's `build.rs` |
| `engine/src/lib.rs`           | Engine module wiring (`app`, `color`, `font`, `input`, `present`, `render`, `scene`, `sprites`) |
| `engine/src/sprites.rs`       | `Sprite`/`SpriteSheet` decode + `RleSprite` (RLE) blit      |
| `engine/src/input.rs`         | Device-aware Android gamepad queue + `surfaceOnPlayerKey` JNI export |
| `engine/src/color.rs`           | `Color`, `Palette`: classic 16-color default + dynamic `add` (up to 254 slots, 255 = transparent) |
| `engine/src/render.rs`          | `Renderer` trait, indexed `Framebuffer` + per-scene palette, `integer_scale` |
| `engine/src/render.rs`          | `Renderer` trait, indexed `Framebuffer` + per-scene palette, `integer_scale` |
| `engine/src/font.rs`          | 8x8 bitmap font blitting (font8x8), `text_width`            |
| `engine/src/present.rs`       | `Presenter`: index + palette texture upload, palette-lookup shader, integer-scaled fullscreen quad |
| `engine/src/app.rs`           | `Stage`: miniquad `EventHandler`, input, HUD               |
| `engine/src/scene.rs`         | `Scene` trait + `SceneAction` (scenes decouple the games from the shell) |
| `snake/Cargo.toml`            | `snake` package manifest (engine + rand)                    |
| `snake/build.rs`              | Embeds `snake/assets/` via `engine/build_assets.rs`         |
| `snake/src/lib.rs`            | Module wiring (`assets`, `game`, `play`, `snake`)           |
| `snake/src/assets.rs`         | `snake` asset registry: look up embedded assets by file name |
| `snake/assets/apple_rotate.png` | Snake's bundled food sprite sheet                        |
| `snake/src/snake.rs`          | Reusable `Snake`: body, direction, input queue, growth, drawing |
| `snake/src/game.rs`           | `Game`: two snakes, shared food, tick clock, per-player input, drawing |
| `snake/src/palette.rs`        | `snake`'s palette: the game's fixed colors added over the 16-color default |
| `snake/src/play.rs`           | `Playing` scene: `Game` + fixed-timestep accumulator + pause |
| `gibbon/Cargo.toml`           | `gibbon` package manifest (engine + rand)                   |
| `gibbon/build.rs`             | Embeds `gibbon/assets/` via `engine/build_assets.rs`        |
| `gibbon/src/lib.rs`           | Module wiring (`assets`, `game`, `level`, `palette`, `play`) |
| `gibbon/src/assets.rs`        | `gibbon` asset registry: look up embedded assets by file name |
| `gibbon/assets/apple_rotate.png` | Gibbon's bundled fruit sprite sheet (12 frames)          |
| `gibbon/assets/gibbon.png`    | Gibbon character sheet (5 frames: right0, right1, left0, left1, climb) |
| `gibbon/assets/guard.png`     | Guard character sheet (same 5-frame layout)                 |
| `gibbon/assets/levels/lvlN.txt` | Text levels parsed into 20x11 grids (`@` fruit, `s` gibbon spawn, `g` guard spawn, `|` ladder, `-` railing, `#` wood, `*` brick) |
| `gibbon/examples/gen_assets.rs` | Regenerates `gibbon.png` / `guard.png` pixel art          |
| `gibbon/src/level.rs`         | `Level` + parser (pads/clips, skips levels without a spawn) + a BFS fruit-reachability test |
| `gibbon/src/game.rs`          | Lode Runner `Game`: gibbon, guards, digging, physics, fruit collection |
| `gibbon/src/palette.rs`       | `gibbon`'s palette: warm-theme fixed colors over the 16-color default |
| `gibbon/src/play.rs`          | `Playing` scene: loads sprites, holds the `Game`, HUD, pause, dig input |
| `app/Cargo.toml`              | `app` package manifest (engine + miniquad + snake + gibbon) |
| `app/build.rs`                | Embeds `app/assets/` via `engine/build_assets.rs`           |
| `app/src/lib.rs`              | Module wiring, `Conf`, `desktop_main()`, Android `quad_main` |
| `app/src/main.rs`             | Calls `app::desktop_main()` (desktop only)                  |
| `app/src/assets.rs`           | `app` asset registry: look up embedded assets by file name  |
| `app/assets/`                 | Main-menu assets (currently empty; the registry is ready)   |
| `app/src/game_select.rs`      | `GameSelect` screen + `GameKind`: choose the game; confirming creates the chosen game's instance |
| `app/src/menu.rs`             | `Menu` scene + `PendingGame`: player-count selection, starts the held game |
| `android/java/.../MainActivity.java` | Android activity glue (SurfaceView + lifecycle + gamepad device->player routing) |
| `android/java/quad_native/QuadNative.java` | JNI declarations matched by miniquad 0.4 + `surfaceOnPlayerKey` |
| `android/AndroidManifest.xml` | Manifest (regular Activity, TV leanback launcher)           |
| `scripts/build-apk.sh`        | cargo-ndk + javac/d8 + aapt2/zipalign/apksigner             |

## Snake (`snake/src/snake.rs`)

The `snake` package is a self-contained library for the two-player snake game;
the `gibbon` package is a separate, single-player Lode Runner-style game (see
below). The app package selects between them.

* `Snake` is a pure, reusable unit: a body (`Vec<Segment>`), direction, the
  input ring buffer (`MAX_QUEUED_INPUTS = 3`), face state and a body color.
  It owns no food, no RNG and no timing.
* `Snake::spawn(color, head, direction)` builds a 4-cell snake with the body
  trailing opposite the facing direction (player 1 spawns top-left facing
  right, player 2 bottom-right facing left).
* `move_tick(food)` advances one board move (wrap at the edges, eat food, grow)
  and sets `grew_last_tick`; the caller decides when to call it and respawns
  the food. `queue_direction`/`set_direction` buffer/apply turns (repeats and
  reversals are rejected).
* `draw()` renders body + interpolated head (eyes/tongue) through `Renderer`.

## Game (`snake/src/game.rs`)

* Fixed simulation step: 60 Hz (`SIM_STEP_HZ`), snakes move every 0.5 s
  (`TICK_STEPS = 30` steps per move), in lockstep, sharing one food.
* `Game` owns `Vec<Snake>` (up to `PLAYERS = 2`), the shared `food`, the RNG
  (food respawn off any snake body) and the shared tick clock/`alpha()`.
  `Game::new(players)` spawns 1 or 2 snakes; the menu chooses the count via
  `Playing::start`.
* Colors are drawn as the palette's exact entries, so `game.rs`/`snake.rs` draw
  with colors imported from the game's `palette.rs` (see
  `snake/src/palette.rs`, `gibbon/src/palette.rs`), which `Playing::new` folds
  into the scene palette.
* Board: `GRID_SIZE_X = 20` x `GRID_SIZE_Y = 11` cells, 0-based coords
  (`x` in 0..20, `y` in 0..11, top-left origin), edges wrap.
* Direction changes are buffered in a small ring buffer
  (`MAX_QUEUED_INPUTS = 3`); repeats and reversals are rejected.
* Movement is interpolated between ticks using fixed-point `alpha`
  (0..=65536) so rendering stays smooth while simulation stays deterministic.
* The sim is pure Rust (no Bevy/GPU types) and draws through the `Renderer`
  trait, so it runs identically in desktop, Android, and tests.
* Every frame the framebuffer is fully repainted (clear + grid + snakes + food +
  HUD), so no incremental/dirty tracking is needed.

## Gibbon (`gibbon/src/level.rs`, `gibbon/src/game.rs`)

The `gibbon` package is a single-player Lode Runner-style game: run and climb a
20x11 grid, collect every fruit before the two guards catch you.

* Fixed simulation step: 60 Hz, the gibbon acts every `SIM_FRAMES = 10` frames
  (6 moves per second); guards advance every 2 gibbon moves.
* Tiles: `Wood` (#) and `Brick` (*) are solid, `Ladder` (|) and `Railing` (-)
  are climbable, `Fruit` (@) is collectible, everything else is empty.
* Movement: left/right into any non-solid cell; up/down only into a ladder or
  railing. An actor is *supported* when the cell below is solid, or it stands
  on (or hangs from) a ladder/railing; otherwise gravity pulls it one cell per
  tick. Fruits are collected the moment the gibbon passes through their cell,
  including mid-fall.
* Digging: holding Down + Left/Right (edge-triggered, once per combination)
  digs the wood tile diagonally below, opening a hole for 10 seconds
  (`DIG_TICKS = 60` moves) before it regrows (deferred while a character stands
  in it). Only possible while standing on solid ground, and only against wood.
* Guards chase the gibbon — guard 0 minimizes the vertical distance first,
  guard 1 the horizontal — falling back to a random legal move so they never
  freeze. Catching the gibbon costs a life (`LIVES = 3`): it respawns at the
  spawn after a short delay; at 0 lives the game is over.
* A level clears once the last fruit is collected; the game advances to the
  next embedded level, and completing the final one is a win.
* Levels are plain text (see `gibbon/src/level.rs`), parsed at build time into
  embedded `Level`s. The parser pads short rows, clips oversized ones and
  skips levels without a gibbon spawn.
* A `#[cfg(test)]` BFS model in `level.rs` mirrors the game's movement and dig
  rules to assert that every embedded level has all its fruits reachable
  (including fruits collected while falling through a dug hole), so broken
  level files fail the test suite.

## Rendering (`engine/src/render.rs`, `engine/src/font.rs`)

* `Renderer` trait exposes only integer, top-left-origin calls, including
  `draw_rle_image` for sprite blits.
* `Framebuffer` is a 480x270 indexed `Vec<u8>`: each pixel is one 8-bit index
  into a 256-entry `Palette` (a third of the old RGB8 buffer). Each game owns
  its own palette (see "Game lifecycle & palettes"); the framebuffer's palette
  is swapped to the active scene's. `Palette::add` appends a color, deduping
  exact matches and falling back to the nearest defined color once 254 slots
  are taken (index 255 is reserved for transparency); `Palette::index_of` finds
  the exact entry. All shapes are clipped.
* `integer_scale(w, h)` returns the largest integer scale that fits:
  `min(w/480, h/270).max(1)`.
* Text uses the public-domain `font8x8::legacy::BASIC_LEGACY` bitmap font
  (8x8 glyphs, bit 7 = leftmost column, non-ASCII falls back to `?`).

## Assets & sprites (`engine/build_assets.rs`, `engine/src/sprites.rs`)

* The engine ships no game assets. Every game/screen crate (`snake`, `gibbon`,
  `app`) owns its own `assets/` directory and embeds it with a three-line
  `build.rs` that `include!`s the shared `engine/build_assets.rs` helper.
* The helper scans `<crate>/assets/` and emits a static `(name, bytes)` table
  that is `include!`-ed into the crate's `src/assets.rs`, so every asset is
  embedded in the binary and loadable by file name on desktop, Android and in
  tests. The games hand the bytes to `SpriteSheet::from_png` to decode.
* `SpriteSheet::from_png(bytes, &mut palette, size_x, size_y, sprite_count)`
  decodes a PNG (via the `png` crate) into RGBA8, quantizes it against the
  game's palette (`Palette::quantize_rgba` — adding each color to the palette,
  so the sprite owns exact entries) and crops a horizontal strip of
  `size_x` x `size_y` frames; `SpriteSheet::sprite(i)` returns a `Sprite`.
* `SpriteSheet::to_rle()` encodes every frame once at load time into an
  `RleSprite` (a compact run-length stream of palette indices), which is what
  the game draws with: `RleSprite::draw(&mut renderer, x, y)` blits the frame
  through `Renderer::draw_rle_image`, writing only opaque runs so transparent
  pixels never touch the framebuffer.
* Each game's fruit uses its own embedded `apple_rotate.png` sheet (12 frames of
  24x24, one board cell), animated one frame per move tick. Gibbon also embeds
  its character sheets (`gibbon.png`, `guard.png`: 5 frames each, animated from
  the walking frames) and its text levels, loaded via `level::load_all()`.

## Presentation (`engine/src/present.rs`)

* Two textures: a 480x270 index texture (`FilterMode::Nearest`, no mipmaps)
  holding the framebuffer's 8-bit palette indices, and a 256x1 RGB8 palette
  texture seeded with the default palette. When the active scene's palette
  changes the shell calls `Presenter::set_palette` to re-upload it.
* The fragment shader samples the index texture, multiplies the red channel
  by 255 to recover the index, and looks the color up in the palette texture,
  so the CPU never expands indices to RGB.
* The index texture is an 8-bit red (R8) texture where the backend supports it
  (desktop GL 3+, GLES3). GLES2 (the Android context) and WebGL1 cannot do R8,
  so there the presenter replicates each index across an RGB8 texture before
  upload; the shader reads `.r` either way.
* Per frame: one index texture upload then a single fullscreen quad with vertex
  positions recomputed on resize to center the integer-scaled viewport.
  `Presenter::present(fb, clear_color)` paints the letterbox bars with the
  active scene's clear color — the default palette's black, or each game's
  background color via `Scene::clear_color` — so they blend with each game's
  frame edge.

## Game lifecycle & palettes

* A game instance (its decoded food sprites + palette) is created at the
  **game-selection screen**: confirming `SNAKE`/`GIBBON` calls
  `Playing::new()` and hands the instance to the player-count menu as a
  `PendingGame`. Only the chosen game's palette and sprites are in memory at
  any time.
* The player-count menu holds the instance; Confirm calls `Playing::start(n)`
  (spawning the snakes — the gibbon ignores the count, being single-player)
  and pushes the `Playing` scene. Back pops the menu, dropping the instance;
  Back inside the game (`PopToRoot`) drops it too.
* Each game owns a `Palette` built as the classic 16-color default (fixed
  slots 0-15: black, blue, green, cyan, red, magenta, brown, light gray, gray,
  and the bright variants; index 255 = transparent) plus the game's own fixed
  colors via `Palette::add` (snake: `BG`, `GRID`, snake colors, `EYE`, `TONGUE`,
  `HUD`; gibbon: `BG`, wood/brick/ladder/railing colors, gibbon/guard colors,
  `HUD`). The games' background colors live in `snake/src/palette.rs`
  (dark blue-black) and `gibbon/src/palette.rs` (warm purple-black), imported
  by the game modules and drawn through `Renderer`.
* `Playing::new` builds the game palette first, then decodes the sprite sheets
  against it (`from_png(&mut palette)`), which appends the sprites'
  colors. The `Playing` scene keeps that palette and reports it as
  `Scene::palette()`, so framebuffer indices always match the loaded sprites.
* `Stage` swaps the framebuffer and presenter palette to `scene.palette()`
  before each `draw`; the HUD text uses the fixed light-gray slot (always in
  the default), and the letterbox uses `Scene::clear_color()` (default black;
  each game returns its `BG`). Each game also paints its background in
  `Playing::draw`, so the whole frame comes from one palette.

## App loop (`engine/src/app.rs`)

* `Stage` implements `miniquad::EventHandler` (`update`/`draw`) and keeps a
  scene stack. Scene flow (in the `app` package): `GameSelect` (choose `SNAKE`
  or `GIBBON`) -> `Menu` (player-count selection: 1 or 2 for snake, 1 only for
  gibbon) -> the chosen game's `Playing` scene (`snake::play::Playing` or
  `gibbon::play::Playing`). Confirming a game on `GameSelect` creates the
  game instance immediately; the `Menu` holds it and starts it on Confirm. The
  selection screens are navigated with the direction keys (selection cycles)
  and confirmed with Enter/OK; Back pops one level (`Menu` returns to
  `GameSelect`, dropping the game), and Back inside a game returns to the root
  `GameSelect` (`PopToRoot`, also dropping the game). Back on `GameSelect`
  quits the app.
* `Input` (`Input` enum) maps Android TV/desktop keys to game actions.
  Player 1: DPAD/WASD + F1-F4; player 2: IJKL + F5-F8 (desktop) or the second
  gamepad (Android). Enter + Back/Escape + Menu/Space for global actions.
* `Stage` keeps per-player held state (`held: [[bool; INPUT_COUNT]; PLAYERS]`)
  for edge detection and face buttons (A hides the tongue, B closes the eyes).
  Auto-repeat suppression is keyed on the physical key (`held_keys`), so
  holding one key never blocks a different key that maps to the same input.
* A fixed-timestep accumulator advances the sim at 60 Hz regardless of the
  display rate; rendering runs each `draw` with a bounded max frame time.
* HUD shows FPS / window size / logical resolution / scale; refreshes every
  30 frames. Pause toggles on Pause action; window minimize pauses.

## Android input (`engine/src/input.rs`)

* miniquad 0.4.11 has no gamepad/device API, so `GamepadKeyEvent`s travel
  through a small `Mutex<Vec>` queue fed by a custom JNI export
  `Java_quad_1native_QuadNative_surfaceOnPlayerKey` (declared in
  `QuadNative.java`, implemented in `input.rs`). `Stage::update` drains it once
  per frame; on desktop the queue is never fed and drains as a no-op.
* `MainActivity.java` assigns player slots by `InputDevice.getDeviceId()`:
  devices exposing `SOURCE_JOYSTICK` or `SOURCE_BUTTON` are gamepads (analog
  or D-pad/face-button), first-seen = player 0, second = player 1, extras clamp
  to player 1. Gamepad key events go through the device-aware path with raw
  Android keycodes; non-gamepad devices (TV remote, keyboard) keep the legacy
  miniquad key path and control player 0.

## Android

* miniquad 0.4 uses its own Activity-based Java glue (not `NativeActivity`):
  `MainActivity` creates a `SurfaceView`, forwards lifecycle/input via
  `quad_native.QuadNative` JNI calls, and the Rust entry point is the exported
  `quad_main` symbol (`app/src/lib.rs`). The Java glue loads the native
  library as `"app"` (`libapp.so`), built from the `app` crate.
* `AndroidManifest.xml` declares `rust.snake.MainActivity`, targets API 30
  (min 26), and registers both `LAUNCHER` and `LEANBACK_LAUNCHER` intents.
* `scripts/build-apk.sh` cross-compiles `libapp.so` with cargo-ndk, compiles
  the Java glue with `javac` (against the newest platform `android.jar`),
  dexes it with `d8`, then packages/signs the APK without cargo-apk.

## Verification

* `cargo test --workspace` (all packages: app, engine, gibbon, snake).
* `cargo clippy --workspace --all-targets -- -D warnings`
* `cargo fmt --check`
* `cargo check -p app --target armv7-linux-androideabi` /
  `cargo check -p app --target aarch64-linux-android` (no Android SDK required).
* APK build requires the Android SDK/NDK, cargo-ndk and a JDK:
  `just apk` (defaults to `armeabi-v7a`).
