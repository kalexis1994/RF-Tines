//! Does the second motion coordinate let a legal pickup geometry work?
//!
//! `docs/PICKUP-GEOMETRY-CEILING.md` states the prediction this example
//! exists to settle. Every pickup search this project has run bounded the gap
//! below at 0.5 mm and finished on that bound, which is a geometry the Rhodes
//! service manual permits only in the middle and upper ranges of pianos built
//! after March 1972. Scored on the training notes, that shipping voicing
//! reaches 181.86 dB^2; the best geometry the manual allows reaches 271.25,
//! because a one-coordinate tine cannot make the upper-partial content the
//! reference has without being run that close.
//!
//! The claim is that the missing content is the tine's second transverse
//! direction: rotated principal axes at slightly split frequencies open the
//! tip's path into an ellipse, and an ellipse crossing a two-dimensional field
//! makes harmonics a line cannot. If that is right, a search confined to the
//! manual's gaps should now reach 181.86 or better. If it is not, the second
//! coordinate was not the missing piece and the document is wrong.
//!
//! Training notes only; the reserved notes are never loaded here.
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Case, Result, baseline, load, render, score_with};
use rf_tines_dsp::Profile;
use std::path::Path;

/// Non-reserved notes across the whole range, as the geometry probe uses.
const TRAINING_NOTES: [u8; 9] = [30, 38, 50, 55, 59, 67, 76, 88, 98];
/// What the shipping voicing scores on these same cases, from the probe.
const SHIPPING: f64 = 181.86;

const NAMES: [&str; 12] = [
    "hueco grave mm",
    "hueco agudo mm",
    "despl mm",
    "despl transv mm",
    "angulo rad",
    "razon ejes",
    "martillo m/s",
    "exponente",
    "union tonebar",
    "razon tonebar",
    "cuna del polo",
    "radio polo mm",
];

/// The manual gives the bass a floor of 1/16 in and a ceiling of 1/8 in, and
/// lets the middle and upper ranges close to 0.020 in, so the gap is graded
/// across the keyboard rather than held fixed. The offset stays small because
/// the manual rests the tine slightly above dead centre, not a whole gap off
/// it. Everything here is inside what the factory permits.
const BOUNDS: [(f64, f64); 12] = [
    (1.588, 3.175),
    (0.508, 3.175),
    (0.05, 1.0),
    (-1.0, 1.0),
    (0.0, 0.8),
    (1.0, 1.15),
    (0.2, 1.6),
    (0.5, 3.0),
    // The second prong: how hard it is joined, and where it sits. The paper
    // measures the prongs several hundred to more than 1400 cents apart,
    // which is the range the ratio is held to.
    (0.0, 3.0),
    (1.19, 2.24),
    // How far the pole face is ground from a disc towards an edge, and how
    // wide it is. See docs/WEDGE-POLE.md.
    (0.0, 1.0),
    (0.5, 3.0),
];

/// Three starts, because one coarse descent is not evidence that a hypothesis
/// is dead. The first holds the manual's floor throughout, the second grades
/// the treble closed, the third opens the bass wide and turns the axes.
const STARTS: [[f64; 12]; 3] = [
    [1.588, 1.588, 0.25, 0.0, 0.3, 1.02, 0.8, 1.4, 0.0, 1.5, 1.0, 2.0],
    [1.588, 0.508, 0.5, 0.0, 0.3, 1.02, 0.8, 1.4, 0.0, 1.3, 0.6, 1.0],
    [3.175, 1.0, 0.25, 0.3, 0.5, 1.05, 1.0, 1.4, 0.0, 2.0, 1.0, 0.8],
];

/// Gap interpolated smoothly across the keyboard between the two ends.
fn register_gap_m(note: u8, bass_mm: f64, treble_mm: f64) -> f64 {
    let t = ((f64::from(note) - 28.0) / 72.0).clamp(0.0, 1.0);
    let w = t * t * (3.0 - 2.0 * t);
    (bass_mm * (1.0 - w) + treble_mm * w) * 1e-3
}

fn profile(x: [f64; 12], note: u8) -> Profile {
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

/// A trial the measurement cannot read is not a good trial. Far out in the
/// box a geometry can render so quietly that the onset detector finds nothing
/// to measure; that is a rejection, not a crash, so it scores as unusable and
/// the search walks on.
fn score(cases: &[Case], x: [f64; 12]) -> f64 {
    match score_with(cases, baseline(), false, |c, _, seconds| {
        render(c, profile(x, c.note), seconds)
    }) {
        Ok((mse, _)) => mse,
        Err(_) => f64::INFINITY,
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 {
        return Err("usage: two_plane_fit SOURCE_SAMPLES".into());
    }
    let cases = load(Path::new(&args[1]), &TRAINING_NOTES)?;
    println!("{} casos, notas {TRAINING_NOTES:?}", cases.len());
    println!("a batir: {SHIPPING:.2} dB^2, la geometria ilegal que sale en release\n");

    let mut best = STARTS[0];
    let mut best_score = f64::INFINITY;
    for (index, start) in STARTS.into_iter().enumerate() {
        let mut here = start;
        let mut here_score = score(&cases, here);
        println!("arranque {index}: {here_score:.2} dB^2");
        // Deterministic coordinate rounds with halving steps, as the earlier
        // searches used. No reserved note steers anything.
        for round in 0..4 {
            let fraction = 0.5_f64.powi(round);
            for dim in 0..12 {
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
                        println!("  ronda {round} {:<16} -> {loss:.2}", NAMES[dim]);
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

    println!("\nmejor {best_score:.2} dB^2");
    for (name, value) in NAMES.iter().zip(best) {
        println!("  {name:<16} {value:.4}");
    }
    let verdict = if best_score <= SHIPPING {
        "CUMPLE: una geometria legal alcanza a la ilegal"
    } else {
        "NO CUMPLE: sigue sin cerrar la brecha"
    };
    println!(
        "\n{verdict} ({:+.1}% contra {SHIPPING:.2})",
        (best_score / SHIPPING - 1.0) * 100.0
    );
    Ok(())
}
