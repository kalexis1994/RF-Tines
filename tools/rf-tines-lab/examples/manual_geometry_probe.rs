//! Scores candidate pickup geometries against the project's own objective.
//!
//! Every search this project has run bounded the pickup gap below at 0.5 mm,
//! and every one of them finished sitting on that floor. The Rhodes service
//! manual specifies the pickup-to-tine gap between 1/16 in (1.588 mm) and
//! 1/8 in (3.175 mm), and allows 0.020 in (0.508 mm) only on pianos built
//! after March 1972 and only in the middle and upper ranges. The shipping
//! profile runs the whole keyboard at that extreme minimum, with the tine a
//! full gap off centre where the manual asks for it to rest slightly above
//! dead centre.
//!
//! At 0.5 mm the finite-aperture flux slope changes sign about 0.4 mm out,
//! inside the swing the bass notes reach, so their waveform inverts polarity
//! mid-cycle. At the manual's gaps the sign change is gone entirely. This
//! probe asks whether moving into the manual's box costs or buys anything on
//! the objective the project already scores with, before anything is fitted.
//!
//! Searches nothing and writes nothing. Training notes only; the reserved
//! notes are never loaded.
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Case, Result, baseline, load, render, score, score_with};
use rf_tines_dsp::Profile;
use std::path::Path;

/// Non-reserved notes spanning the model's whole range. The frozen protocol
/// trains on 50, 55 and 59, which is nine semitones; a register effect is
/// invisible inside that span, so this probe spreads out while leaving every
/// reserved note (36, 43, 64, 72, 84, 96) untouched.
const TRAINING_NOTES: [u8; 9] = [30, 38, 50, 55, 59, 67, 76, 88, 98];

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 {
        return Err("usage: manual_geometry_probe SOURCE_SAMPLES".into());
    }
    let cases = load(Path::new(&args[1]), &TRAINING_NOTES)?;
    println!("{} casos, notas {:?}\n", cases.len(), TRAINING_NOTES);

    let candidates: Vec<(String, Profile)> = [
        ("el de fabrica (0.5 / 0.5)", 0.0005, 0.0005),
        ("minimo del manual (1.588 / 0.1)", 0.001588, 0.0001),
        ("minimo del manual (1.588 / 0.25)", 0.001588, 0.00025),
        ("minimo del manual (1.588 / 0.5)", 0.001588, 0.0005),
        ("medio (2.4 / 0.25)", 0.0024, 0.00025),
        ("maximo del manual (3.175 / 0.25)", 0.003175, 0.00025),
        ("maximo del manual (3.175 / 0.5)", 0.003175, 0.0005),
    ]
    .into_iter()
    .map(|(name, gap, offset)| {
        (
            name.to_string(),
            Profile {
                pickup_gap_m: gap,
                pickup_offset_m: offset,
                ..baseline()
            },
        )
    })
    .collect();

    let mut reference = None;
    println!("{:<34}{:>14}{:>12}", "geometria fija", "MSE dB^2", "contra");
    for (name, profile) in candidates {
        let (mse, _) = score(&cases, profile, false)?;
        let base = *reference.get_or_insert(mse);
        println!("{name:<34}{mse:>14.2}{:>11.1}%", (mse / base - 1.0) * 100.0);
    }
    let fixed_baseline = reference.ok_or("no fixed geometry scored")?;

    // The manual does not give one gap; it gives a range whose floor depends
    // on the register. 1/16 in is the normal minimum and 0.020 in is allowed
    // only in the middle and upper ranges, so the bass is held wide and the
    // treble is free to close. That is a register law with the factory as its
    // source, which is what the earlier constant-gap probe could not express.
    println!("\n{:<34}{:>14}{:>12}", "hueco por registro", "MSE dB^2", "contra");
    for (bass_mm, treble_mm) in [
        (1.588, 1.588),
        (1.588, 1.0),
        (1.588, 0.508),
        (2.4, 0.508),
        (3.175, 0.508),
        (3.175, 1.0),
    ] {
        for offset_m in [0.00025, 0.0005] {
            let mse = score_register(&cases, bass_mm, treble_mm, offset_m)?;
            println!(
                "{:<34}{mse:>14.2}{:>11.1}%",
                format!("grave {bass_mm} agudo {treble_mm} despl {}", offset_m * 1e3),
                (mse / fixed_baseline - 1.0) * 100.0
            );
        }
    }
    Ok(())
}

/// Gap interpolated across the keyboard, smoothly, between the two ends.
fn register_gap_m(note: u8, bass_mm: f64, treble_mm: f64) -> f64 {
    let t = ((f64::from(note) - 28.0) / 72.0).clamp(0.0, 1.0);
    let w = t * t * (3.0 - 2.0 * t);
    (bass_mm * (1.0 - w) + treble_mm * w) * 1e-3
}

fn score_register(cases: &[Case], bass_mm: f64, treble_mm: f64, offset_m: f64) -> Result<f64> {
    let (mse, _) = score_with(cases, baseline(), false, |c, _, seconds| {
        render(
            c,
            Profile {
                pickup_gap_m: register_gap_m(c.note, bass_mm, treble_mm),
                pickup_offset_m: offset_m,
                ..baseline()
            },
            seconds,
        )
    })?;
    Ok(mse)
}
