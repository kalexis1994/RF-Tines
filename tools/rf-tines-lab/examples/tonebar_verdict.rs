//! Does the tonebar help the fit, held at the best point the search found?
//!
//! The ten-coordinate search drove the coupling to zero, but a coordinate
//! descent that lands somewhere worse than an earlier eight-coordinate one is
//! weak evidence on its own: the trajectory changed, not only the model. So
//! this pins every other coordinate at the best point the previous search
//! reached -- 226.77 dB^2, the two-plane result in
//! docs/PICKUP-GEOMETRY-CEILING.md -- and moves the tonebar alone.
//!
//! If the second prong supplies content the pickup is currently standing in
//! for, some joining of it must score better than none. If nothing does, it
//! does not, whatever the search trajectory was.
//!
//! `cargo run --release -p rf-tines-lab --example tonebar_verdict -- SAMPLES`
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Case, Result, baseline, load, render, score_with};
use rf_tines_dsp::Profile;
use std::path::Path;

const TRAINING_NOTES: [u8; 9] = [30, 38, 50, 55, 59, 67, 76, 88, 98];
/// The best legal geometry found before the tonebar existed.
const BEST_WITHOUT: [f64; 8] = [1.588, 0.508, 0.5, 0.125, 0.55, 1.0341, 0.8438, 1.3219];
const REACHED: f64 = 226.77;
const SHIPPING: f64 = 181.86;

fn register_gap_m(note: u8, bass_mm: f64, treble_mm: f64) -> f64 {
    let t = ((f64::from(note) - 28.0) / 72.0).clamp(0.0, 1.0);
    let w = t * t * (3.0 - 2.0 * t);
    (bass_mm * (1.0 - w) + treble_mm * w) * 1e-3
}

fn profile(note: u8, coupling: f64, ratio: f64, decay: f64) -> Profile {
    let x = BEST_WITHOUT;
    Profile {
        pickup_gap_m: register_gap_m(note, x[0], x[1]),
        pickup_offset_m: x[2] * 1e-3,
        pickup_transverse_offset_m: x[3] * 1e-3,
        tine_boundary_angle_rad: x[4],
        tine_transverse_frequency_ratio: x[5],
        maximum_hammer_speed_m_s: x[6],
        velocity_exponent: x[7],
        tonebar_coupling: coupling,
        tonebar_frequency_ratio: ratio,
        tonebar_decay_seconds: decay,
        ..baseline()
    }
}

fn score(cases: &[Case], coupling: f64, ratio: f64, decay: f64) -> f64 {
    match score_with(cases, baseline(), false, |c, _, seconds| {
        render(c, profile(c.note, coupling, ratio, decay), seconds)
    }) {
        Ok((mse, _)) => mse,
        Err(_) => f64::INFINITY,
    }
}

fn main() -> Result<()> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: tonebar_verdict SOURCE_SAMPLES")?;
    let cases = load(Path::new(&source), &TRAINING_NOTES)?;
    println!(
        "{} casos. Todo fijo en el mejor punto legal previo.",
        cases.len()
    );
    println!("sin tonebar alcanzaba {REACHED:.2}; la geometria ilegal {SHIPPING:.2}\n");

    let none = score(&cases, 0.0, 1.5, 4.0);
    println!("union 0 (apagado): {none:.2} dB^2\n");
    println!(
        "{:<9}{:<9}{:>12}{:>12}",
        "union", "razon", "MSE", "contra 0"
    );
    let mut best = (none, 0.0, 0.0, 0.0);
    for ratio in [1.19, 1.4, 1.7, 2.0, 2.24] {
        for coupling in [0.1, 0.3, 0.8, 1.6, 3.0] {
            // A prong that rings as long as the tine, and one that dies fast.
            for decay in [4.0, 16.0] {
                let mse = score(&cases, coupling, ratio, decay);
                if mse < best.0 {
                    best = (mse, coupling, ratio, decay);
                }
                if decay == 4.0 {
                    println!(
                        "{coupling:<9}{ratio:<9}{mse:>12.2}{:>11.1}%",
                        (mse / none - 1.0) * 100.0
                    );
                }
            }
        }
    }
    println!();
    if best.1 == 0.0 {
        println!("VEREDICTO: nada mejora sobre apagarlo. El tonebar asi construido");
        println!("no aporta lo que el pickup esta supliendo.");
    } else {
        println!(
            "mejor: union {:.2}, razon {:.2}, cola {:.0} s -> {:.2} dB^2 ({:+.1}% contra apagado)",
            best.1,
            best.2,
            best.3,
            best.0,
            (best.0 / none - 1.0) * 100.0
        );
    }
    Ok(())
}
