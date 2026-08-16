//! Looping background-music playback on top of `quad-snd`.
//!
//! `quad-snd` is the miniquad author's cross-platform audio backend: ALSA on
//! Linux and OpenSL ES on Android. It runs its own audio thread with a small
//! mixer; we hand it pre-decoded PCM (a WAV in memory) and let it loop it.
//!
//! The audio context is created once, lazily, the first time music is played,
//! and then shared by every caller, so a game that is quit and re-selected
//! never stacks a second audio thread on top of the first. There is only one
//! looping music slot: starting a new loop replaces the previous one.

use quad_snd::{AudioContext, PlaySoundParams, Sound};

use std::sync::{Mutex, OnceLock};

/// Playback volume applied to the looping music.
const MUSIC_VOLUME: f32 = 0.5;

struct AudioState {
    /// The shared audio backend; created on first use.
    ctx: AudioContext,
    /// The currently loaded looping music, if any.
    music: Option<Sound>,
}

/// The process-wide audio state, initialized the first time music is played.
fn state() -> &'static Mutex<Option<AudioState>> {
    static STATE: OnceLock<Mutex<Option<AudioState>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

/// Play `wav` (RIFF/WAV bytes, the output of [`crate::midi::render_wav`] or
/// any other WAV) as a loop, replacing whatever was looping before.
pub fn play_loop(wav: &[u8]) {
    let state = state();
    let mut guard = state.lock().expect("audio state poisoned");
    // Create the backend on first use. `AudioContext::new` just starts the
    // audio thread and returns; if the platform has no audio device the
    // thread fails there, not here.
    if guard.is_none() {
        *guard = Some(AudioState {
            ctx: AudioContext::new(),
            music: None,
        });
    }
    let state = guard.as_mut().expect("audio state just initialized");
    if let Some(old) = state.music.take() {
        old.stop(&state.ctx);
        old.delete(&state.ctx);
    }
    let music = Sound::load(&state.ctx, wav);
    music.play(
        &state.ctx,
        PlaySoundParams {
            looped: true,
            volume: MUSIC_VOLUME,
        },
    );
    state.music = Some(music);
}

/// Stop the looping music and free the loaded sound.
pub fn stop() {
    let mut guard = state().lock().expect("audio state poisoned");
    if let Some(state) = guard.as_mut()
        && let Some(music) = state.music.take()
    {
        music.stop(&state.ctx);
        music.delete(&state.ctx);
    }
}
