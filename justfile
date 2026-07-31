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

apk:
    cargo apk build --release

# Build only native library
ndk:
    cargo ndk -t arm64-v8a build --release

# --------------------
# Install
# --------------------

install:
    adb install -r target/release/apk/*.apk

run-tv:
    adb shell monkey -p com.example.game 1

log:
    adb logcat

# --------------------
# Combined workflow
# --------------------

deploy:
    just apk
    just install
    just run-tv
