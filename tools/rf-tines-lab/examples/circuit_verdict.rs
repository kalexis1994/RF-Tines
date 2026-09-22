//! The circuit winner, frozen: reserved notes, and does it ring at one pitch?
//!
//! `circuit_fit` reached 60.33 dB^2 on the training notes with a lawful
//! geometry and neither the wedge nor the tonebar. Two checks decide whether
//! that means anything. The reserved notes, which no fit here has touched.
//! And the guard a listener's ear demanded after the previous candidate
//! reached 73.07 and turned out to ring at two pitches 78 cents apart.
//!
//! `cargo run --release -p rf-tines-lab --example circuit_verdict -- SAMPLES`
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Case, Result, baseline, load, render, score_with};
use rf_tines_dsp::{PickupLaw, Profile, Voice};
use std::path::Path;

const HELD_OUT: [u8; 6] = [36, 43, 64, 72, 84, 96];
const TRAINING: [u8; 9] = [30, 38, 50, 55, 59, 67, 76, 88, 98];
/// Exactly what `circuit_fit` left.
const WINNER: [f64; 7] = [1.5880, 0.5497, 0.2727, 0.5000, 0.0000, 0.5156, 1.2047];

fn register_gap_m(note: u8, bass_mm: f64, treble_mm: f64) -> f64 {
    let t = ((f64::from(note) - 28.0) / 72.0).clamp(0.0, 1.0);
    let w = t * t * (3.0 - 2.0 * t);
    (bass_mm * (1.0 - w) + treble_mm * w) * 1e-3
}

fn candidate(note: u8) -> Profile {
    let x = WINNER;
    Profile {
        pickup_law: PickupLaw::Reluctance,
        pickup_gap_m: register_gap_m(note, x[0], x[1]),
        pickup_offset_m: x[2] * 1e-3,
        pickup_pole_radius_m: x[3] * 1e-3,
        pickup_circuit_floor: x[4],
        maximum_hammer_speed_m_s: x[5],
        velocity_exponent: x[6],
        ..baseline()
    }
}

fn score(cases: &[Case], profile: impl Fn(u8) -> Profile) -> Result<f64> {
    let (mse, _) = score_with(cases, baseline(), false, |c, _, seconds| {
        render(c, profile(c.note), seconds)
    })?;
    Ok(mse)
}

/// Partials near the fundamental that are loud enough to be a second note.
fn second_pitches(profile: Profile, note: u8) -> Result<Vec<(f64, f64)>> {
    let target = 440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0);
    let mut voice = Voice::new(48_000.0, note, profile)?;
    voice.strike(0.85);
    for _ in 0..(48_000 * 4 * 3 / 10) {
        voice.tick();
    }
    let taken = (48_000.0 * 4.0 * 2.0) as usize;
    let samples: Vec<f64> = (0..taken).map(|_| voice.tick()).collect();
    let energy = |frequency: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, value) in samples.iter().enumerate() {
            let turn = i as f64 / samples.len() as f64;
            let window = 0.5 - 0.5 * (std::f64::consts::TAU * turn).cos();
            let phase = std::f64::consts::TAU * frequency * i as f64 / (48_000.0 * 4.0);
            re += value * window * phase.cos();
            im += value * window * phase.sin();
        }
        ((re * re + im * im).sqrt() / samples.len() as f64).max(1e-30)
    };
    let root = energy(target);
    let mut found = Vec::new();
    let mut cents = -350.0;
    while cents <= 350.0 {
        let frequency = target * 2.0_f64.powf(cents / 1200.0);
        if (frequency - target).abs() >= 1.5 {
            let level = 20.0 * (energy(frequency) / root).log10();
            if level > -24.0 {
                found.push((cents, level));
            }
        }
        cents += 12.5;
    }
    Ok(found)
}

fn main() -> Result<()> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: circuit_verdict SOURCE_SAMPLES")?;
    let source = Path::new(&source);

    println!("{:<12}{:>12}{:>12}{:>12}", "notas", "release", "circuito", "cambio");
    for (label, notes) in [("entrenam.", &TRAINING[..]), ("reservadas", &HELD_OUT[..])] {
        let cases = load(source, notes)?;
        let shipping = score(&cases, |_| baseline())?;
        let circuit = score(&cases, candidate)?;
        println!(
            "{label:<12}{shipping:>12.2}{circuit:>12.2}{:>11.1}%",
            (circuit / shipping - 1.0) * 100.0
        );
    }

    println!("
por nota reservada:");
    println!("{:>6}{:>12}{:>12}{:>12}", "nota", "release", "circuito", "cambio");
    let mut regressed = Vec::new();
    for note in HELD_OUT {
        let cases = load(source, &[note])?;
        let shipping = score(&cases, |_| baseline())?;
        let circuit = score(&cases, candidate)?;
        let change = (circuit / shipping - 1.0) * 100.0;
        println!("{note:>6}{shipping:>12.2}{circuit:>12.2}{change:>11.1}%");
        if change > 10.0 {
            regressed.push(note);
        }
    }
    println!(
        "
{}",
        if regressed.is_empty() {
            "Ninguna nota reservada empeora mas del 10%.".to_string()
        } else {
            format!("Empeoran mas del 10%: {regressed:?}")
        }
    );

    println!("
una sola altura?");
    let mut two_pitched = Vec::new();
    for note in [28, 40, 55, 76, 100] {
        let found = second_pitches(candidate(note), note)?;
        println!(
            "{note:>6}  {}",
            if found.is_empty() {
                "una".to_string()
            } else {
                format!("DOS: {found:?}")
            }
        );
        if !found.is_empty() {
            two_pitched.push(note);
        }
    }
    println!(
        "
{}",
        if two_pitched.is_empty() {
            "Pasa el guard que el oido pidio."
        } else {
            "NO pasa: suena a dos alturas."
        }
    );
    Ok(())
}
