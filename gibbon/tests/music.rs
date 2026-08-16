//! The game's background theme must always render to valid audio.

use engine::midi::SAMPLE_RATE;

#[test]
fn music_renders_to_wav() {
    let midi = gibbon::assets::load("music.mid").expect("music.mid is embedded");
    let wav = engine::midi::render_wav(midi).expect("music.mid renders");
    assert_eq!(&wav[..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    assert_eq!(&wav[22..24], &1u16.to_le_bytes());
    assert_eq!(&wav[24..28], &SAMPLE_RATE.to_le_bytes());
    let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
    assert_eq!(data_len, wav.len() - 44);

    // The theme is roughly half a minute of audio.
    let seconds = data_len as f64 / 2.0 / SAMPLE_RATE as f64;
    assert!(
        (30.0..36.0).contains(&seconds),
        "expected ~32s of music, got {seconds:.1}s"
    );

    // And it is audible: the renderer normalizes to a 0.5 peak.
    let peak = wav[44..]
        .chunks_exact(2)
        .map(|c| (i16::from_le_bytes([c[0], c[1]]) as f32).abs() / 32767.0)
        .fold(0.0f32, f32::max);
    assert!((peak - 0.5).abs() < 0.05, "peak level was {peak}");
}
