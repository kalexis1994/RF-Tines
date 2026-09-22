//! How much broadband noise does the attack carry, ours against the reference?
//!
//! The reference's attack is not a harmonic ladder and not a set of modes: in
//! the first 24 ms at MIDI 52 its energy sits as much between the harmonics
//! as on them, 3 dB apart where ours is 19. It is noise, it comes from the
//! instrument and not from the recording -- 53 dB over that file's own floor
//! -- and it is steeply nonlinear in velocity, 45 dB from the softest layer
//! to the hardest while the fundamental moves 5.
//!
//! Adding the fourth and fifth bending modes did not touch it, because modes
//! are tones. See docs/HIGH-BENDING-MODES.md for that experiment.
//!
//! This measures the same statistic on both, layer by layer, so a model of
//! the contact has something to be wrong against.
//!
//! `cargo run --release -p rf-tines-plugin --example contact_noise -- SAMPLES`
use rackforge_plugin_sdk::{MidiEvent, Processor};
use rf_tines_plugin::RfTinesProcessor;
use std::{f64::consts::TAU, path::Path};

const RATE: f64 = 48_000.0;
const FRAMES: usize = 256;
const NOTE: u8 = 52;
/// The reference's four layers, and the velocities the plugin is driven at.
const LAYERS: [(&str, u8); 4] = [("p", 32), ("mp", 60), ("mf", 88), ("f", 108)];

fn read_wav(path: &Path) -> Result<(Vec<f64>, f64), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let (mut i, mut fmt, mut data) = (12, None, None);
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into()?) as usize;
        let body = &bytes[i + 8..(i + 8 + size).min(bytes.len())];
        match id {
            b"fmt " => fmt = Some(body.to_vec()),
            b"data" => data = Some(body.to_vec()),
            _ => {}
        }
        i += 8 + size + (size & 1);
    }
    let (fmt, data) = (fmt.ok_or("no fmt")?, data.ok_or("no data")?);
    let tag = u16::from_le_bytes(fmt[0..2].try_into()?);
    let channels = usize::from(u16::from_le_bytes(fmt[2..4].try_into()?));
    let rate = f64::from(u32::from_le_bytes(fmt[4..8].try_into()?));
    let bits = u16::from_le_bytes(fmt[14..16].try_into()?);
    let all: Vec<f64> = match (tag, bits) {
        (3, 32) => data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f64::from(f32::from_le_bytes(*c)))
            .collect(),
        (1, 16) => data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| f64::from(i16::from_le_bytes(*c)) / 32768.0)
            .collect(),
        (1, 24) => data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| {
                let v = i32::from(c[0]) | i32::from(c[1]) << 8 | (i32::from(c[2] as i8)) << 16;
                f64::from(v) / 8_388_608.0
            })
            .collect(),
        _ => return Err("unsupported wav".into()),
    };
    Ok((all.iter().step_by(channels).copied().collect(), rate))
}

fn render(velocity: u8) -> Vec<f64> {
    let mut plugin = RfTinesProcessor::default();
    assert!(plugin.load_preset("portable-bark-1972"));
    assert!(plugin.prepare(RATE, FRAMES as u32, 0, 2));
    let on = [MidiEvent {
        frame: 0,
        data: [0x90, NOTE, velocity],
        length: 3,
    }];
    let blocks = (RATE * 2.0 / FRAMES as f64) as usize;
    let mut out = Vec::with_capacity(blocks * FRAMES);
    for block in 0..blocks {
        let mut audio = [0.0_f32; FRAMES * 2];
        let events: &[MidiEvent] = if block == 0 { &on } else { &[] };
        plugin.process(&[], &mut audio, events, &[], FRAMES as u32, 0, 2);
        out.extend(audio.as_chunks::<2>().0.iter().map(|f| f64::from(f[0])));
    }
    out
}

fn line(block: &[f64], rate: f64, frequency: f64) -> f64 {
    let n = block.len();
    let (mut re, mut im) = (0.0, 0.0);
    for (i, value) in block.iter().enumerate() {
        let w = 0.5 - 0.5 * (TAU * i as f64 / n as f64).cos();
        let phase = TAU * frequency * i as f64 / rate;
        re += value * w * phase.cos();
        im += value * w * phase.sin();
    }
    (re * re + im * im) / (n * n) as f64
}

/// Energy between the harmonics in the attack, in dB under the fundamental,
/// and how flat the attack's spectrum is across the harmonic grid.
fn noise(signal: &[f64], rate: f64, f0: f64) -> (f64, f64) {
    let peak = signal.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    let onset = signal
        .iter()
        .position(|v| v.abs() > peak * 0.02)
        .unwrap_or(0);
    let attack = &signal[onset..(onset + (rate * 0.024) as usize).min(signal.len())];
    let tone = &signal[onset..(onset + (rate * 0.25) as usize).min(signal.len())];
    let between: f64 = (8..=40)
        .map(|h| f0 * (f64::from(h) + 0.5))
        .filter(|f| *f < rate / 2.0)
        .map(|f| line(attack, rate, f))
        .sum();
    let on_grid: f64 = (8..=40)
        .map(|h| f0 * f64::from(h))
        .filter(|f| *f < rate / 2.0)
        .map(|f| line(attack, rate, f))
        .sum();
    (
        10.0 * (between / line(tone, rate, f0).max(1e-30))
            .max(1e-30)
            .log10(),
        10.0 * (on_grid / between.max(1e-30)).max(1e-30).log10(),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: contact_noise SAMPLES")?;
    let source = Path::new(&source);
    let f0 = 440.0 * 2.0_f64.powf((f64::from(NOTE) - 69.0) / 12.0);

    println!("MIDI {NOTE}, ruido del ataque en los primeros 24 ms\n");
    println!(
        "{:<8}{:>26}{:>26}",
        "capa", "REAL  entre/fund  rejilla", "NUESTRO  entre/fund  rejilla"
    );
    for (layer, velocity) in LAYERS {
        let (reference, rate) = read_wav(&source.join(format!("E2-{layer}.wav")))?;
        let (real_noise, real_grid) = noise(&reference, rate, f0);
        let (ours_noise, ours_grid) = noise(&render(velocity), RATE, f0);
        println!(
            "{layer:<8}{real_noise:>16.1} dB{real_grid:>7.1}{ours_noise:>18.1} dB{ours_grid:>7.1}"
        );
    }
    println!("\nentre/fund: energia entre armonicos, en dB bajo el fundamental");
    println!("rejilla: cuanto mas hay sobre los armonicos que entre ellos (bajo = ruido)");
    Ok(())
}
