# Goal: Replace Bevy with a Lightweight Integer-Based 2D Renderer

## Objective

Replace the current Bevy-based rendering/game architecture with a lightweight Rust 2D architecture optimized for the target Philips 48OLED806/12 Android TV.

The primary goals are:

* Reduce rendering and framework overhead.
* Remove unnecessary Bevy/wgpu/render-graph complexity.
* Use integer-based game coordinates.
* Use a top-left `(0, 0)` screen origin.
* Make the game's logical resolution independent of the physical TV resolution.
* Target pixel-perfect rendering with integer scaling.
* Minimize CPU/GPU synchronization and frame latency.
* Preserve the existing game's gameplay behavior and visual appearance as closely as practical.
* Produce an Android TV APK targeting the actual device architecture.

Do **not** optimize prematurely by manually optimizing individual arithmetic operations. The primary optimization is architectural: simplify the rendering pipeline and reduce the amount of data processed.

---

# Target Hardware

The primary target is:

**Philips 48OLED806/12**

Known/reported platform characteristics:

* Android TV 10
* Android API level 29
* MediaTek MT5895 / MT9970 platform
* 4 × ARM Cortex-A73
* ARMv8-A CPU architecture
* 32-bit Android userspace/application ABI
* `armeabi-v7a`
* Approximately 3 GB system RAM
* ARM Mali-G52 MC2 GPU
* OpenGL ES 3.x capability reported for this platform
* Physical panel: 3840×2160
* TV supports up to 120 Hz input/display modes

The implementation must not assume that the Android userspace is 64-bit.

Primary Rust target:

```text
armv7-linux-androideabi
```

Primary Android native ABI:

```text
armeabi-v7a
```

Before finalizing the implementation, verify the actual device with ADB:

```bash
adb shell getprop ro.product.cpu.abi
adb shell getprop ro.product.cpu.abilist
adb shell getprop ro.product.cpu.abilist32
adb shell getprop ro.product.cpu.abilist64
adb shell getprop ro.board.platform
adb shell getprop ro.hardware
adb shell getprop ro.build.version.sdk
adb shell getprop ro.build.version.release
```

If the actual device contradicts any of the assumptions above, document the discrepancy and adapt the implementation accordingly.

---

# Rendering Architecture

Do not replace Bevy with another large game engine.

The preferred architecture is:

```text
                    Game
                     │
             integer coordinates
                     │
                     ▼
             Lightweight 2D API
                     │
             ┌───────┴────────┐
             │                │
        game rendering     UI/debug
             │
             ▼
       miniquad / equivalent
             │
             ▼
         OpenGL ES
             │
             ▼
        Mali-G52 GPU
             │
             ▼
      Android TV display
```

Preferred rendering backend:

**miniquad + OpenGL ES**

Alternative lightweight backend may be used if miniquad proves incompatible with the target Android TV environment.

Do not introduce Bevy, wgpu, Fyrox, or another heavyweight engine as a replacement.

SDL2 is an acceptable alternative if it provides materially better Android TV compatibility, input handling, or graphics integration.

---

# Logical Game Resolution

The game must use a logical rendering resolution of:

```text
480 × 270
```

This resolution is intentional.

It has a 16:9 aspect ratio and scales exactly to both 1920×1080 and 3840×2160:

```text
480 × 270
    ×4
1920 × 1080
    ×8
3840 × 2160
```

Therefore every logical pixel can correspond to exactly:

```text
4×4 pixels at 1080p
8×8 pixels at 4K
```

No fractional scaling is permitted.

Do not use:

```text
640 × 480
```

because it is 4:3 and cannot scale uniformly to 16:9.

Do not use:

```text
768 × 432
```

because 1920 / 768 = 2.5, producing non-integer scaling.

---

# Coordinate System

The game world/rendering coordinate system must use:

```text
origin = top-left
x increases → right
y increases → down
```

Coordinates should be integer-based.

Example:

```rust
struct Position {
    x: i32,
    y: i32,
}
```

Avoid floating-point coordinates for normal 2D game positioning.

Use integer coordinates for:

