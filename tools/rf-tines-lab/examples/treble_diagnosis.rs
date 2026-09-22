//! What exactly is wrong with the treble?
//!
//! Two candidates built on different physics both regress on note 84, and
//! it is the worst regression in both. The register map taken at the start
//! of this work already found the model's velocity-to-brightness slope
//! locking flat above A3 while the reference falls and turns negative in the
//! top octave, so the treble was wrong before any of this began.
//!
//! Before changing anything, this prints the shape of the error rather than
//! its size: every harmonic the objective scores, in both its windows, at
//! every dynamic layer, for the notes that fail and a note that does not.
//!
//! `cargo run --release -p rf-tines-lab --example treble_diagnosis -- SAMPLES`
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Result, baseline, load, render, score_with};
use rf_tines_dsp::{PickupLaw, Profile};
use std::path::Path;

/// The circuit winner from docs/RELUCTANCE-PICKUP.md.
const WINNER: [f64; 7] = [1.5880, 0.5497, 0.2727, 0.5000, 0.0000, 0.5156, 1.2047];
/// Two that fail the gate, one that passes well, and one from training.
const NOTES: [u8; 4] = [64, 84, 96, 76];

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

fn main() -> Result<()> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: treble_diagnosis SOURCE_SAMPLES")?;
    let source = Path::new(&source);
    println!("error del candidato por armonico, dB. + = el modelo tiene de MAS.
");
    for note in NOTES {
        let cases = load(source, &[note])?;
        let (_, rows) = score_with(&cases, baseline(), true, |c, _, seconds| {
            render(c, candidate(c.note), seconds)
        })?;
        println!("--- nota {note} ---");
        println!(
            "{:<7}{:>10}{:>10}{:>10}{:>10}{:>10}{:>10}",
            "capa", "atq H2", "atq H3", "atq H4", "cpo H2", "cpo H3", "cpo H4"
        );
        for row in &rows {
            let layer = row["layer"].as_str().unwrap_or("?");
            let mut cells = vec![String::from("-"); 6];
            if let Some(terms) = row["balances"].as_array() {
                for term in terms {
                    let window = term["window"].as_u64().unwrap_or(0);
                    let harmonic = term["harmonic"].as_u64().unwrap_or(0);
                    let error = term["error_db"].as_f64().unwrap_or(f64::NAN);
                    let slot = (window as usize - 1) * 3 + (harmonic as usize - 2);
                    if slot < cells.len() {
                        cells[slot] = format!("{error:+.1}");
                    }
                }
            }
            print!("{layer:<7}");
            for cell in cells {
                print!("{cell:>10}");
            }
            println!();
        }
        println!();
    }
    Ok(())
}
