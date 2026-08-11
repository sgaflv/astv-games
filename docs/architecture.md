# Architecture

The game is a lightweight, integer-based 2D snake game rendered into a 480x270
CPU framebuffer and presented via miniquad/OpenGL ES with integer
nearest-neighbour scaling. Bevy has been removed completely.

## Pipeline

```text
Game logic (integer coords, fixed 60 Hz timestep)
      │
      ▼
Renderer trait (clear / fill_rect / fill_circle / draw_text)
      │
      ▼
480×270 RGB8 CPU framebuffer (388,800 B ≈ 380 KiB)
      │
      ▼
one texture upload + one fullscreen quad (nearest-neighbour, integer scale)
      │
      ▼
physical display (1920×1080 ×4, 3840×2160 ×8, or letterboxed)
```

## Crate layout

| Path                          | Responsibility                                              |
| ----------------------------- | ----------------------------------------------------------- |
| `src/lib.rs`                  | Module wiring, `Conf`, `desktop_main()`, Android `quad_main` |
| `src/main.rs`                 | Calls `snake::desktop_main()` (desktop only)                |
| `src/game.rs`                 | Snake simulation + fixed timestep + drawing through `Renderer` |
| `src/render.rs`               | `Renderer` trait, `Color`, `Framebuffer`, `integer_scale`   |
| `src/font.rs`                 | 8x8 bitmap font blitting (font8x8), `text_width`            |
| `src/present.rs`              | `Presenter`: GL texture upload + integer-scaled fullscreen quad |
| `src/app.rs`                  | `Stage`: miniquad `EventHandler`, input, pacing, HUD        |
| `android/java/.../MainActivity.java` | Android activity glue (SurfaceView + lifecycle)        |
| `android/java/quad_native/QuadNative.java` | JNI declarations matched by miniquad 0.4            |
| `android/AndroidManifest.xml` | Manifest (regular Activity, TV leanback launcher)           |
| `scripts/build-apk.sh`        | cargo-ndk + javac/d8 + aapt2/zipalign/apksigner             |

## Game (`src/game.rs`)

* Fixed simulation step: 60 Hz (`SIM_STEP_HZ`), snake moves every 0.5 s
  (`TICK_STEPS = 30` steps per move).
* Board: `GRID_SIZE_X = 40` x `GRID_SIZE_Y = 12` cells, 0-based coords
  (`x` in 0..40, `y` in 0..12, top-left origin), edges wrap.
* Direction changes are buffered in a small ring buffer
  (`MAX_QUEUED_INPUTS = 3`); repeats and reversals are rejected.
* Movement is interpolated between ticks using fixed-point `alpha`
  (0..=65536) so rendering stays smooth while simulation stays deterministic.
* The sim is pure Rust (no Bevy/GPU types) and draws through the `Renderer`
  trait, so it runs identically in desktop, Android, and tests.
* Every frame the framebuffer is fully repainted (clear + grid + snake + food +
  HUD), so no incremental/dirty tracking is needed.

## Rendering (`src/render.rs`, `src/font.rs`)

* `Renderer` trait exposes only integer, top-left-origin calls.
* `Framebuffer` is a 480x270 RGB8 `Vec<u8>`; all shapes are clipped.
* `integer_scale(w, h)` returns the largest integer scale that fits:
  `min(w/480, h/270).max(1)`.
* Text uses the public-domain `font8x8::legacy::BASIC_LEGACY` bitmap font
  (8x8 glyphs, bit 7 = leftmost column, non-ASCII falls back to `?`).

## Presentation (`src/present.rs`)

* One 480x270 RGB8 `TextureId`, `FilterMode::Nearest`, no mipmaps.
* Per frame: `texture_update(framebuffer)` then a single fullscreen quad with
  vertex positions recomputed on resize to center the integer-scaled viewport
  (letterbox bars blend with the background color).
* The clear color (13,13,18) matches the game background.

## App loop (`src/app.rs`)

* `Stage` implements `miniquad::EventHandler` (`update`/`draw`).
* Input (`Input` enum) maps Android TV/desktop keys to game actions
  (DPAD/WASD + Enter + Back/Escape + Menu/Space for pause).
* Fixed timestep accumulator advances the sim at 60 Hz; rendering runs each
  draw and is paced to ~60 FPS with a bounded max frame time.
* HUD shows FPS / window size / logical resolution / scale; refreshes every
  30 frames. Pause toggles on Pause action; window minimize pauses.

## Android

* miniquad 0.4 uses its own Activity-based Java glue (not `NativeActivity`):
  `MainActivity` creates a `SurfaceView`, forwards lifecycle/input via
  `quad_native.QuadNative` JNI calls, and the Rust entry point is the exported
  `quad_main` symbol (`lib.rs`).
* `AndroidManifest.xml` declares `rust.snake.MainActivity`, targets API 30
  (min 26), and registers both `LAUNCHER` and `LEANBACK_LAUNCHER` intents.
* `scripts/build-apk.sh` cross-compiles `libsnake.so` with cargo-ndk, compiles
  the Java glue with `javac` (against the newest platform `android.jar`),
  dexes it with `d8`, then packages/signs the APK without cargo-apk.

## Verification

* `cargo test` — simulation, framebuffer and font unit tests.
* `cargo clippy --all-targets -- -D warnings`
* `cargo fmt --check`
* `cargo check --target armv7-linux-androideabi` /
  `cargo check --target aarch64-linux-android` (no Android SDK required).
* APK build requires the Android SDK/NDK, cargo-ndk and a JDK:
  `just apk` (defaults to `armeabi-v7a`).
