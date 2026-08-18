# Android Smart TV Games

Two games plus a debug tool, selected from the launcher:

- **Snake** — two-player snake (1-2 players).
- **Gibbon** — one- or two-player Lode Runner-style game: collect every fruit on
  each level while outrunning the guards, digging through wood to reach them.
  Levels live in `gibbon/assets/levels/*.txt`.
- **Keys** — debug tool (not a game): a full-screen FPS window that prints the
  currently held keys per player, so keyboards and gamepads can be verified.

# Compiling (desktop)

To compile you will need Rust installed:
[https://rust-lang.org/tools/install/]

Enter project folder and run:
```bash
cargo run
```

# Installing on Android TV

## Prerequisites

1. **Rust** — [https://rust-lang.org/tools/install/]
2. **just** — [https://just.systems/](https://just.systems/) (`cargo install just`)
3. **Android SDK** — with build-tools, platforms/android-30+, and NDK
   (`$ANDROID_HOME` or `$ANDROID_SDK_ROOT` must point to the SDK)
4. **JDK** — `javac` and `d8` must be on `PATH`
5. **cargo-ndk** — `cargo install cargo-ndk`
6. **ADB** — install via your package manager or
   [https://developer.android.com/tools/releases/platform-tools/]

## Add the Rust cross-compilation target

```bash
just targets
```

## Build the APK

Debug build (default):
```bash
just apk
```

Release build:
```bash
just apk-release
```

The APK will be at `target/apk/app.apk`.

## Connect to the TV

Make sure **USB debugging** or **network debugging** is enabled on the TV.

Over USB:
```bash
adb devices   # verify the TV appears
```

Over Wi-Fi (update the IP to match your TV):
```bash
just connect
```

## Install and run

```bash
just install    # adb install -r -d target/apk/app.apk
just tv-run     # launch the game
```

Or do all three steps (build + install + run) at once:
```bash
just deploy
```

## Other TV commands

| Command | Description |
|---------|-------------|
| `just tv-stop` | Force-stop the app |
| `just tv-cancel` | Send HOME key (return to launcher) |
| `just tv-power` | Toggle TV power |
| `just tv-volume-up` | Volume up |
| `just tv-volume-down` | Volume down |
| `just tv-mute` | Mute |
| `just log` | Stream adb logcat |


