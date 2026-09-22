//! What does removing the hammer click cost on the objective?
//!
//! `hammer_click` found the contact stiffness controls a burst of 5-10 kHz
//! in the first twelve milliseconds that the real instrument does not have,
//! and that softening the contact by four hundred times removes 51 dB of it
//! while bringing the contact duration from 116 us to 889 us -- into the
//! range the piano-hammer literature reports instead of far below it.
//!
//! Before proposing that, the price. Training notes only.
//!
//! `cargo run --release -p rf-tines-lab --example click_cost -- SAMPLES`
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Result, baseline, load, render, score_with};
use rf_tines_dsp::Profile;
use std::path::Path;

const TRAINING: [u8; 9] = [30, 38, 50, 55, 59, 67, 76, 88, 98];

fn main() -> Result<()> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: click_cost SOURCE_SAMPLES")?;
    let cases = load(Path::new(&source), &TRAINING)?;
    println!("notas de entrenamiento. La rigidez que sale en release es 4e10.");
    println!("El control de dureza solo llega a 1.6e9 en su extremo blando.
");
    println!("{:<14}{:>12}{:>12}", "rigidez", "MSE", "contra 4e10");
    let mut reference = None;
    for stiffness in [1.0e8, 4.0e8, 1.6e9, 6.4e9, 4.0e10, 1.0e12] {
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
        if stiffness == 4.0e10 {
            reference = Some(mse);
        }
        println!("{stiffness:<14.0e}{mse:>12.2}{:>12}", "");
        let _ = reference;
    }
    // Printed again with the comparison now that the reference is known.
    let base = score_with(&cases, baseline(), false, |c, _, seconds| {
        render(c, baseline(), seconds)
    })?
    .0;
    println!("
referencia (4e10): {base:.2} dB^2");
    for stiffness in [1.0e8, 4.0e8, 1.6e9] {
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
        println!(
            "{stiffness:<14.0e}{mse:>12.2}  ({:+.1}%)",
            (mse / base - 1.0) * 100.0
        );
    }
    Ok(())
}
