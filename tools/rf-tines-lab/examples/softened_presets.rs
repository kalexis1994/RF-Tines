//! What the softened contact does to the presets that actually ship.
//!
//! The mapping moved: `hardness` now centres on 4e8, which holds the hammer
//! on the tine for 547 us, instead of 4e10 and 116 us. Every preset keeps
//! its own hardness value, so the era ordering is untouched and all of them
//! move together onto a contact the piano literature recognises.
//!
//! This scores the default preset's profile before and after, on the
//! training notes, and reports the tick alongside it.
//!
//! `cargo run --release -p rf-tines-lab --example softened_presets -- SAMPLES`
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Result, baseline, load, render, score_with};
use rf_tines_dsp::{Engine, Profile};
use std::{f64::consts::TAU, path::Path};

const TRAINING: [u8; 9] = [30, 38, 50, 55, 59, 67, 76, 88, 98];
const RATE: f64 = 48_000.0;
/// What `hardness` 0.46, the default preset's value, used to mean and means.
const BEFORE: f64 = 4.0e10 * 0.775;
const AFTER: f64 = 4.0e8 * 0.9575;

fn tick(stiffness: f64) -> Result<f64> {
    let mut engine = Engine::new(
        RATE,
        Profile {
            contact_stiffness: stiffness,
            ..baseline()
        },
    )?;
    engine.set_gain(0.5);
    engine.set_level_compensation(true);
    engine.reset();
    engine.note_on(0, 55, 0.85);
    let signal: Vec<f64> = (0..(RATE * 1.5) as usize)
        .map(|_| f64::from(engine.next_sample()))
        .collect();
    let start = signal.iter().position(|x| x.abs() > 1e-4).unwrap_or(0);
    let band = |from: usize, length: usize| {
        let end = (from + length).min(signal.len());
        let block = &signal[from..end];
        let mut total = 0.0;
        for i in 0..48 {
            let frequency = 5000.0 * 2.0_f64.powf(i as f64 / 47.0);
            let (mut re, mut im) = (0.0, 0.0);
            for (n, value) in block.iter().enumerate() {
                let turn = n as f64 / block.len() as f64;
                let window = 0.5 - 0.5 * (TAU * turn).cos();
                let phase = TAU * frequency * n as f64 / RATE;
                re += value * window * phase.cos();
                im += value * window * phase.sin();
            }
            total += (re * re + im * im) / (block.len() * block.len()) as f64;
        }
        10.0 * total.max(1e-30).log10()
    };
    Ok(band(start, (RATE * 0.012) as usize)
        - band(start + (RATE * 0.40) as usize, (RATE * 0.50) as usize))
}

fn main() -> Result<()> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: softened_presets SOURCE_SAMPLES")?;
    let cases = load(Path::new(&source), &TRAINING)?;
    println!(
        "perfil del preset por defecto, dureza 0.46
"
    );
    println!(
        "{:<12}{:>14}{:>12}{:>14}",
        "", "rigidez", "MSE", "tick 5-10k"
    );
    let mut first = None;
    for (label, stiffness) in [("antes", BEFORE), ("ahora", AFTER)] {
        let (mse, _) = score_with(&cases, baseline(), false, |c, _, seconds| {
            render(
                c,
                Profile {
                    contact_stiffness: stiffness,
                    ..baseline()
                },
                seconds,
            )
        })?;
        let click = tick(stiffness)?;
        println!("{label:<12}{stiffness:>14.1e}{mse:>12.2}{click:>14.1}");
        let base = *first.get_or_insert(mse);
        if label == "ahora" {
            println!(
                "
cambio en el objetivo: {:+.1}%",
                (mse / base - 1.0) * 100.0
            );
        }
    }
    Ok(())
}
