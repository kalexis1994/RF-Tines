//! Can **any** pickup shape make the reference's harmonics?
//!
//! Two resonator hypotheses have now been predicted, built and measured, and
//! neither closed the gap: the second motion coordinate reached 226.77 and
//! the tonebar 226.34, against the 181.86 an illegal pickup geometry reaches
//! (docs/PICKUP-GEOMETRY-CEILING.md, docs/PLAYABLE-TONEBAR.md). Each cost a
//! build to find out.
//!
//! The next candidate is the pole's shape -- the sources describe a wedge
//! pointing at the tine and this model uses a disc. Before building a wedge,
//! this asks the question one level up: is there **any** shape that would do?
//!
//! The pickup has no back-action on the tine here, so the mechanics can be
//! rendered once and the transducer swept over it for nothing. A pickup of
//! any shape turns tip motion into `-dPhi/dt = -g(x) v`, where `g` is the
//! flux slope along the tine's path: one function of displacement, whatever
//! the pole looks like. So fitting `g` freely, as a piecewise-linear curve
//! with no physics imposed on it at all, bounds what every possible pole
//! geometry can reach.
//!
//! If the bound sits at or under 181.86, the pole's shape is worth building
//! and a wedge is the thing to try. If it sits where the resonator attempts
//! landed, no pole shape is the answer and the problem is somewhere neither
//! the resonator nor the transducer has been looked at.
//!
//! The curve is laid out over absolute displacement, shared by every case,
//! because that is what a fixed pole is. A first attempt normalised it to
//! each note's own swing, which quietly made it a different pickup per note
//! and excluded the real one; the run below starts from the shipping
//! geometry's own curve and has to reproduce its score before any freedom is
//! believed.
//!
//! `cargo run --release -p rf-tines-lab --example transducer_ceiling -- SAMPLES`
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Case, Result, baseline, load, score_with};
use rf_tines_analysis::AudioClip;
use rf_tines_dsp::{
    AxialAperture, ProductionDecimator, SpatialPickupProfile, Voice, aperture_voltage,
};
use std::path::Path;

const RATE: f64 = 48_000.0;
const OVERSAMPLE: usize = 4;
const SECONDS: f64 = 0.9;
const TRAINING_NOTES: [u8; 9] = [30, 38, 50, 55, 59, 67, 76, 88, 98];
const SHIPPING: f64 = 181.86;
const RESONATORS: f64 = 226.34;

/// Knots of the free flux slope, spread over the swing the whole keyboard
/// reaches. More would fit tighter and mean less; this is already far more
/// freedom than a pole face has, which is the point of an upper bound.
const KNOTS: usize = 15;
/// Half the span the knots cover, in metres. The bottom note reaches 1.83 mm.
const SPAN_M: f64 = 0.002;

/// The mechanics of one case, sampled at the internal rate, with no pickup.
struct Motion {
    displacement: Vec<f64>,
    velocity: Vec<f64>,
}

fn mechanics(note: u8, velocity: f64) -> Result<Motion> {
    // Exactly the shipping mechanics, so the reference point below is the
    // 181.86 the shipping geometry reaches and nothing else has moved.
    let mut voice = Voice::new(RATE, note, baseline())?;
    if !voice.strike(velocity) {
        return Err("strike rejected".into());
    }
    let steps = (RATE * SECONDS) as usize * OVERSAMPLE;
    let mut displacement = Vec::with_capacity(steps);
    let mut motion = Vec::with_capacity(steps);
    for _ in 0..steps {
        voice.tick();
        let probe = voice.probe();
        displacement.push(probe.displacement_m);
        motion.push(probe.velocity_m_s);
    }
    Ok(Motion {
        displacement,
        velocity: motion,
    })
}

/// Where a displacement falls among the knots.
fn place_of(x: f64) -> (usize, f64) {
    let place =
        ((x + SPAN_M) / (2.0 * SPAN_M) * (KNOTS - 1) as f64).clamp(0.0, (KNOTS - 1) as f64);
    let low = (place.floor() as usize).min(KNOTS - 2);
    (low, place - low as f64)
}

/// Voltage through a flux slope given as knots over `[-SPAN_M, SPAN_M]`,
/// decimated exactly as the engine decimates.
fn through(motion: &Motion, knots: &[f64; KNOTS]) -> Result<AudioClip> {
    let mut decimator = ProductionDecimator::new();
    let mut out = Vec::with_capacity(motion.displacement.len() / OVERSAMPLE);
    for (index, (x, v)) in motion
        .displacement
        .iter()
        .zip(&motion.velocity)
        .enumerate()
    {
        let (low, fraction) = place_of(*x);
        let slope = knots[low] + fraction * (knots[low + 1] - knots[low]);
        decimator.push(-slope * v);
        if index % OVERSAMPLE == OVERSAMPLE - 1 {
            out.push(decimator.output());
        }
    }
    Ok(AudioClip::from_samples(RATE as u32, out)?)
}

