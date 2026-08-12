//! Device-aware gamepad input for Android.
//!
//! miniquad 0.4.11 has no gamepad/device API: every key arrives through
//! `EventHandler` with no way to tell which gamepad it came from. To support a
//! second player we forward gamepad keys with their player slot straight into a
//! small thread-safe queue, which `Stage::update` drains each frame.
//!
//! On Android the Java glue assigns each connected gamepad a player slot
//! (first-seen = player 0, second = player 1) and calls the native
//! `surfaceOnPlayerKey` method declared in `QuadNative.java`. That symbol is
//! implemented here (see `Java_quad_1native_QuadNative_surfaceOnPlayerKey`).
//! On desktop the queue is never fed and drains as a no-op.

use std::sync::{Mutex, OnceLock};

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
