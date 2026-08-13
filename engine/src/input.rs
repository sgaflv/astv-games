//! Logical game inputs, key mapping, per-player held state, and the
//! device-aware Android gamepad queue.
//!
//! The engine exposes a small set of *logical* inputs (directions, confirm,
//! back, pause and the four gamepad face buttons) so games built on the engine
//! never have to know about physical keys. Key events arrive two ways:
//!
//! * Desktop / TV remote: miniquad `EventHandler` key events are mapped with
//!   [`Input::from_keycode`].
//! * Android gamepads: device-aware events (player slot + raw Android keycode)
//!   are forwarded through a thread-safe queue (see the queue docs below).
//!
//! [`InputState`] keeps the per-player held flags plus the physical key that
//! holds each slot, so the engine can filter OS/Android auto-repeat without
//! swallowing a *different* key that maps to the same logical input.

use miniquad::KeyCode;

use std::sync::{Mutex, OnceLock};

/// Maximum number of players the engine routes input for.
pub const PLAYERS: usize = 2;

/// Number of distinct logical inputs ([`Input`] variants).
pub const INPUT_COUNT: usize = 11;

/// A logical game input, decoupled from any physical key or gamepad button.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Pause,
    // Gamepad face buttons. On Android, gamepads bypass miniquad's key path
    // entirely (device-aware `surfaceOnPlayerKey`, see below); for other
    // devices the Java glue remaps KEYCODE_BUTTON_* to F1-F4 because miniquad
    // 0.4.11 cannot tell them apart. F1-F8 on a desktop keyboard trigger the
    // same inputs for easy testing.
    GameA,
    GameB,
    GameX,
    GameY,
}

const fn input_index(input: Input) -> usize {
    match input {
        Input::Up => 0,
        Input::Down => 1,
        Input::Left => 2,
        Input::Right => 3,
        Input::Confirm => 4,
        Input::Back => 5,
        Input::Pause => 6,
        Input::GameA => 7,
        Input::GameB => 8,
        Input::GameX => 9,
        Input::GameY => 10,
    }
}

impl Input {
    /// Map a desktop key to a (player, input) pair. Player 1 uses arrows/WASD
    /// and F1-F4, player 2 uses IJKL and F5-F8. Global actions (pause/back)
    /// use player 0; their player index is irrelevant.
    pub(crate) fn from_keycode(key: KeyCode) -> Option<(usize, Input)> {
        use KeyCode::*;
        let input = match key {
            Up | W => Input::Up,
            Down | S => Input::Down,
            Left | A => Input::Left,
            Right | D => Input::Right,
            I => Input::Up,
            K => Input::Down,
            J => Input::Left,
            L => Input::Right,
            Enter => Input::Confirm,
            Escape | Back => Input::Back,
            Space | Menu => Input::Pause,
            F1 => Input::GameA,
            F2 => Input::GameB,
            F3 => Input::GameX,
            F4 => Input::GameY,
            F5 => Input::GameA,
            F6 => Input::GameB,
            F7 => Input::GameX,
            F8 => Input::GameY,
            _ => return None,
        };
        let player = match key {
            I | K | J | L | F5 | F6 | F7 | F8 => 1,
            _ => 0,
        };
        Some((player, input))
    }
}

/// Raw Android keycode -> game input. Used by the device-aware gamepad path
/// (`surfaceOnPlayerKey`), which bypasses miniquad's keycode translation.
/// Values are the android.view.KeyEvent constants.
#[cfg(target_os = "android")]
pub(crate) fn android_keycode_to_input(keycode: i32) -> Option<Input> {
    match keycode {
        19 => Some(Input::Up),      // KEYCODE_DPAD_UP
        20 => Some(Input::Down),    // KEYCODE_DPAD_DOWN
        21 => Some(Input::Left),    // KEYCODE_DPAD_LEFT
        22 => Some(Input::Right),   // KEYCODE_DPAD_RIGHT
        66 => Some(Input::Confirm), // KEYCODE_ENTER
        23 => Some(Input::Confirm), // KEYCODE_DPAD_CENTER (OK; gamepad A often sends this)
        4 => Some(Input::Back),     // KEYCODE_BACK
        111 => Some(Input::Back),   // KEYCODE_ESCAPE
        82 => Some(Input::Pause),   // KEYCODE_MENU
        62 => Some(Input::Pause),   // KEYCODE_SPACE
        96 => Some(Input::GameA),   // KEYCODE_BUTTON_A
        97 => Some(Input::GameB),   // KEYCODE_BUTTON_B
        99 => Some(Input::GameX),   // KEYCODE_BUTTON_X
        100 => Some(Input::GameY),  // KEYCODE_BUTTON_Y
        _ => None,
    }
}

