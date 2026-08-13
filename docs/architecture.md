# Architecture

The game is a lightweight, integer-based 2D snake game rendered into a 480x270
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
| `engine/build.rs`             | Embeds every file under `engine/assets/` into the binary    |
| `engine/src/lib.rs`           | Engine module wiring (`app`, `assets`, `color`, `font`, `input`, `present`, `render`, `scene`, `sprites`) |
| `engine/src/sprites.rs`       | `Sprite`/`SpriteSheet` decode + `RleSprite` (RLE) blit      |
| `engine/src/input.rs`         | Device-aware Android gamepad queue + `surfaceOnPlayerKey` JNI export |
| `engine/src/render.rs`        | `Renderer` trait, `Color`, `Palette`, indexed `Framebuffer`, `integer_scale` |
| `engine/src/font.rs`          | 8x8 bitmap font blitting (font8x8), `text_width`            |
| `engine/src/present.rs`       | `Presenter`: index + palette texture upload, palette-lookup shader, integer-scaled fullscreen quad |
| `engine/src/app.rs`           | `Stage`: miniquad `EventHandler`, input, HUD               |
| `engine/src/scene.rs`         | `Scene` trait + `SceneAction` (scenes decouple the games from the shell) |
| `engine/src/assets.rs`        | Asset registry: look up embedded assets by file name        |
| `engine/assets/apple_rotate.png` | Bundled sprite sheet asset                               |
| `snake/Cargo.toml`            | `snake` package manifest (engine + rand)                    |
| `snake/src/lib.rs`            | Module wiring (`game`, `play`, `snake`)                     |
| `snake/src/snake.rs`          | Reusable `Snake`: body, direction, input queue, growth, drawing |
| `snake/src/game.rs`           | `Game`: two snakes, shared food, tick clock, per-player input, drawing |
| `snake/src/play.rs`           | `Playing` scene: `Game` + fixed-timestep accumulator + pause |
| `gibbon/Cargo.toml`           | `gibbon` package manifest (engine + rand)                   |
| `gibbon/src/lib.rs`           | Module wiring (`game`, `play`, `snake`)                     |
| `gibbon/src/snake.rs`         | Copy of `snake::snake` (the games can diverge)              |
| `gibbon/src/game.rs`          | Copy of `snake::game` (identical behavior for now)          |
| `gibbon/src/play.rs`          | Copy of `snake::play` (`Playing` scene)                     |
| `app/Cargo.toml`              | `app` package manifest (engine + miniquad + snake + gibbon) |
| `app/src/lib.rs`              | Module wiring, `Conf`, `desktop_main()`, Android `quad_main` |
| `app/src/main.rs`             | Calls `app::desktop_main()` (desktop only)                  |
| `app/src/game_select.rs`      | `GameSelect` screen + `GameKind`: choose the game before the player count |
| `app/src/menu.rs`             | `Menu` scene: player-count selection, starts the chosen game |
| `android/java/.../MainActivity.java` | Android activity glue (SurfaceView + lifecycle + gamepad device->player routing) |
| `android/java/quad_native/QuadNative.java` | JNI declarations matched by miniquad 0.4 + `surfaceOnPlayerKey` |
| `android/AndroidManifest.xml` | Manifest (regular Activity, TV leanback launcher)           |
| `scripts/build-apk.sh`        | cargo-ndk + javac/d8 + aapt2/zipalign/apksigner             |

## Snake (`snake/src/snake.rs`)

The `snake` package is a self-contained library for the snake game. The
`gibbon` package is a byte-for-byte copy of the same modules (`gibbon/src/...`)
so the two games can diverge independently; the app package selects between
them.

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
  `Game::new(players)` spawns 1 or 2 snakes; the menu chooses the count.
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

## Rendering (`engine/src/render.rs`, `engine/src/font.rs`)

* `Renderer` trait exposes only integer, top-left-origin calls, including
  `draw_rle_image` for sprite blits.
* `Framebuffer` is a 480x270 indexed `Vec<u8>`: each pixel is one 8-bit index
  into a 256-entry `Palette` (a third of the old RGB8 buffer). The game's
  colors are pinned at fixed palette indices; `Palette::index_of` finds the
  exact entry (or the nearest entry, used by `Palette::quantize_rgba` when
  sprites are converted to palette indices at load time). All shapes are
  clipped.
* `integer_scale(w, h)` returns the largest integer scale that fits:
  `min(w/480, h/270).max(1)`.
* Text uses the public-domain `font8x8::legacy::BASIC_LEGACY` bitmap font
  (8x8 glyphs, bit 7 = leftmost column, non-ASCII falls back to `?`).

## Assets & sprites (`engine/build.rs`, `engine/src/assets.rs`, `engine/src/sprites.rs`)

* `build.rs` scans `assets/` and emits a static `(name, bytes)` table that is
  `include!`-ed into `src/assets.rs`, so every asset is embedded in the binary
  and loadable by file name on desktop, Android and in tests.
* `SpriteSheet::load(name, size_x, size_y, sprite_count)` decodes a PNG
  (via the `png` crate) into RGBA8 and crops a horizontal strip of
  `size_x` x `size_y` frames; `SpriteSheet::sprite(i)` returns a `Sprite`.
* `SpriteSheet::to_rle()` encodes every frame once at load time into an
  `RleSprite` (a compact run-length stream of palette indices), which is what
  the game draws with: `RleSprite::draw(&mut renderer, x, y)` blits the frame
  through `Renderer::draw_rle_image`, writing only opaque runs so transparent
  pixels never touch the framebuffer.
* The game's food uses the embedded `apple_rotate.png` sheet (12 frames of
  24x24, one board cell), animated one frame per snake move tick.

## Presentation (`engine/src/present.rs`)

* Two textures: a 480x270 index texture (`FilterMode::Nearest`, no mipmaps)
  holding the framebuffer's 8-bit palette indices, and a 256x1 RGB8 palette
  texture built once from the default `Palette`.
* The fragment shader samples the index texture, multiplies the red channel
  by 255 to recover the index, and looks the color up in the palette texture,
  so the CPU never expands indices to RGB.
* The index texture is an 8-bit red (R8) texture where the backend supports it
  (desktop GL 3+, GLES3). GLES2 (the Android context) and WebGL1 cannot do R8,
  so there the presenter replicates each index across an RGB8 texture before
  upload; the shader reads `.r` either way.
* Per frame: one index texture upload then a single fullscreen quad with vertex
  positions recomputed on resize to center the integer-scaled viewport
  (letterbox bars blend with the background color).
* The clear color (13,13,18) matches the game background.

## App loop (`engine/src/app.rs`)

* `Stage` implements `miniquad::EventHandler` (`update`/`draw`) and keeps a
  scene stack. Scene flow (in the `app` package): `GameSelect` (choose `SNAKE`
  or `GIBBON`) -> `Menu` (1 or 2 player selection for the chosen game) -> the
  chosen game's `Playing` scene (`snake::play::Playing` or
  `gibbon::play::Playing`). The selection screens are navigated with the
  direction keys (selection cycles) and confirmed with Enter/OK; Back pops one
  level (`Menu` returns to `GameSelect`), and Back inside a game returns to
  the root `GameSelect` (`PopToRoot`). Back on `GameSelect` quits the app.
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
