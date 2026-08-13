//! The scene abstraction: a game-agnostic way to split an app into screens or
//! modes (a player menu, the gameplay itself, a score screen, a game-over
//! screen, ...). The engine [`crate::app::Stage`] owns exactly one
//! active [`Scene`] at a time and forwards input, update, draw and lifecycle
//! calls to it; it never knows what a scene draws or what its input means.
//!
//! A game builds its scenes and swaps them by returning
//! [`SceneAction::Switch`] from `input` or `update` — e.g. the menu returns a
//! `Playing` scene on confirm, and gameplay will later return a game-over
//! scene. Because the engine only sees the trait, the same engine shell can be
//! reused for an entirely different game.

use crate::input::{Input, InputState};
use crate::render::Framebuffer;

/// What a scene asks the engine to do next.
pub enum SceneAction {
    /// Stay in the current scene.
    Continue,
    /// Replace the current scene with a new one.
    Switch(Box<dyn Scene>),
    /// Request the application to quit.
    Quit,
}

/// One screen/mode of a game.
pub trait Scene {
    /// Handle one input edge (`down == true` press, `false` release).
    /// Auto-repeat of an already-held key is filtered by the engine before
    /// this is called, and the per-player held state is always visible through
    /// `input` during [`Scene::update`].
    fn input(&mut self, _player: usize, _input: Input, _down: bool) -> SceneAction {
        SceneAction::Continue
    }

    /// Advance the scene by `dt` seconds. `input` exposes the current
    /// per-player held inputs for sampling.
    fn update(&mut self, _dt: f64, _input: &InputState) -> SceneAction {
        SceneAction::Continue
    }

    /// Draw the scene into the framebuffer. The engine clears the framebuffer
    /// and presents it afterwards, so a scene only paints its own pixels.
    fn draw(&mut self, fb: &mut Framebuffer);

    /// The window was minimized / the app was backgrounded: pause anything
    /// that should not advance while hidden.
    fn suspend(&mut self) {}

    /// The window was restored / the app was foregrounded.
    fn resume(&mut self) {}
}
