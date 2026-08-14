//! The keys debug tool: a minimal scene that prints the currently held
//! logical inputs (per player) and the key mapping, so keyboards and gamepads
//! can be verified. Not a game; the engine's FPS HUD is the only other
//! on-screen information.

pub mod keys;

pub use keys::Keys;