/// Per-player held-input state, maintained by the engine's edge detection and
/// sampled by scenes during [`crate::scene::Scene::update`] (e.g. to
/// read a held face button).
#[derive(Clone)]
pub struct InputState {
    /// Logical inputs currently held, per player.
    held: [[bool; INPUT_COUNT]; PLAYERS],
    /// The physical key (a platform keycode) currently holding each logical
    /// input, per player. Used to ignore OS/Android auto-repeat (the same key
    /// re-sent while held) without swallowing a *different* key that maps to
    /// the same input (e.g. arrow-Up while W is held, or a second controller
    /// sharing the slot).
    held_keys: [[Option<u32>; INPUT_COUNT]; PLAYERS],
}

impl Default for InputState {
    fn default() -> InputState {
        InputState::new()
    }
}

impl InputState {
    pub fn new() -> InputState {
        InputState {
            held: [[false; INPUT_COUNT]; PLAYERS],
            held_keys: [[None; INPUT_COUNT]; PLAYERS],
        }
    }

    /// Whether `input` is currently held down for `player`.
    pub fn held(&self, player: usize, input: Input) -> bool {
        self.held[player][input_index(input)]
    }

    /// Feed one input edge from the physical `key` (a platform keycode used
    /// only to tell distinct keys apart). Returns `true` when it is a *new*
    /// edge (a fresh press, or the release of the key holding the slot) and
    /// `false` for auto-repeat of the same held key, which the caller should
    /// ignore. A different key that maps to the same logical input still
    /// counts as a new press, so one player's held key never blocks another
    /// player's (or another key's) press.
    pub fn key_edge(&mut self, player: usize, key: u32, input: Input, down: bool) -> bool {
        let idx = input_index(input);
        if down {
            if self.held_keys[player][idx] == Some(key) {
                return false; // OS/Android auto-repeat of an already-held key
            }
            self.held_keys[player][idx] = Some(key);
            self.held[player][idx] = true;
            true
        } else if self.held_keys[player][idx] == Some(key) {
            self.held_keys[player][idx] = None;
            self.held[player][idx] = false;
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Device-aware gamepad input for Android.
//
// miniquad 0.4.11 has no gamepad/device API: every key arrives through
// `EventHandler` with no way to tell which gamepad it came from. To support a
// second player we forward gamepad keys with their player slot straight into a
// small thread-safe queue, which `Stage::update` drains each frame.
//
// On Android the Java glue assigns each connected gamepad a player slot
// (first-seen = player 0, second = player 1) and calls the native
// `surfaceOnPlayerKey` method declared in `QuadNative.java`. That symbol is
// implemented here (see `Java_quad_1native_QuadNative_surfaceOnPlayerKey`).
// On desktop the queue is never fed and drains as a no-op.
// ---------------------------------------------------------------------------

/// One per-player key event coming from an Android gamepad.
#[derive(Clone, Copy, Debug)]
pub struct PlayerKeyEvent {
    /// Player slot (0 = snake 1, 1 = snake 2).
    pub player: usize,
    /// Raw Android keycode (KEYCODE_DPAD_* / KEYCODE_BUTTON_*).
    pub keycode: i32,
    /// True when the key went down, false on key up.
    pub down: bool,
}

static QUEUE: OnceLock<Mutex<Vec<PlayerKeyEvent>>> = OnceLock::new();

/// Set up the input queue. Android only (called from `quad_main`).
#[cfg(target_os = "android")]
pub fn init() {
    let _ = QUEUE.set(Mutex::new(Vec::new()));
}

/// Push a key event from a JNI callback (Android UI thread).
#[cfg(target_os = "android")]
pub fn push(event: PlayerKeyEvent) {
    if let Some(queue) = QUEUE.get() {
        queue.lock().unwrap().push(event);
    }
}

/// Drain all pending events into `cb`. Called once per frame from `update`
/// (no-op on desktop, where the queue is never set up).
pub fn drain_into(mut cb: impl FnMut(PlayerKeyEvent)) {
    if let Some(queue) = QUEUE.get() {
        let mut queue = queue.lock().unwrap();
        for event in queue.drain(..) {
            cb(event);
        }
    }
}

// JNI types are declared here so this module has no dependency on miniquad's
// (private) Android backend. The signatures only use C-ABI compatible types.
// The enums are opaque pointer targets; they are never dereferenced.
#[cfg(target_os = "android")]
mod jni {
    pub enum JNIEnv {}

    #[allow(non_camel_case_types)]
    pub enum _jobject {}

    #[allow(non_camel_case_types)]
    pub type jobject = *mut _jobject;
    #[allow(non_camel_case_types)]
    pub type jint = i32;
}

/// Native implementation of `quad_native.QuadNative.surfaceOnPlayerKey`,
/// declared in `android/java/quad_native/QuadNative.java`.
///
/// Runs on the Android UI thread (from `MainActivity.onKey`). The symbol name
/// is the JNI-mangled form of `quad_native.QuadNative.surfaceOnPlayerKey`.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Java_quad_1native_QuadNative_surfaceOnPlayerKey(
    _env: *mut jni::JNIEnv,
    _this: jni::jobject,
    player: jni::jint,
    keycode: jni::jint,
    down: jni::jint,
) {
    push(PlayerKeyEvent {
        player: player.max(0) as usize,
        keycode,
        down: down != 0,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Player 0 (arrows/WASD/F1-F4) and player 1 (IJKL/F5-F8) must never be
    /// routed to the same slot: this is what keeps two keyboard players'
    /// controls independent on the desktop.
    #[test]
    fn player_key_sets_are_routed_to_separate_players() {
        use KeyCode::*;
        let player_0 = [Up, Down, Left, Right, W, A, S, D, F1, F2, F3, F4];
        let player_1 = [I, K, J, L, F5, F6, F7, F8];
        for key in player_0 {
            assert_eq!(
                Input::from_keycode(key).unwrap().0,
                0,
                "keycode {} -> player 0",
                key as u32
            );
        }
        for key in player_1 {
            assert_eq!(
                Input::from_keycode(key).unwrap().0,
                1,
                "keycode {} -> player 1",
                key as u32
            );
        }
    }

    /// Distinct physical keys may map to the same logical input (arrow-Up and
    /// W both steer up). They carry distinct keycodes, so the auto-repeat
    /// suppression in `key_edge` must treat them as separate keys: holding one
    /// never swallows a press of the other.
    #[test]
    fn distinct_keys_sharing_an_input_stay_distinct() {
        use KeyCode::*;
        let (p0, up_arrow) = Input::from_keycode(Up).unwrap();
        let (w0, w) = Input::from_keycode(W).unwrap();
        assert_eq!(p0, w0);
        assert!(matches!(up_arrow, Input::Up));
        assert!(matches!(w, Input::Up));
        assert_ne!(Up as u32, W as u32);
    }

    #[test]
    fn auto_repeat_is_filtered_per_physical_key() {
        let mut state = InputState::new();
        // Press arrow-Up (player 0).
        assert!(state.key_edge(0, 100, Input::Up, true));
        assert!(state.held(0, Input::Up));
        // Auto-repeat of the same key is ignored.
        assert!(!state.key_edge(0, 100, Input::Up, true));
        // A different key (W) mapping to the same input is still a new press.
        assert!(state.key_edge(0, 101, Input::Up, true));
        // Releasing the old key (arrow-Up) is ignored; W still holds the slot.
        assert!(!state.key_edge(0, 100, Input::Up, false));
        assert!(state.held(0, Input::Up));
        // Releasing W clears the input.
        assert!(state.key_edge(0, 101, Input::Up, false));
        assert!(!state.held(0, Input::Up));
    }

    #[test]
    fn players_hold_state_independently() {
        let mut state = InputState::new();
        state.key_edge(0, 1, Input::GameA, true);
        assert!(state.held(0, Input::GameA));
        assert!(!state.held(1, Input::GameA));
        state.key_edge(0, 1, Input::GameA, false);
        assert!(!state.held(0, Input::GameA));
    }
}
