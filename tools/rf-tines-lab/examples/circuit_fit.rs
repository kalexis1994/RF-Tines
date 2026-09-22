//! Does the magnetic circuit let a lawful geometry reach the bar?
//!
//! The primary prediction of docs/RELUCTANCE-PICKUP.md, and after three of
//! that document's premises were falsified, the only part of its case left
//! standing. The fit is confined to the service manual's gaps, and the wedge
//! and the tonebar are both switched off: if the circuit is the right model
//! it should not need either.
//!
//! The bar is 181.86 dB^2, what the illegal 0.5 mm dipole geometry reaches.
//! The second motion coordinate reached 226.77 and the tonebar 226.34.
//!
//! Training notes only; the reserved notes are never loaded here.
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Case, Result, baseline, load, render, score_with};
use rf_tines_dsp::{PickupLaw, Profile};
use std::path::Path;

const TRAINING_NOTES: [u8; 9] = [30, 38, 50, 55, 59, 67, 76, 88, 98];
const SHIPPING: f64 = 181.86;

const NAMES: [&str; 7] = [
    "hueco grave mm",
    "hueco agudo mm",
    "despl mm",
    "ancho polo mm",
    "hierro",
    "martillo m/s",
    "exponente",
];
/// Gaps inside the manual; the pole's half-width and the circuit's iron are
/// the shape parameters the dipole never had.
const BOUNDS: [(f64, f64); 7] = [
    (1.588, 3.175),
    (0.508, 3.175),
    (0.05, 1.5),
    (0.2, 3.0),
    (0.0, 20.0),
    (0.2, 1.6),
    (0.5, 3.0),
];
const STARTS: [[f64; 7]; 3] = [
    [1.588, 1.588, 0.5, 2.0, 1.0, 0.8, 1.4],
    [1.588, 0.508, 0.25, 0.5, 0.25, 0.8, 1.4],
    [2.4, 1.0, 1.0, 1.0, 6.0, 1.0, 1.4],
];

fn register_gap_m(note: u8, bass_mm: f64, treble_mm: f64) -> f64 {
    let t = ((f64::from(note) - 28.0) / 72.0).clamp(0.0, 1.0);
    let w = t * t * (3.0 - 2.0 * t);
    (bass_mm * (1.0 - w) + treble_mm * w) * 1e-3
}

fn profile(x: [f64; 7], note: u8) -> Profile {
    Profile {
        pickup_law: PickupLaw::Reluctance,
        pickup_gap_m: register_gap_m(note, x[0], x[1]),
        pickup_offset_m: x[2] * 1e-3,
        pickup_pole_radius_m: x[3] * 1e-3,
        pickup_circuit_floor: x[4],
        maximum_hammer_speed_m_s: x[5],
        velocity_exponent: x[6],
        // Both switched off, as the prediction says.
        pickup_pole_wedge: 0.0,
        tonebar_coupling: 0.0,
        ..baseline()
    }
}

fn score(cases: &[Case], x: [f64; 7]) -> f64 {
    match score_with(cases, baseline(), false, |c, _, seconds| {
        render(c, profile(x, c.note), seconds)
    }) {
        Ok((mse, _)) => mse,
        Err(_) => f64::INFINITY,
    }
}

fn main() -> Result<()> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: circuit_fit SOURCE_SAMPLES")?;
    let cases = load(Path::new(&source), &TRAINING_NOTES)?;
    println!("{} casos. Ley de circuito, sin cuna ni tonebar.", cases.len());
    println!("a batir: {SHIPPING:.2} dB^2
");

    let mut best = STARTS[0];
    let mut best_score = f64::INFINITY;
    for (index, start) in STARTS.into_iter().enumerate() {
        let mut here = start;
        let mut here_score = score(&cases, here);
        println!("arranque {index}: {here_score:.2}");
        for round in 0..5 {
            let fraction = 0.5_f64.powi(round);
            for dim in 0..7 {
                let step = (BOUNDS[dim].1 - BOUNDS[dim].0) * 0.25 * fraction;
                for sign in [-1.0, 1.0] {
                    let mut trial = here;
                    trial[dim] = (trial[dim] + sign * step).clamp(BOUNDS[dim].0, BOUNDS[dim].1);
                    if trial[dim] == here[dim] {
                        continue;
                    }
                    let loss = score(&cases, trial);
                    if loss < here_score {
                        here = trial;
                        here_score = loss;
                    }
                }
            }
        }
        println!("  termina en {here_score:.2}");
        if here_score < best_score {
            best = here;
            best_score = here_score;
        }
    }

    println!("
mejor {best_score:.2} dB^2");
    for (name, value) in NAMES.iter().zip(best) {
        println!("  {name:<16} {value:.4}");
    }
    println!(
        "
{} ({:+.1}% contra {SHIPPING:.2})",
        if best_score <= SHIPPING { "CUMPLE" } else { "NO CUMPLE" },
        (best_score / SHIPPING - 1.0) * 100.0
    );
    Ok(())
}
