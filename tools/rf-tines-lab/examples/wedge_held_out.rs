//! The wedge's winner, frozen, against notes the search never saw.
//!
//! `two_plane_fit` reached 73.05 dB² on the nine training notes with a legal
//! geometry, where the bar was the illegal geometry's 181.86. A training
//! score is not a result: the coordinates below are frozen exactly as the
//! search left them, and scored on the reserved notes for the first time.
//!
//! The reserved set is the frozen protocol's — 36, 43, 64, 72, 84 and 96 —
//! and no fit in this work has touched it. It is weaker than pristine, and
//! docs/WEDGE-POLE.md says why: this session measured the error map across
//! all 73 notes, so those six have been seen in aggregate even though
//! nothing was tuned on them.
//!
//! `cargo run --release -p rf-tines-lab --example wedge_held_out -- SAMPLES`
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Case, Result, baseline, load, render, score_with};
use rf_tines_dsp::Profile;
use std::path::Path;

const HELD_OUT: [u8; 6] = [36, 43, 64, 72, 84, 96];
const TRAINING: [u8; 9] = [30, 38, 50, 55, 59, 67, 76, 88, 98];
/// Exactly what the search left, unrounded and unretouched.
const WINNER: [f64; 12] = [
    1.5880, 0.9213, 0.6359, -0.4375, 0.3250, 1.0434, 1.3250, 1.1656, 0.5625, 1.4016, 1.0000,
    1.0625,
];

fn register_gap_m(note: u8, bass_mm: f64, treble_mm: f64) -> f64 {
    let t = ((f64::from(note) - 28.0) / 72.0).clamp(0.0, 1.0);
    let w = t * t * (3.0 - 2.0 * t);
    (bass_mm * (1.0 - w) + treble_mm * w) * 1e-3
}

fn candidate(note: u8) -> Profile {
    let x = WINNER;
    Profile {
        pickup_gap_m: register_gap_m(note, x[0], x[1]),
        pickup_offset_m: x[2] * 1e-3,
        pickup_transverse_offset_m: x[3] * 1e-3,
        tine_boundary_angle_rad: x[4],
        tine_transverse_frequency_ratio: x[5],
        maximum_hammer_speed_m_s: x[6],
        velocity_exponent: x[7],
        tonebar_coupling: x[8],
        tonebar_frequency_ratio: x[9],
        pickup_pole_wedge: x[10],
        pickup_pole_radius_m: x[11] * 1e-3,
        ..baseline()
    }
}

fn score(cases: &[Case], profile: impl Fn(u8) -> Profile) -> Result<f64> {
    let (mse, _) = score_with(cases, baseline(), false, |c, _, seconds| {
        render(c, profile(c.note), seconds)
    })?;
    Ok(mse)
}

fn main() -> Result<()> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: wedge_held_out SOURCE_SAMPLES")?;
    let source = Path::new(&source);

    println!("{:<12}{:>12}{:>12}{:>12}", "notas", "release", "cuna", "cambio");
    let mut verdicts = Vec::new();
    for (label, notes) in [("entrenam.", &TRAINING[..]), ("reservadas", &HELD_OUT[..])] {
        let cases = load(source, notes)?;
        let shipping = score(&cases, |_| baseline())?;
        let wedge = score(&cases, candidate)?;
        let change = (wedge / shipping - 1.0) * 100.0;
        println!("{label:<12}{shipping:>12.2}{wedge:>12.2}{change:>11.1}%");
        verdicts.push((label, shipping, wedge, change));
    }

    // The frozen protocol's rule: at least ten percent off both splits, and
    // no reserved note allowed to get more than ten percent worse.
    println!();
    let promotes = verdicts.iter().all(|(_, _, _, change)| *change <= -10.0);
    if promotes {
        println!("Ambas mitades bajan mas del 10%: pasa la regla agregada.");
    } else {
        println!("NO pasa la regla agregada del 10% en ambas mitades.");
    }

    println!("\npor nota reservada:");
    println!("{:>6}{:>12}{:>12}{:>12}", "nota", "release", "cuna", "cambio");
    let mut regressed = Vec::new();
    for note in HELD_OUT {
        let cases = load(source, &[note])?;
        let shipping = score(&cases, |_| baseline())?;
        let wedge = score(&cases, candidate)?;
        let change = (wedge / shipping - 1.0) * 100.0;
        println!("{note:>6}{shipping:>12.2}{wedge:>12.2}{change:>11.1}%");
        if change > 10.0 {
            regressed.push(note);
        }
    }
    if regressed.is_empty() {
        println!("\nNinguna nota reservada empeora mas del 10%.");
    } else {
        println!("\nEmpeoran mas del 10%: {regressed:?}");
    }
    Ok(())
}