fn score(cases: &[Case], motions: &[Motion], knots: &[f64; KNOTS]) -> f64 {
    // `score_with` hands back each case in the order it was given, so the
    // pre-rendered motion is found by matching the case rather than by
    // counting calls, which a `Fn` closure could not do anyway.
    let result = score_with(cases, baseline(), false, |case, _, _| {
        let at = cases
            .iter()
            .position(|other| other.note == case.note && other.layer == case.layer)
            .ok_or("case not pre-rendered")?;
        through(&motions[at], knots)
    });
    match result {
        Ok((mse, _)) => mse,
        Err(_) => f64::INFINITY,
    }
}

fn main() -> Result<()> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: transducer_ceiling SOURCE_SAMPLES")?;
    let cases = load(Path::new(&source), &TRAINING_NOTES)?;
    println!("{} casos. Mecanica renderizada una vez; solo cambia el transductor.", cases.len());
    let motions: Vec<Motion> = cases
        .iter()
        .map(|c| mechanics(c.note, c.velocity))
        .collect::<Result<_>>()?;

    // Before believing anything the search finds, reproduce a point that is
    // already known. Sampling the shipping pickup's own flux slope at the
    // knots has to score what the shipping pickup scores; if it does not,
    // this parameterisation is not a superset of real pickups and nothing
    // below bounds anything.
    let aperture = AxialAperture::new(SpatialPickupProfile {
        gap_m: 0.0005,
        offset_xy_m: [0.0005, 0.0],
        pole_radius_m: 0.002,
        pole_wedge: 0.0,
        flux_scale_wb: 0.001,
    })?;
    let shipping_curve: [f64; KNOTS] = core::array::from_fn(|i| {
        let x = -SPAN_M + 2.0 * SPAN_M * i as f64 / (KNOTS - 1) as f64;
        -aperture_voltage(&aperture, x, 1.0)
    });
    let reproduced = score(&cases, &motions, &shipping_curve);
    println!("control: la curva del pickup de release da {reproduced:.2} dB^2,");
    println!(
        "         y ese pickup puntua {SHIPPING:.2}. Diferencia {:+.1}%.",
        (reproduced / SHIPPING - 1.0) * 100.0
    );
    if (reproduced / SHIPPING - 1.0).abs() > 0.15 {
        println!();
        println!("LA SONDA NO ES FIEL: no reproduce un punto conocido, asi que no");
        println!("acota nada. No se sigue.");
        return Ok(());
    }
    println!("         reproduce el punto conocido; se puede seguir.");
    println!();

    let mut best = shipping_curve;
    let mut best_score = reproduced;

    for round in 0..6 {
        let step = 0.8 * 0.6_f64.powi(round);
        let mut improved = false;
        for knot in 0..KNOTS {
            for sign in [-1.0, 1.0] {
                let mut trial = best;
                trial[knot] += sign * step;
                let loss = score(&cases, &motions, &trial);
                if loss < best_score {
                    best = trial;
                    best_score = loss;
                    improved = true;
                }
            }
        }
        println!("ronda {round}: {best_score:.2} dB^2");
        if !improved && round > 2 {
            break;
        }
    }

    println!("\ncurva libre hallada, pendiente de flujo por nodo:");
    for (index, value) in best.iter().enumerate() {
        let place = -1.0 + 2.0 * index as f64 / (KNOTS - 1) as f64;
        println!("  {place:>6.2} del recorrido  {value:>8.3}");
    }
    println!("\ncota del transductor : {best_score:.2} dB^2");
    println!("resonadores llegaron : {RESONATORS:.2}");
    println!("geometria ilegal     : {SHIPPING:.2}");
    if best_score <= SHIPPING {
        println!("\nALCANZA: alguna forma de polo basta, y vale construir la cuna.");
    } else {
        println!(
            "\nNO ALCANZA: ninguna forma de polo basta ({:+.1}% sobre {SHIPPING:.2}).",
            (best_score / SHIPPING - 1.0) * 100.0
        );
        println!("El problema no esta ni en el resonador ni en la forma del transductor.");
    }
    Ok(())
}