* sprite positions
* tile positions
* UI positions
* collision rectangles where practical
* screen-space coordinates
* camera offsets where practical

Floating-point values may still be used internally where genuinely useful, e.g.:

* physics requiring fractional velocity
* interpolation
* trigonometry
* shader calculations
* GPU coordinate conversion

However, the public 2D rendering API should operate in integer logical coordinates.

Example desired API:

```rust
renderer.draw_sprite(sprite, x, y);
renderer.draw_rect(x, y, width, height, color);
renderer.draw_text(font, x, y, text);
```

rather than requiring callers to provide normalized device coordinates or centered floating-point coordinates.

---

# Pixel Alignment

Sprites must be rendered on integer logical coordinates.

Avoid subpixel positioning unless explicitly required by a game effect.

For pixel-art assets:

* use nearest-neighbor filtering
* do not introduce bilinear filtering
* avoid texture bleeding from adjacent atlas sprites
* use appropriate texture padding/extrusion where required
* ensure texture coordinates do not introduce half-pixel sampling artifacts

The visual objective is stable, deterministic pixel placement.

---

# Rendering Strategy

Use a low-resolution render target:

```text
480 × 270
```

The game should render into this logical resolution.

The final image is then upscaled to the physical/application output resolution.

The upscale must use:

```text
nearest-neighbor
```

and an integer scale factor.

Preferred presentation modes:

```text
480×270 → 1920×1080   ×4
480×270 → 3840×2160   ×8
```

If the Android surface is not exactly one of these resolutions, preserve aspect ratio and use the largest available integer scale that fits, with letterboxing if necessary.

Never distort the 16:9 image to fit an arbitrary aspect ratio.

---

# Important: Do Not Render the Game Twice

Rendering at 480×270 and then presenting at 1920×1080 does **not** mean rendering all game objects twice.

The intended pipeline is:

```text
Game objects
    │
    ▼
480×270 render target
    │
    ▼
single fullscreen textured quad
    │
    ▼
1920×1080 / 3840×2160
```

The second operation is only texture presentation/upscaling.

Do not redraw sprites, tilemaps, effects, etc. at the physical resolution.

---

# GPU Rendering Model

Prefer batching.

The renderer should attempt to minimize:

* draw calls
* texture binds
* pipeline/state changes
* CPU/GPU synchronization
* framebuffer changes

Use texture atlases where appropriate.

For a typical 2D frame, aim for:

```text
game rendering
    ↓
one or a small number of batched sprite draws
    ↓
480×270 framebuffer
    ↓
one fullscreen quad
```

Do not create one GPU draw call per sprite unless profiling demonstrates that this is harmless.

---

# Buffering

Use double buffering where the rendering API/platform requires explicit application-managed buffers.

Conceptually:

```text
Front buffer → currently being displayed
Back buffer  → currently being constructed
```

At frame completion:

```text
swap(front, back)
```

Do not copy the entire framebuffer merely to swap frames.

If the GPU/Android presentation system already provides swapchain buffering, do not introduce an unnecessary additional CPU-side buffer solely for the sake of double buffering.

Avoid unnecessary triple buffering because additional queued frames can increase input-to-display latency.

Prioritize:

1. no tearing
2. no CPU/GPU stalls
3. low input latency
4. stable frame pacing

over maximizing the number of queued frames.

---

# CPU Software Rendering

A CPU framebuffer implementation may be used if it provides a clear advantage for specific rendering effects.

If a CPU framebuffer is used, its logical resolution is still:

```text
480 × 270
```

RGBA32:

```text
480 × 270 × 4
= 518,400 bytes
≈ 506 KiB
```

Two buffers therefore require approximately:

```text
1.01 MiB
```

This is entirely acceptable for the target device.

CPU software rendering must not upload a 1920×1080 or 3840×2160 framebuffer every frame.

If uploading a CPU framebuffer to the GPU, upload only the 480×270 image and use the GPU for final nearest-neighbor scaling.

---

# GPU Render Target Alternative

The preferred implementation should benchmark both of these possibilities:

### A. GPU-native low-resolution renderer

