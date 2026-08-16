//! A tiny Standard MIDI File (`.mid`) player: parses format 0/1 files and
//! renders them to a mono 16-bit PCM WAV in memory with a small software
//! synthesizer. The WAV can then be handed to [`crate::audio::play_loop`].
//!
//! The synthesizer is intentionally simple: it plays every note with the same
//! plucky, piano-like tone (an exponentially decaying sine with a fast-decaying
//! sparkle from the upper harmonics), ignores instrument changes, sustain and
//! pitch bend, and supports a handful of tempo changes. That is enough to make
//! a single-track game theme loop nicely while staying a few hundred lines of
//! dependency-free code. SMPTE-division files are not supported.

/// Sample rate of the rendered audio.
pub const SAMPLE_RATE: u32 = 44_100;

/// Attack time of a note, in seconds: a quick linear ramp to full level.
const ATTACK: f64 = 0.004;
/// Exponential decay of the fundamental, in seconds.
const DECAY: f64 = 1.1;
/// Time it takes a note to fade out after its note-off, in seconds.
const RELEASE: f64 = 0.06;
/// Decay of the second harmonic, in seconds: it gives the tone its initial
/// sparkle and dies away quickly, like a struck string.
const SECOND_DECAY: f64 = 0.35;
/// Decay of the third harmonic, in seconds.
const THIRD_DECAY: f64 = 0.12;
/// Polyphony limit; the file's densest chord is well under this.
const MAX_VOICES: usize = 16;
/// A little silence after the last note so the loop breathes.
const TAIL: f64 = 0.5;

/// Default tempo (500_000 µs/beat = 120 BPM) when the file sets none.
const DEFAULT_TEMPO: u32 = 500_000;

/// One rendered note, with absolute times in seconds.
#[derive(Clone, Copy)]
struct Note {
    start: f64,
    end: f64,
    note: u8,
    vel: f32,
}

/// A note event recovered from the file, tagged with its absolute tick and
/// channel (a note-on in one track pairs with its note-off in the same track).
#[derive(Clone, Copy)]
struct TrackEvent {
    tick: u64,
    channel: u8,
    note: u8,
    vel: u8,
    on: bool,
}

/// The result of parsing the file: merged note events, tempo changes and the
/// division (ticks per quarter note).
struct Parsed {
    events: Vec<TrackEvent>,
    tempos: Vec<(u64, u32)>,
    division: u32,
}

/// Render `data` (the bytes of a format 0/1 `.mid` file) to a mono 16-bit PCM
/// WAV. Returns `None` for malformed input, SMPTE division, or a file with no
/// notes.
pub fn render_wav(data: &[u8]) -> Option<Vec<u8>> {
    let parsed = parse(data)?;
    if parsed.division >> 15 != 0 {
        // SMPTE timecode division; not supported.
        return None;
    }

    // Pair note-ons with their note-offs into absolute-time notes.
    let notes = pair_notes(&parsed.events, &parsed.tempos, parsed.division);
    if notes.is_empty() {
        return None;
    }

    let samples = synthesize(&notes);
    Some(wav_pcm16(&samples, SAMPLE_RATE))
}

/// Parse the SMF header and every track.
fn parse(data: &[u8]) -> Option<Parsed> {
    let mut pos = 14usize;
    let mut header = [0u8; 14];
    header.copy_from_slice(data.get(..14)?);
    if &header[..4] != b"MThd" {
        return None;
    }
    let division = u16::from_be_bytes([header[12], header[13]]) as u32;
    let ntrks = u16::from_be_bytes([header[10], header[11]]);

    let mut events = Vec::new();
    let mut tempos = Vec::new();

    for _ in 0..ntrks {
        let tag = data.get(pos..pos + 4)?;
        if tag != b"MTrk" {
            return None;
        }
        let len = u32::from_be_bytes(data.get(pos + 4..pos + 8)?.try_into().ok()?);
        let start = pos + 8;
        let end = start + len as usize;
        if end > data.len() {
            return None;
        }
        parse_track(data, start, end, &mut events, &mut tempos)?;
        pos = end;
    }

    tempos.sort_by_key(|(tick, _)| *tick);
    Some(Parsed {
        events,
        tempos,
        division,
    })
}

