//! The three converging axes together, for listening.
//!
//! Each was found separately and each fixed one band and no other:
//! pickup alignment moves the attack's 250 and 330 Hz, where the reference
//! peaks and we were 14 and 19 dB short; contact stiffness moves 6-11 kHz,
//! the only thing all day that did; and tonebar coupling brings 250 Hz from
//! -34.7 to -13.4 against the reference's -12.8.
//!
//! The literature says the same three things make the bark: pickup proximity
//! and alignment, escapement, and the overtones of the tine *and tone bar*.
//! Nothing here is a new mechanism; all three already existed and were off or
//! in the wrong place. docs/POLE-THRESHOLD.md and docs/PLAYABLE-TONEBAR.md
//! record why, and it was the same reason each time: a frozen objective that
//! scores H2-H4 over windows far longer than the attack and cannot see it.
//!
//! `cargo run --release -p rf-tines-plugin --example combo_audition -- OUT_DIR`
use rackforge_plugin_sdk::{MidiEvent, Processor};
use rf_tines_plugin::RfTinesProcessor;
use std::{fs, io::Write, path::Path};

const RATE: f64 = 48_000.0;
const FRAMES: usize = 256;
const NOTE: u8 = 52;
const PARAMETER_ALIGNMENT: u32 = 3;
const PARAMETER_HARDNESS: u32 = 4;

fn render(alignment: f64, hardness: f64, velocity: u8) -> Vec<f64> {
    let mut plugin = RfTinesProcessor::default();
    assert!(plugin.load_preset("portable-bark-1972"));
    assert!(plugin.set_parameter(PARAMETER_ALIGNMENT, alignment));
    assert!(plugin.set_parameter(PARAMETER_HARDNESS, hardness));
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
        .ok_or("usage: combo_audition OUT_DIR")?;
    let tag = std::env::args().nth(2).unwrap_or_else(|| "x".into());
    let out = Path::new(&out);
    fs::create_dir_all(out)?;
    for (alignment, hardness, name) in [
        (0.6, 0.45, "envia"),
        (0.30, 1.00, "combo-a030"),
        (0.15, 1.00, "combo-a015"),
    ] {
        for (velocity, layer) in [(108_u8, "f"), (32, "p")] {
            write_wav(
                &out.join(format!("{name}-{tag}-{layer}.wav")),
                &render(alignment, hardness, velocity),
            )?;
        }
    }
    println!("escritos en {}", out.display());
    Ok(())
}