```text
Game
 ↓
480×270 GPU framebuffer
 ↓
fullscreen quad
 ↓
display
```

### B. CPU framebuffer renderer

```text
Game
 ↓
480×270 CPU framebuffer
 ↓
texture upload
 ↓
fullscreen quad
 ↓
display
```

Select the implementation based on measured performance and latency on the target TV.

For ordinary sprite-heavy games, prefer GPU rendering.

For deliberately pixel-by-pixel effects, a CPU framebuffer may be advantageous.

---

# Game Loop

Use a deterministic fixed timestep for game simulation.

Recommended:

```text
simulation timestep = 1/60 second
```

The game should separate:

```text
input
 ↓
simulation
 ↓
render
 ↓
presentation
```

Do not make game physics depend directly on the variable rendering frame rate.

The renderer may run at the display refresh rate while the simulation remains deterministic.

For example:

```text
60 Hz simulation
60/120 Hz presentation
```

If 120 Hz presentation is supported, do not duplicate simulation steps merely because the display is refreshing at 120 Hz.

---

# Frame Pacing

The implementation should target:

```text
60 FPS
```

initially.

That means:

```text
16.67 ms/frame
```

The system should be designed so that 120 Hz presentation can be supported later, but 120 FPS is not a requirement for the first implementation.

At 120 Hz:

```text
8.33 ms/frame
```

would be available for a fully independent 120 FPS game loop, but do not sacrifice stability and input latency at 60 FPS merely to pursue 120 FPS.

---

# Resolution and Pixel Mapping

The renderer must correctly handle both:

```text
1920×1080
```

and:

```text
3840×2160
```

Output mapping:

```text
480×270 → 1920×1080
scale = 4

480×270 → 3840×2160
scale = 8
```

The renderer must calculate the output viewport dynamically.

Example:

```rust
let scale = min(
    output_width / 480,
    output_height / 270,
);
```

The resulting viewport should be centered if letterboxing is necessary.

Do not use fractional scale factors.

---

# Separation of Game and Renderer

The game logic must not depend directly on OpenGL, miniquad, Android, or GPU-specific types.

Use a small renderer abstraction.

Example:

```rust
trait Renderer {
    fn begin_frame(&mut self);
    fn draw_sprite(&mut self, sprite: SpriteId, x: i32, y: i32);
    fn draw_rect(&mut self, rect: Rect, color: Color);
    fn draw_text(&mut self, font: FontId, x: i32, y: i32, text: &str);
    fn end_frame(&mut self);
}
```

The exact API can differ, but the architectural principle is mandatory:

```text
Game
  ↓
Renderer abstraction
  ↓
Graphics backend
```

rather than:

```text
Game
  ↓
OpenGL/miniquad calls everywhere
```

---

# Asset Requirements

Sprites should be designed around the 480×270 logical coordinate system.

Texture filtering:

```text
NEAREST
```

for pixel-art assets.

Texture atlases should be preferred over many independent textures.

The renderer should avoid texture bleeding at atlas boundaries.

Large background images should not be unnecessarily rendered at 3840×2160 if the logical game resolution is only 480×270.

---

# Android TV Requirements

The resulting application must:

* run on Android TV 10
* target `armeabi-v7a`
* package the Rust native library correctly
* start without requiring touch input
* support TV remote/game-controller input
* handle Android activity lifecycle correctly
* pause/resume cleanly
* handle surface creation/destruction
* recreate graphics resources after surface loss where required
* not assume a phone touchscreen exists

Input should be abstracted behind a game-level input API.

Example:

```rust
enum Action {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Pause,
}
```

Map Android TV remote/controller events to these actions.

---

# Performance Requirements

The implementation should be benchmarked on the actual Philips TV rather than relying on desktop benchmarks.

Measure at minimum:

* CPU frame time
* GPU frame time if available
* render submission time
* texture upload time
* number of draw calls
* number of texture binds
* memory usage
* frame pacing
* input-to-render latency where measurable

Initial target:

```text
60 FPS sustained
```

with no recurring frame drops under normal gameplay.

A frame should ideally complete comfortably below:

```text
16.67 ms
```