/// Walk one track chunk, collecting note and tempo events with absolute ticks.
fn parse_track(
    data: &[u8],
    start: usize,
    end: usize,
    events: &mut Vec<TrackEvent>,
    tempos: &mut Vec<(u64, u32)>,
) -> Option<()> {
    let mut pos = start;
    let mut tick: u64 = 0;
    let mut running: u8 = 0;

    while pos < end {
        let delta = read_vlq(data, &mut pos)?;
        tick += delta as u64;

        let b0 = *data.get(pos)?;
        pos += 1;

        // First byte of the event: a status byte or, when running status is
        // active, the first data byte.
        let (status, first);
        if b0 >= 0x80 {
            match b0 {
                0xFF => {
                    // Meta event.
                    let meta = *data.get(pos)?;
                    pos += 1;
                    let len = read_vlq(data, &mut pos)? as usize;
                    if meta == 0x51 && len == 3 {
                        let bytes = data.get(pos..pos + 3)?;
                        let tempo =
                            ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | bytes[2] as u32;
                        tempos.push((tick, tempo));
                    }
                    pos += len;
                    if meta == 0x2F {
                        // End of track: stop here even if the chunk holds a
                        // few trailing bytes.
                        return Some(());
                    }
                    continue;
                }
                0xF0 | 0xF7 => {
                    // Sysex: skip.
                    let len = read_vlq(data, &mut pos)? as usize;
                    pos += len;
                    continue;
                }
                _ => {
                    status = b0;
                    running = b0;
                    first = *data.get(pos)?;
                    pos += 1;
                }
            }
        } else {
            if running == 0 {
                return None;
            }
            status = running;
            first = b0;
        }

        match status & 0xF0 {
            0x80 => {
                let _ = *data.get(pos)?;
                pos += 1;
                events.push(TrackEvent {
                    tick,
                    channel: status & 0x0F,
                    note: first,
                    vel: 0,
                    on: false,
                });
            }
            0x90 => {
                let vel = *data.get(pos)?;
                pos += 1;
                events.push(TrackEvent {
                    tick,
                    channel: status & 0x0F,
                    note: first,
                    vel,
                    on: vel != 0,
                });
            }
            // Polyphonic aftertouch / control change / pitch bend: two data
            // bytes, ignored here.
            0xA0 | 0xB0 | 0xE0 => {
                let _ = *data.get(pos)?;
                pos += 1;
            }
            // Program change / channel pressure: one data byte, ignored.
            0xC0 | 0xD0 => {}
            _ => return None,
        }
    }
    Some(())
}

