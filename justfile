# Default recipe
default:
    @just --list

# Local run
run:
    cargo run

# Local release build
release:
    cargo build --release


check:
    cargo check

test:
    cargo test --workspace

fmt:
    cargo fmt

lint:
    cargo clippy --all-targets --all-features -- -D warnings

clean:
    cargo clean

# --------------------
# Android
# --------------------

# Rust cross targets needed for Android builds
targets:
    rustup target add armv7-linux-androideabi

# cargo check for both Android targets (no SDK required)
check-android:
    cargo check -p app --target armv7-linux-androideabi

# Build signed debug apk (cargo ndk + javac/d8 + aapt2/zipalign/apksigner)
apk:
    PROFILE=release ./scripts/build-apk.sh

# Build apk for arm64-v8a
apk-arm64:
    ANDROID_ABI=arm64-v8a ./scripts/build-apk.sh

# Build only native library
ndk:
    cargo ndk -t arm64-v8a build -p app --release

# --------------------
# Install
# --------------------

connect:
    adb connect 192.168.188.54

install:
    adb install -r -d target/apk/app.apk

tv-run:
    adb shell monkey -p rust.snake 1

tv-stop:
    adb shell am force-stop rust.snake

tv-cancel:
    adb shell input keyevent KEYCODE_HOME

tv-power:
    adb shell input keyevent KEYCODE_POWER
    
tv-volume-up:
	adb shell input keyevent KEYCODE_VOLUME_UP

tv-volume-down:
	adb shell input keyevent KEYCODE_VOLUME_DOWN

tv-mute:
	adb shell input keyevent KEYCODE_VOLUME_MUTE

tv-volume:
	adb shell cmd media_session volume --stream 3 --set 10    
    
log:
    adb logcat

# --------------------
# Combined workflow
# --------------------

deploy:
    just apk
    just install
    just tv-run