rather than merely averaging below it.

Avoid allocations during the normal frame loop.

---

# Migration Strategy

Do not rewrite the entire game in one step.

Use these stages:

## Stage 1 — Renderer abstraction

Introduce a game-level rendering interface.

Move game code away from direct Bevy rendering APIs.

Keep Bevy temporarily if necessary.

Acceptance criterion:

```text
Game logic no longer requires Bevy rendering types.
```

## Stage 2 — Integer coordinate system

Introduce:

```rust
Position { x: i32, y: i32 }
```

and related integer-based geometry.

Convert the game from centered/floating-point screen coordinates to:

```text
(0,0) = top-left
x → right
y → down
```

Acceptance criterion:

```text
Existing gameplay remains visually/functionally equivalent.
```

## Stage 3 — 480×270 logical resolution

Make:

```text
480×270
```

the canonical game coordinate space.

Acceptance criterion:

```text
All gameplay and rendering positions are expressed in logical coordinates.
```

## Stage 4 — Implement lightweight renderer

Implement the new renderer using miniquad/OpenGL ES or another validated lightweight backend.

Implement:

* sprite rendering
* rectangles
* textures
* text
* fullscreen presentation
* nearest-neighbor scaling
* viewport management

## Stage 5 — Remove Bevy rendering

Remove:

* Bevy renderer
* Bevy camera
* Bevy sprite rendering
* Bevy render graph
* Bevy rendering resources

Only retain Bevy components that are genuinely required temporarily during migration.

## Stage 6 — Remove Bevy completely

Once all required functionality has been migrated, remove Bevy dependencies.

Remove unused transitive dependencies and simplify Cargo configuration.

## Stage 7 — Android TV build

Build:

```text
armv7-linux-androideabi
```

package:

```text
armeabi-v7a
```

and deploy to the Philips TV.

## Stage 8 — Benchmark and optimize

Benchmark on the actual TV.

Do not make further architectural changes based solely on desktop performance.

---

# Acceptance Criteria

The migration is complete when all of the following are true:

1. The game runs on the Philips 48OLED806/12.
2. The game runs under Android TV 10.
3. The native application uses the correct 32-bit ARM Android ABI.
4. Bevy is no longer required.
5. The game uses a 480×270 logical resolution.
6. `(0,0)` is the top-left corner.
7. Normal game coordinates are integer-based.
8. Pixel-art rendering uses nearest-neighbor filtering.
9. 480×270 scales exactly to 1920×1080.
10. 480×270 scales exactly to 3840×2160.
11. No fractional scaling occurs.
12. The game does not render all game objects again at the physical display resolution.
13. The final upscale is performed as a simple texture presentation.
14. Rendering uses double buffering where necessary without unnecessary framebuffer copies.
15. The normal frame loop performs no avoidable heap allocations.
16. The game maintains 60 FPS during normal gameplay on the target TV.
17. Input latency is not increased by unnecessary buffering.
18. Android TV lifecycle/surface recreation works correctly.
19. Existing gameplay behavior is preserved.
20. The renderer is isolated from game logic behind a lightweight interface.

---

# Final Architecture Target

The desired end state is approximately:

```text
                    Android TV 10
                         │
                    armeabi-v7a
                         │
                  Cortex-A73 × 4
                         │
              ┌──────────┴──────────┐
              │                     │
           Game logic            Renderer
              │                     │
       integer coordinates      miniquad
              │                     │
              │                OpenGL ES
              │                     │
              └──────────┬──────────┘
                         │
                    480 × 270
                   render target
                         │
                         │ nearest
                         ▼
              ┌──────────────────────┐
              │ 1920×1080 / 4×       │
              │ or                   │
              │ 3840×2160 / 8×       │
              └──────────────────────┘
                         │
                         ▼
                  Mali-G52 MC2
                         │
                         ▼
                 Philips OLED panel
```

The key architectural principle is:

> **Keep the game simple and integer-based; let the GPU perform the small amount of work it is exceptionally good at—batched 2D rasterization and integer texture scaling—while avoiding the general-purpose rendering machinery of a full game engine.**


