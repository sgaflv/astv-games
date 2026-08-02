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
    cargo test

fmt:
    cargo fmt

lint:
    cargo clippy --all-targets --all-features -- -D warnings

clean:
    cargo clean

# --------------------
# Android
# --------------------

# Build signed debug apk (cargo ndk + aapt2/zipalign/apksigner)
apk:
    ./scripts/build-apk.sh

# Build signed release apk
apk-release:
    PROFILE=release ./scripts/build-apk.sh

# Build apk for arm64-v8a
apk-arm64:
    ANDROID_ABI=arm64-v8a ./scripts/build-apk.sh

# Build only native library
ndk:
    cargo ndk -t arm64-v8a build --release

# --------------------
# Install
# --------------------

connect:
    adb connect 192.168.188.54

install:
    adb install -r -d target/apk/snake.apk

tv-run:
    adb shell monkey -p rust.snake 1

tv-stop:
    adb shell am force-stop rust.snake

tv-cancel:
    adb shell input keyevent KEYCODE_HOME
    
log:
    adb logcat

# --------------------
# Combined workflow
# --------------------

deploy:
    just apk
    just install
    just tv-run
