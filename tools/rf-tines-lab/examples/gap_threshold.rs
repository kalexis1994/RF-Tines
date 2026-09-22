//! Is the attack noise the pickup, folded by a tine that swings past the pole?
//!
//! docs/STRUCK-FRAME.md ends needing a mechanism with a threshold: something
//! that barely happens at piano and dominates at forte, landing between the
//! harmonics rather than on them. Three additive models failed because a
//! linear source driven by the blow grows exactly as the tone does.
//!
//! The clearance probe found the tine swinging one to five times past the
//! pole at forte, and under half the gap at piano. That is a threshold, and
//! it is already in the model. The content it makes is between the harmonics
//! and not on them because the partials themselves are not harmonic: 1,
//! 6.267 and 17.55 mix to 5.267, 11.28 and the rest, none of them integers.
//!
//! So this asks the question with the parameters that already exist, before
//! anything is built. Nothing is fitted and nothing is promoted.
//!
//! `cargo run --release -p rf-tines-lab --example gap_threshold -- SAMPLES`
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;
use previous::{Result, baseline};
use rf_tines_dsp::{Engine, PickupLaw, Profile};
use std::{f64::consts::TAU, path::Path};

const RATE: f64 = 48_000.0;
const NOTE: u8 = 52;
const LAYERS: [(&str, f64); 4] = [("p", 0.25), ("mp", 0.45), ("mf", 0.65), ("f", 0.85)];
/// Measured on the reference, in dB under each file's own fundamental.
const TARGET: [f64; 4] = [-57.7, -49.0, -50.0, -17.6];

fn read_wav(path: &Path) -> Result<(Vec<f64>, f64)> {
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
            .chunks_exact(4)
            .map(|c| f64::from(f32::from_le_bytes(c.try_into().unwrap())))
            .collect(),
        (1, 16) => data
            .chunks_exact(2)
            .map(|c| f64::from(i16::from_le_bytes(c.try_into().unwrap())) / 32768.0)
            .collect(),
        (1, 24) => data
            .chunks_exact(3)
            .map(|c| {
                let v = i32::from(c[0]) | i32::from(c[1]) << 8 | (i32::from(c[2] as i8)) << 16;
                f64::from(v) / 8_388_608.0
            })
            .collect(),
        _ => return Err("unsupported wav".into()),
    };
    Ok((all.iter().step_by(channels).copied().collect(), rate))
}

fn render(profile: Profile, velocity: f64) -> Result<Vec<f64>> {
    let mut engine = Engine::new(RATE, profile)?;
    engine.set_gain(0.5);
    engine.set_level_compensation(true);
    engine.reset();
    engine.note_on(0, NOTE, velocity);
    Ok((0..(RATE * 0.6) as usize)
        .map(|_| f64::from(engine.next_sample()))
        .collect())
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

/// Between-harmonic energy in the attack, in dB under the fundamental, and
/// how much more sits on the grid than between it.
fn statistic(signal: &[f64], rate: f64, f0: f64) -> (f64, f64) {
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
    let grid: f64 = (8..=40)
        .map(|h| f0 * f64::from(h))
        .filter(|f| *f < rate / 2.0)
        .map(|f| line(attack, rate, f))
        .sum();
    (
        10.0 * (between / line(tone, rate, f0).max(1e-30))
            .max(1e-30)
            .log10(),
        10.0 * (grid / between.max(1e-30)).max(1e-30).log10(),
    )
}

fn main() -> Result<()> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: gap_threshold SAMPLES")?;
    let source = Path::new(&source);
    let f0 = 440.0 * 2.0_f64.powf((f64::from(NOTE) - 69.0) / 12.0);
    let (reference, rate) = read_wav(&source.join("E2-f.wav"))?;
    let (_, reference_grid) = statistic(&reference, rate, f0);

    println!("MIDI {NOTE}: energia entre armonicos en el ataque, dB bajo el fundamental");
    println!(
        "objetivo   p {:.1}   mp {:.1}   mf {:.1}   f {:.1}",
        TARGET[0], TARGET[1], TARGET[2], TARGET[3]
    );
    println!("rejilla del real en forte: {reference_grid:.1} dB\n");
    println!(
        "{:<18}{:>8}{:>9}{:>9}{:>9}{:>9}{:>11}{:>10}",
        "ley", "hueco", "p", "mp", "mf", "f", "rejilla f", "transit."
    );
    // The manual's box: 1/16 in to 1/8 in, with 0.020 in allowed only on
    // pianos after March 1972 and only in the middle and upper ranges. The
    // 500 um the profile ships is outside all of it, and is kept as the
    // reference point rather than dropped.
    for (name, law) in [
        ("Production", PickupLaw::Production),
        ("Aperture", PickupLaw::Aperture),
        ("RegisterAperture", PickupLaw::RegisterAperture),
        ("Reluctance", PickupLaw::Reluctance),
    ] {
        for gap_um in [500.0, 1588.0, 2400.0, 3175.0] {
            let profile = Profile {
                pickup_law: law,
                pickup_gap_m: gap_um * 1e-6,
                ..baseline()
            };
            let mut row = Vec::new();
            let mut grid = 0.0;
            for (_, velocity) in LAYERS {
                let (between, g) = statistic(&render(profile, velocity)?, RATE, f0);
                row.push(between);
                grid = g;
            }
            let loud = render(profile, 0.85)?;
            let attack = loud[..(RATE * 0.02) as usize]
                .iter()
                .fold(0.0_f64, |m, v| m.max(v.abs()));
            let body = loud[(RATE * 0.1) as usize..]
                .iter()
                .fold(0.0_f64, |m, v| m.max(v.abs()));
            print!("{name:<18}{gap_um:>8.0}");
            for value in &row {
                print!("{value:>9.1}");
            }
            println!("{grid:>11.1}{:>10.2}", attack / body.max(1e-30));
        }
    }
    println!("\nel real sube {:.1} dB de p a f", TARGET[3] - TARGET[0]);
    Ok(())
}