/// Read a MIDI variable-length quantity at `pos`, advancing it.
fn read_vlq(data: &[u8], pos: &mut usize) -> Option<u32> {
    let mut value: u32 = 0;
    for _ in 0..4 {
        let byte = *data.get(*pos)?;
        *pos += 1;
        value = (value << 7) | (byte & 0x7F) as u32;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// Convert an absolute tick to seconds, honoring the tempo changes.
fn tick_to_seconds(tick: u64, division: u32, tempos: &[(u64, u32)]) -> f64 {
    let mut prev_tick: u64 = 0;
    let mut seconds: f64 = 0.0;
    let mut tempo = DEFAULT_TEMPO;
    for (t, us) in tempos {
        if tick <= *t {
            seconds += (tick - prev_tick) as f64 / division as f64 * tempo as f64 / 1e6;
            return seconds;
        }
        seconds += (t - prev_tick) as f64 / division as f64 * tempo as f64 / 1e6;
        prev_tick = *t;
        tempo = *us;
    }
    seconds + (tick - prev_tick) as f64 / division as f64 * tempo as f64 / 1e6
}

/// Pair note-ons with note-offs into absolute-time notes. A note-on still held
/// at the end of the file is released at the last event's tick.
fn pair_notes(events: &[TrackEvent], tempos: &[(u64, u32)], division: u32) -> Vec<Note> {
    let mut sorted = events.to_vec();
    sorted.sort_by_key(|e| e.tick);
    let last_tick = sorted.last().map(|e| e.tick).unwrap_or(0);

    // (channel, note) -> (start tick, velocity) of the held note.
    let mut held: Vec<(u8, u8, u64, u8)> = Vec::new();
    let mut notes: Vec<Note> = Vec::new();

    for event in &sorted {
        let key = (event.channel, event.note);
        if event.on {
            held.push((event.channel, event.note, event.tick, event.vel));
        } else if let Some(i) = held.iter().position(|&(c, n, _, _)| (c, n) == key) {
            let (_, _, start_tick, vel) = held.remove(i);
            notes.push(Note {
                start: tick_to_seconds(start_tick, division, tempos),
                end: tick_to_seconds(event.tick, division, tempos),
                note: event.note,
                vel: vel as f32 / 127.0,
            });
        }
    }

    // Release anything still held when the file ends.
    for (_, note, start_tick, vel) in held {
        notes.push(Note {
            start: tick_to_seconds(start_tick, division, tempos),
            end: tick_to_seconds(last_tick, division, tempos),
            note,
            vel: vel as f32 / 127.0,
        });
    }

    notes
}

/// A currently sounding note.
struct Voice {
    /// Phase of the fundamental, advanced once per sample.
    phase: f64,
    freq: f64,
    start: f64,
    end: f64,
    /// Note-off time, or `None` while the note is still held.
    released: Option<f64>,
    gain: f32,
}

impl Voice {
    fn new(note: &Note) -> Voice {
        let midi = note.note as f64;
        Voice {
            phase: 0.0,
            freq: 440.0 * 2.0f64.powf((midi - 69.0) / 12.0),
            start: note.start,
            end: note.end,
            released: None,
            gain: note.vel.powf(1.5),
        }
    }

    /// Mix one sample, advancing the oscillator phase. Returns `None` once the
    /// release fade has finished.
    fn sample(&mut self, t: f64) -> Option<f64> {
        if let Some(released) = self.released {
            let fade = 1.0 - (t - released) / RELEASE;
            if fade <= 0.0 {
                return None;
            }
        }
        let age = t - self.start;
        if age < 0.0 {
            return Some(0.0);
        }
        self.phase += 2.0 * std::f64::consts::PI * self.freq / SAMPLE_RATE as f64;

        // Envelope: fast attack, exponential decay.
        let env = if age < ATTACK {
            age / ATTACK
        } else {
            (-(age - ATTACK) / DECAY).exp()
        };
        // Release fade applied on top.
        let env = if let Some(released) = self.released {
            env * (1.0 - (t - released) / RELEASE).clamp(0.0, 1.0)
        } else {
            env
        };

        let sparkle = 0.5 * (2.0 * self.phase).sin() * (-age / SECOND_DECAY).exp()
            + 0.15 * (3.0 * self.phase).sin() * (-age / THIRD_DECAY).exp();
        Some((self.phase.sin() + sparkle) * env * self.gain as f64)
    }
}

/// Render the notes to mono samples at [`SAMPLE_RATE`], normalized so the
/// peak level is 0.5.
fn synthesize(notes: &[Note]) -> Vec<f32> {
    let mut sorted = notes.to_vec();
    sorted.sort_by(|a, b| a.start.total_cmp(&b.start));

    let last_end = sorted.iter().fold(0.0f64, |max, n| max.max(n.end));
    let total = ((last_end + TAIL) * SAMPLE_RATE as f64).ceil() as usize;
    let mut out = vec![0.0f32; total];
    let mut voices: Vec<Voice> = Vec::new();
    let mut next = 0usize;
    let mut peak = 0.0f32;

    for (i, sample) in out.iter_mut().enumerate() {
        let t = i as f64 / SAMPLE_RATE as f64;

        // Start every note whose time has come.
        while next < sorted.len() && sorted[next].start <= t {
            voices.push(Voice::new(&sorted[next]));
            next += 1;
            if voices.len() > MAX_VOICES {
                voices.remove(0);
            }
        }
        // Note-offs release the voice; it fades out and is then dropped.
        for voice in voices.iter_mut() {
            if voice.released.is_none() && voice.end <= t {
                voice.released = Some(t);
            }
        }

        let mut sum = 0.0f64;
        voices.retain_mut(|voice| match voice.sample(t) {
            Some(value) => {
                sum += value;
                true
            }
            None => false,
        });

        *sample = (sum * 0.5) as f32;
        peak = peak.max(sample.abs());
    }

    if peak > 0.0 {
        let scale = 0.5 / peak;
        for sample in out.iter_mut() {
            *sample *= scale;
        }
    }
    out
}

/// Encode mono samples as a 16-bit PCM WAV in memory.
fn wav_pcm16(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = samples.len() as u32 * 2;
    let byte_rate = sample_rate * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a value as a MIDI variable-length quantity.
    fn push_vlq(track: &mut Vec<u8>, mut value: u32) {
        let mut groups = [0u8; 4];
        let mut n = 0;
        groups[n] = (value & 0x7F) as u8;
        value >>= 7;
        while value > 0 {
            n += 1;
            groups[n] = (value & 0x7F) as u8;
            value >>= 7;
        }
        for i in (0..=n).rev() {
            track.push(if i == 0 { groups[i] } else { groups[i] | 0x80 });
        }
    }

    /// Build a minimal format-0 file from `(delta, event bytes)` pairs.
    fn small_midi(events: &[(u32, &[u8])]) -> Vec<u8> {
        let mut track = Vec::new();
        for (delta, bytes) in events {
            push_vlq(&mut track, *delta);
            track.extend_from_slice(bytes);
        }
        track.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);

        let mut out = Vec::new();
        out.extend_from_slice(b"MThd");
        out.extend_from_slice(&6u32.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&480u16.to_be_bytes());
        out.extend_from_slice(b"MTrk");
        out.extend_from_slice(&(track.len() as u32).to_be_bytes());
        out.extend_from_slice(&track);
        out
    }

    #[test]
    fn vlq() {
        let mut pos = 0;
        assert_eq!(read_vlq(&[0x00], &mut pos), Some(0));
        assert_eq!(pos, 1);
        pos = 0;
        assert_eq!(read_vlq(&[0x81, 0x00], &mut pos), Some(128));
        assert_eq!(pos, 2);
        pos = 0;
        assert_eq!(read_vlq(&[0xFF, 0x7F], &mut pos), Some(16383));
        assert_eq!(pos, 2);
        // 480 = (0x03 << 7) | 0x60.
        pos = 0;
        assert_eq!(read_vlq(&[0x83, 0x60], &mut pos), Some(480));
        assert_eq!(pos, 2);
    }

    #[test]
    fn header_and_wav() {
        // A4 (69) held one quarter note (480 ticks at 480 TPQN). Default tempo
        // is 120 BPM, so the note lasts 0.5 s.
        let midi = small_midi(&[(0, &[0x90, 69, 100]), (480, &[0x80, 69, 0])]);
        let wav = render_wav(&midi).expect("renders");
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[22..24], &1u16.to_le_bytes());
        assert_eq!(&wav[24..28], &SAMPLE_RATE.to_le_bytes());
        assert_eq!(&wav[34..36], &16u16.to_le_bytes());
        assert_eq!(&wav[36..40], b"data");
        let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
        assert_eq!(data_len, wav.len() - 44);
        assert_eq!(data_len, ((0.5 + TAIL) * SAMPLE_RATE as f64) as usize * 2);
        // A note is audible: the normalized peak is 0.5.
        let peak = wav[44..]
            .chunks_exact(2)
            .map(|c| (i16::from_le_bytes([c[0], c[1]]) as f32).abs() / 32767.0)
            .fold(0.0f32, f32::max);
        assert!((peak - 0.5).abs() < 0.01);
    }

    #[test]
    fn tempo_event() {
        // Set 400_000 us/beat (150 BPM) via a meta event, then hold a note for
        // one quarter note: 0.4 s.
        let midi = small_midi(&[
            (0, &[0xFF, 0x51, 0x03, 0x06, 0x1A, 0x80]),
            (0, &[0x90, 60, 90]),
            (480, &[0x80, 60, 0]),
        ]);
        let wav = render_wav(&midi).expect("renders");
        let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
        assert_eq!(data_len, ((0.4 + TAIL) * SAMPLE_RATE as f64) as usize * 2);
    }

    #[test]
    fn running_status() {
        // A note-off using running status (the second event has no status
        // byte) must still be parsed.
        let midi = small_midi(&[(0, &[0x90, 60, 90]), (480, &[60, 0])]);
        let wav = render_wav(&midi).expect("renders running status");
        let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
        assert_eq!(data_len, ((0.5 + TAIL) * SAMPLE_RATE as f64) as usize * 2);
    }

    #[test]
    fn empty_is_none() {
        assert!(render_wav(b"not a midi file").is_none());
        // A file with a tempo event but no notes.
        let midi = small_midi(&[(0, &[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20])]);
        assert!(render_wav(&midi).is_none());
    }
}
