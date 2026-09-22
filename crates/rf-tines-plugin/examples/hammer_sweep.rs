//! Sweeps one plugin parameter and reports what it does to the attack.
//!
//! Written to answer whether the hammer was too light -- it is not: three
//! times the mass moves the transient from 1.08 to 1.10 against the
//! reference's 1.52, while adding 12.4 dB of attack brightness, so mass
//! colours the click and does not enlarge it. It was then pointed at contact
//! stiffness and at pickup alignment in turn, and the constants below say
//! which parameter it currently carries.
//!
//! `PARAMETER_HAMMER` is the index swept and `MASSES` the values; both names
//! are from its first use and are kept only because every figure quoted in
//! docs/POLE-THRESHOLD.md came out of this file under them.
//!
//! `cargo run --release -p rf-tines-plugin --example hammer_sweep -- OUT_DIR`
use rackforge_plugin_sdk::{MidiEvent, Processor};
use rf_tines_plugin::RfTinesProcessor;
use std::{f64::consts::TAU, fs, io::Write, path::Path};

const RATE: f64 = 48_000.0;
const FRAMES: usize = 256;
const NOTE: u8 = 52;
const PARAMETER_HAMMER: u32 = 3;
/// What `portable-bark-1972` ships, and four steps either side.
const MASSES: [f64; 5] = [0.05, 0.30, 0.60, 1.00, 1.60];

fn render(hammer: f64) -> Vec<f64> {
    let mut plugin = RfTinesProcessor::default();
    assert!(plugin.load_preset("portable-bark-1972"));
    assert!(plugin.set_parameter(PARAMETER_HAMMER, hammer));
    assert!(plugin.prepare(RATE, FRAMES as u32, 0, 2));
    let on = [MidiEvent {
        frame: 0,
        data: [0x90, NOTE, 108],
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

fn band(block: &[f64], lo: f64, hi: f64) -> f64 {
    let n = block.len();
    if n < 64 {
        return 1e-30;
    }
    let mut total = 0.0;
    for step in 0..24 {
        let f = lo * (hi / lo).powf(f64::from(step) / 23.0);
        let (mut re, mut im) = (0.0, 0.0);
        for (i, value) in block.iter().enumerate() {
            let w = 0.5 - 0.5 * (TAU * i as f64 / n as f64).cos();
            let phase = TAU * f * i as f64 / RATE;
            re += value * w * phase.cos();
            im += value * w * phase.sin();
        }
        total += (re * re + im * im) / (n * n) as f64;
    }
    total
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
        .ok_or("usage: hammer_sweep OUT_DIR")?;
    let out = Path::new(&out);
    fs::create_dir_all(out)?;
    println!("MIDI {NOTE} forte. el real: transitorio 1.52\n");
    println!(
        "{:>8}{:>10}{:>14}{:>16}",
        "align mm", "", "transitorio", "250-330 vs cuerpo"
    );
    for hammer in MASSES {
        let signal = render(hammer);
        let peak = signal.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        let onset = signal
            .iter()
            .position(|v| v.abs() > peak * 0.02)
            .unwrap_or(0);
        let attack = &signal[onset..onset + (RATE * 0.02) as usize];
        let body = &signal[onset + (RATE * 0.1) as usize..onset + (RATE * 0.5) as usize];
        let transient = attack.iter().fold(0.0_f64, |m, v| m.max(v.abs()))
            / body.iter().fold(1e-30_f64, |m, v| m.max(v.abs()));
        let bright = 10.0
            * (band(attack, 240.0, 340.0)
                / band(
                    &signal[onset + (RATE * 0.1) as usize..onset + (RATE * 0.4) as usize],
                    150.0,
                    500.0,
                )
                .max(1e-30))
            .max(1e-30)
            .log10();
        let grams = hammer;
        println!("{hammer:>8.2}{grams:>10.2}{transient:>14.2}{bright:>13.1} dB");
        write_wav(&out.join(format!("align-{hammer:.2}.wav")), &signal)?;
    }
    Ok(())
}
