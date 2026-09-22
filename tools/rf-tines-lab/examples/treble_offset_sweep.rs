//! Can where the tine rests fix the treble?
//!
//! `treble_diagnosis` showed the shape of the failure: across the treble the
//! model has far too little second harmonic and far too much third. That is
//! a symmetry signature. A transducer whose transfer is odd about the tine's
//! resting point makes odd harmonics and no even ones; moving the tine off
//! centre breaks the symmetry and even harmonics appear.
//!
//! The voicing grades the pickup gap across the register and leaves the
//! offset a single global number, which is the least justified part of it:
//! the service manual sets the tine slightly above dead centre of the pole
//! and a technician regulates every one of them. So before grading it, ask
//! whether grading it could even help — walk the offset on the notes that
//! fail and watch the pair move.
//!
//! `cargo run --release -p rf-tines-lab --example treble_offset_sweep -- SAMPLES`
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Result, baseline, load, render, score_with};
use rf_tines_dsp::{PickupLaw, Profile};
use std::path::Path;

/// The circuit winner from docs/RELUCTANCE-PICKUP.md.
const WINNER: [f64; 7] = [1.5880, 0.5497, 0.2727, 0.5000, 0.0000, 0.5156, 1.2047];
const OFFSETS_MM: [f64; 7] = [0.02, 0.05, 0.12, 0.2727, 0.5, 0.9, 1.4];
/// Training notes only.
///
/// A first pass walked 64, 84 and 96 — three reserved notes — because they
/// were the ones failing the gate. That was diagnosis on the reserve, which
/// the frozen protocol forbids for exactly the reason it forbids fitting on
/// it: the six are weaker for it now, and docs/RELUCTANCE-PICKUP.md records
/// that they were spent. Everything below stays on notes the fits already
/// use.
const NOTES: [u8; 9] = [30, 38, 50, 55, 59, 67, 76, 88, 98];

fn register_gap_m(note: u8, bass_mm: f64, treble_mm: f64) -> f64 {
    let t = ((f64::from(note) - 28.0) / 72.0).clamp(0.0, 1.0);
    let w = t * t * (3.0 - 2.0 * t);
    (bass_mm * (1.0 - w) + treble_mm * w) * 1e-3
}

fn candidate(note: u8, offset_mm: f64) -> Profile {
    let x = WINNER;
    Profile {
        pickup_law: PickupLaw::Reluctance,
        pickup_gap_m: register_gap_m(note, x[0], x[1]),
        pickup_offset_m: offset_mm * 1e-3,
        pickup_pole_radius_m: x[3] * 1e-3,
        pickup_circuit_floor: x[4],
        maximum_hammer_speed_m_s: x[5],
        velocity_exponent: x[6],
        ..baseline()
    }
}

fn main() -> Result<()> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: treble_offset_sweep SOURCE_SAMPLES")?;
    let source = Path::new(&source);
    println!("error medio en el ataque, dB. + = el modelo tiene de MAS.");
    println!("el candidato usa 0.2727 mm en todo el teclado.\n");
    for note in NOTES {
        let cases = load(source, &[note])?;
        println!("--- nota {note} ---");
        println!("{:<11}{:>10}{:>10}{:>11}", "despl mm", "H2", "H3", "MSE");
        let mut best: Option<(f64, f64)> = None;
        for offset in OFFSETS_MM {
            let (mse, rows) = score_with(&cases, baseline(), true, |c, _, seconds| {
                render(c, candidate(c.note, offset), seconds)
            })?;
            let mut second = Vec::new();
            let mut third = Vec::new();
            for row in &rows {
                for term in row["balances"].as_array().into_iter().flatten() {
                    if term["window"].as_u64() != Some(1) {
                        continue;
                    }
                    let error = term["error_db"].as_f64().unwrap_or(0.0);
                    match term["harmonic"].as_u64() {
                        Some(2) => second.push(error),
                        Some(3) => third.push(error),
                        _ => {}
                    }
                }
            }
            let mean = |values: &[f64]| {
                if values.is_empty() {
                    f64::NAN
                } else {
                    values.iter().sum::<f64>() / values.len() as f64
                }
            };
            println!(
                "{offset:<11}{:>10.1}{:>10.1}{mse:>11.1}",
                mean(&second),
                mean(&third)
            );
            if best.is_none_or(|(score, _)| mse < score) {
                best = Some((mse, offset));
            }
        }
        if let Some((score, offset)) = best {
            let shipping = score_with(&cases, baseline(), false, |c, _, seconds| {
                render(c, candidate(c.note, WINNER[2]), seconds)
            })?
            .0;
            println!(
                "mejor {offset} mm -> {score:.1}, contra {shipping:.1} con el global ({:+.1}%)\n",
                (score / shipping - 1.0) * 100.0
            );
        }
    }
    Ok(())
}
