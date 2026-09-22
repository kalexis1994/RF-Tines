//! Renders every preset on low notes, through the plugin, for analysis.
//!
//! A listener reported `portable-chime-1980` and `wide-dynamics-1984` sounding
//! "broken" in the bass. An earlier probe, `rf-tines-lab`'s `bass_bell_check`,
//! blamed the bar partial, but it built profiles by hand from four controls
//! and so was not testing the presets at all: the period instruments also
//! differ in hammer mass, tine mass, pole radius and axis twist. That
//! conclusion is withdrawn -- through this path there is no inharmonic
//! partial within 40 dB of the peak on a bass note.
//!
//! What ran against these renders, and found nothing that singles the two
//! reported presets out: clipping, slow amplitude beating, a second pitch
//! near the fundamental, harmonic balance against the reference recording,
//! attack-tick level, and spectral jaggedness. Kept so the next attempt does
//! not have to re-derive the renders, and does not repeat the six.
//!
//! `cargo run --release -p rf-tines-plugin --example bass_presets -- OUT_DIR`
use rackforge_plugin_sdk::{MidiEvent, Processor};
use rf_tines_plugin::RfTinesProcessor;

const RATE: f64 = 48_000.0;
const FRAMES: usize = 256;
const SECONDS: f64 = 3.0;

const PRESETS: [&str; 8] = [
    "portable-bark-1972",
    "tine-bass-1960",
    "felt-1966",
    "console-bark-1973",
    "portable-bell-1977",
    "console-bell-1978",
    "portable-chime-1980",
    "wide-dynamics-1984",
];

fn render(preset: &str, note: u8, velocity: u8) -> Vec<f64> {
    let mut plugin = RfTinesProcessor::default();
    assert!(plugin.load_preset(preset), "{preset} does not load");
    assert!(plugin.prepare(RATE, FRAMES as u32, 0, 2));
    let on = [MidiEvent {
        frame: 0,
        data: [0x90, note, velocity],
        length: 3,
    }];
    let blocks = (RATE * SECONDS / FRAMES as f64) as usize;
    let mut out = Vec::with_capacity(blocks * FRAMES);
    for block in 0..blocks {
        let mut audio = [0.0_f32; FRAMES * 2];
        let events: &[MidiEvent] = if block == 0 { &on } else { &[] };
        plugin.process(&[], &mut audio, events, &[], FRAMES as u32, 0, 2);
        out.extend(audio.as_chunks::<2>().0.iter().map(|f| f64::from(f[0])));
    }
    out
}

fn write_wav(path: &std::path::Path, samples: &[f64]) -> std::io::Result<()> {
    use std::io::Write;
    let bytes = (samples.len() * 4) as u32;
    let mut f = std::fs::File::create(path)?;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + bytes).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&3u16.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&(RATE as u32).to_le_bytes())?;
    f.write_all(&((RATE as u32) * 4).to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?;
    f.write_all(&32u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&bytes.to_le_bytes())?;
    for v in samples {
        f.write_all(&(*v as f32).to_le_bytes())?;
    }
    Ok(())
}

fn main() {
    let out = std::env::args()
        .nth(1)
        .expect("usage: bass_presets OUT_DIR");
    let out = std::path::Path::new(&out);
    std::fs::create_dir_all(out).unwrap();
    for preset in PRESETS {
        for (note, name) in [(67_u8, "n67"), (76, "n76"), (84, "n84"), (93, "n93")] {
            for (velocity, vname) in [(28_u8, "pp"), (52, "p"), (80, "mf"), (112, "ff")] {
                let signal = render(preset, note, velocity);
                write_wav(&out.join(format!("{preset}-{name}-{vname}.wav")), &signal).unwrap();
            }
        }
    }
    println!("escritos en {}", out.display());
}
