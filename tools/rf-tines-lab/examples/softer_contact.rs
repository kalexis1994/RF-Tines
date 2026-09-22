//! What does softening the hammer contact sound like?
//!
//! The fit reports harmonic balance error, which is not a loudness and not a
//! perception. This renders the shipping voicing and the circuit candidate side
//! by side and measures the distance between them with the same yardstick the
//! preset work used, so the answer lands on a scale that has already been
//! listened to: the eight instrument presets sit 5.32 dB apart on average,
//! and under about 2 dB two of them are the same sound with another name.
//!
//! It also writes both renders, because a number is not a listening test.
//!
//! `cargo run --release -p rf-tines-lab --example softer_contact -- OUT_DIR`
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Result, baseline};
use rf_tines_dsp::{Engine, Profile};
use std::{fs, io::Write, path::Path};

const RATE: u32 = 48_000;
const SECONDS: f64 = 3.0;
/// The contact stiffness that scores best on the training notes, a hundred
/// times softer than what ships. Everything else is the shipping voicing.
const SOFTER: f64 = 4.0e8;
const NOTES: [(u8, &str); 3] = [(40, "E2"), (55, "G3"), (76, "E5")];
const LAYERS: [(f64, &str); 2] = [(0.3, "p"), (0.85, "f")];

fn wedge(_note: u8) -> Profile {
    Profile {
        contact_stiffness: SOFTER,
        ..baseline()
    }
}

fn render(profile: Profile, note: u8, velocity: f64) -> Result<Vec<f64>> {
    let mut engine = Engine::new(f64::from(RATE), profile)?;
    engine.set_gain(0.5);
    engine.set_level_compensation(true);
    engine.reset();
    engine.note_on(0, note, velocity);
    Ok((0..(f64::from(RATE) * SECONDS) as usize)
        .map(|_| f64::from(engine.next_sample()))
        .collect())
}

/// Level-matched spectral distance, the yardstick the preset work used.
fn apart(a: &[f64], b: &[f64]) -> f64 {
    let colour = |x: &[f64]| {
        let peak = x.iter().fold(0.0_f64, |m, v| m.max(v.abs())).max(1e-30);
        let onset = x.iter().position(|v| v.abs() >= peak * 0.05).unwrap_or(0);
        let block = &x[onset..(onset + RATE as usize).min(x.len())];
        let mut bands: Vec<f64> = (0..29)
            .map(|step| {
                let frequency = 80.0 * 2.0_f64.powf(step as f64 / 4.0);
                let (mut re, mut im) = (0.0, 0.0);
                for (i, value) in block.iter().enumerate() {
                    let phase = std::f64::consts::TAU * frequency * i as f64 / f64::from(RATE);
                    re += value * phase.cos();
                    im += value * phase.sin();
                }
                (re * re + im * im).sqrt() / block.len() as f64
            })
            .collect();
        let top = bands.iter().fold(0.0_f64, |m, v| m.max(*v)).max(1e-30);
        for value in &mut bands {
            *value = 20.0 * (*value / top).max(1e-5).log10();
        }
        bands
    };
    let (x, y) = (colour(a), colour(b));
    (x.iter()
        .zip(&y)
        .map(|(p, q)| (p - q) * (p - q))
        .sum::<f64>()
        / x.len() as f64)
        .sqrt()
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
    file.write_all(&RATE.to_le_bytes())?;
    file.write_all(&(RATE * 4).to_le_bytes())?;
    file.write_all(&4u16.to_le_bytes())?;
    file.write_all(&32u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&bytes.to_le_bytes())?;
    for value in samples {
        file.write_all(&(*value as f32).to_le_bytes())?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let out = std::env::args()
        .nth(1)
        .ok_or("usage: wedge_audition OUT_DIRECTORY")?;
    let out = Path::new(&out);
    fs::create_dir_all(out)?;

    println!("distancia espectral entre release y el contacto blando, dB RMS con nivel igualado");
    println!("referencia: los ocho presets estan a 5.32 dB de promedio entre si,");
    println!("y por debajo de ~2 dB dos de ellos son el mismo sonido.\n");
    println!(
        "{:<8}{:<7}{:>12}{:>14}",
        "nota", "capa", "distancia", "nivel dB"
    );
    let mut all = Vec::new();
    for (note, name) in NOTES {
        for (velocity, layer) in LAYERS {
            let shipping = render(baseline(), note, velocity)?;
            let candidate = render(wedge(note), note, velocity)?;
            let distance = apart(&shipping, &candidate);
            let rms = |x: &[f64]| (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt();
            let level = 20.0 * (rms(&candidate) / rms(&shipping).max(1e-30)).log10();
            println!("{name:<8}{layer:<7}{distance:>12.2}{level:>+14.1}");
            all.push(distance);
            write_wav(&out.join(format!("{name}-{layer}-release.wav")), &shipping)?;
            write_wav(&out.join(format!("{name}-{layer}-blando.wav")), &candidate)?;
        }
    }
    println!(
        "\npromedio {:.2} dB sobre {} comparaciones",
        all.iter().sum::<f64>() / all.len() as f64,
        all.len()
    );
    println!("escritos en {}", out.display());
    Ok(())
}
