//! Renders the pickup gap sweep through the plugin, for listening.
//!
//! docs/POLE-THRESHOLD.md measures what the gap does to the attack: at the
//! 500 um the profile ships, the tine swings past the pole and the pickup
//! folds, which is where the model's attack character comes from; at the
//! service manual's 1.588 mm the fold never happens and every pickup law
//! goes quiet together. Those are numbers. This writes the sound.
//!
//! `cargo run --release -p rf-tines-plugin --example gap_audition -- OUT_DIR`
use rackforge_plugin_sdk::{MidiEvent, Processor};
use rf_tines_plugin::RfTinesProcessor;
use std::{fs, io::Write, path::Path};

const RATE: f64 = 48_000.0;
const FRAMES: usize = 256;
const NOTE: u8 = 52;
const PARAMETER_DISTANCE: u32 = 2;
/// What ships, the two the fold favours, and the manual's own minimum.
const GAPS: [(f64, &str); 4] = [
    (0.6, "envia-0.6mm"),
    (0.7, "fold-0.7mm"),
    (1.0, "plano-1.0mm"),
    (1.588, "manual-1.588mm"),
];

fn render(distance_mm: f64, velocity: u8) -> Vec<f64> {
    let mut plugin = RfTinesProcessor::default();
    assert!(plugin.load_preset("portable-bark-1972"));
    assert!(plugin.set_parameter(PARAMETER_DISTANCE, distance_mm));
    assert!(plugin.prepare(RATE, FRAMES as u32, 0, 2));
    let on = [MidiEvent {
        frame: 0,
        data: [0x90, NOTE, velocity],
        length: 3,
    }];
    let blocks = (RATE * 3.0 / FRAMES as f64) as usize;
    let mut out = Vec::with_capacity(blocks * FRAMES);
    for block in 0..blocks {
        let mut audio = [0.0_f32; FRAMES * 2];
        let events: &[MidiEvent] = if block == 0 { &on } else { &[] };
        plugin.process(&[], &mut audio, events, &[], FRAMES as u32, 0, 2);
        out.extend(audio.as_chunks::<2>().0.iter().map(|f| f64::from(f[0])));
    }
    out
}

fn write_wav(path: &Path, samples: &[f64]) -> std::io::Result<()> {
    let bytes = (samples.len() * 4) as u32;
    let mut file = fs::File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&3u16.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&(RATE as u32).to_le_bytes())?;
    file.write_all(&((RATE as u32) * 4).to_le_bytes())?;
    file.write_all(&4u16.to_le_bytes())?;
    file.write_all(&32u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&bytes.to_le_bytes())?;
    for value in samples {
        file.write_all(&(*value as f32).to_le_bytes())?;
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .ok_or("usage: gap_audition OUT_DIR")?;
    let out = Path::new(&out);
    fs::create_dir_all(out)?;
    for (distance, name) in GAPS {
        for (velocity, layer) in [(108_u8, "f"), (32, "p")] {
            write_wav(
                &out.join(format!("{name}-{layer}.wav")),
                &render(distance, velocity),
            )?;
        }
    }
    println!("escritos en {}", out.display());
    Ok(())
}
