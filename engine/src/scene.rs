//! The scene abstraction: a game-agnostic way to split an app into screens or
//! modes (a player menu, the gameplay itself, a score screen, a game-over
//! screen, ...). The engine [`crate::app::Stage`] owns exactly one
//! active [`Scene`] at a time and forwards input, update, draw and lifecycle
//! calls to it; it never knows what a scene draws or what its input means.
//!
//! A game builds its scenes and swaps them by returning
//! [`SceneAction`] from `input` or `update` — e.g. the game selection returns
//! a player menu on confirm, and the menu returns a `Playing` scene on its own
//! confirm. The engine keeps a stack of scenes: [`SceneAction::Push`] drops a
//! new scene on top of the current one (which stays below), and
//! [`SceneAction::Pop`]/[`SceneAction::PopToRoot`] return to a previous scene,
//! so a gameplay scene can hand control back to the game-selection screen.
//! Because the engine only sees the trait, the same engine shell can be reused
//! for an entirely different game.

use crate::input::{Input, InputState};
use crate::render::Framebuffer;

/// What a scene asks the engine to do next.
pub enum SceneAction {
    /// Stay in the current scene.
    Continue,
    /// Push a new scene on top of the current one. The current scene is kept
    /// below on the stack, so [`SceneAction::Pop`] / [`SceneAction::PopToRoot`]
    /// can return to it.
    Push(Box<dyn Scene>),
    /// Replace the current scene with a new one (the current scene is dropped).
    Switch(Box<dyn Scene>),
    /// Return to the previous scene (pop one level). Quits the app if the
    /// stack is empty.
    Pop,
    /// Return to the root scene (the first one ever pushed, i.e. the app's
    /// home screen), dropping everything above it. Quits the app if the stack
    /// is empty.
    PopToRoot,
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
