# Target technology stack

The game should run on the Android TV 10
Smart TV Philips 48OLED806/12
The processor type is Quad Core with P5 AI Perfect Picture Engine
Resolution: 3820x2160 (with supported frame rates 40-120Hz)

OpenGL ES/Vulkan support, we aim for a 2D game,
Texture atlases
Low draw-call count
Small asset sizes
GPU-friendly sprite batching

Target game frame rate should be capped at 60Hz

We create a game that is runnable of the ARM architecture, compilation target: 
aarch64-linux-android

The project should still run runnable locally, on the current machine for testing:
x86_64-unknown-linux-gnu


# Installation Flow
Rust source
     |
Android NDK toolchain
     |
libgame.so (ARM64 native code)
     |
APK packaging
     |
Android TV installation


# Compilation for the Android TV game
Rust
 |
cargo
 |
cargo-ndk
 |
Android NDK
 |
APK
 |
Android TV


