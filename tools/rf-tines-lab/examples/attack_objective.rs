//! The frozen objective, extended to the attack. See docs/ATTACK-PROTOCOL.md.
//!
//! Three terms, reported separately and never summed. Term A is the existing
//! harmonic balance, untouched. Term B is the attack's colour in third-octave
//! bands over the first 15 ms. Term C is the size of the transient.
//!
//! This run is the protocol's own validation: it scores the three changes
//! whose outcome is already known, and the terms have to catch them.
//!
//! `cargo run --release -p rf-tines-lab --example attack_objective -- SAMPLES`
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Case, Result, baseline, load, render, score_with};
use rf_tines_dsp::Profile;
use std::{f64::consts::TAU, path::Path};

/// Training notes. The fresh reserve -- 33, 45, 57, 70, 81, 93 -- is not
/// loaded here and must not be.
const TRAINING: [u8; 9] = [30, 38, 50, 55, 59, 67, 76, 88, 98];
/// Third-octave centres from the attack's low end to the top of the band the
/// reference carries.
const BANDS: [f64; 17] = [
    250.0, 315.0, 400.0, 500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0,
    4000.0, 5000.0, 6300.0, 8000.0, 10000.0,
];
const FLOOR_DB: f64 = -90.0;

fn line(block: &[f64], rate: f64, frequency: f64) -> f64 {
    let n = block.len();
    if n < 32 {
        return 0.0;
    }
    let (mut re, mut im) = (0.0, 0.0);
    for (i, value) in block.iter().enumerate() {
        let w = 0.5 - 0.5 * (TAU * i as f64 / n as f64).cos();
        let phase = TAU * frequency * i as f64 / rate;
        re += value * w * phase.cos();
        im += value * w * phase.sin();
    }
    (re * re + im * im) / (n * n) as f64
}

fn band(block: &[f64], rate: f64, centre: f64) -> f64 {
    let mut total = 0.0;
    for step in 0..5 {
        let f = centre * 2.0_f64.powf((f64::from(step) / 4.0 - 0.5) / 3.0);
        if f * 2.0 < rate {
            total += line(block, rate, f);
        }
    }
    total
}

/// Term B and Term C for one signal, given its own onset.
fn attack_terms(samples: &[f64], rate: f64, note: u8) -> (Vec<f64>, f64) {
    let peak = samples.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    let onset = samples
        .iter()
        .position(|v| v.abs() > peak * 0.02)
        .unwrap_or(0);
    let attack = &samples[onset..(onset + (rate * 0.015) as usize).min(samples.len())];
    let body_from = (onset + (rate * 0.10) as usize).min(samples.len());
    let body_to = (onset + (rate * 0.40) as usize).min(samples.len());
    let body = &samples[body_from..body_to];
    let f0 = 440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0);
    // The body's fundamental region is the reference level, so the colour is
    // independent of how loud the render or the recording happens to be.
    let reference = (1..=3)
        .map(|h| line(body, rate, f0 * f64::from(h)))
        .sum::<f64>()
        .max(1e-30);
    let colour = BANDS
        .iter()
        .map(|&c| (10.0 * (band(attack, rate, c) / reference).max(1e-30).log10()).max(FLOOR_DB))
        .collect();
    let attack_peak = attack.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    let body_peak = body.iter().fold(1e-30_f64, |m, v| m.max(v.abs()));
    (colour, 20.0 * (attack_peak / body_peak).log10())
}

/// Terms B and C over every case, against the reference.
fn score_attack(cases: &[Case], profile: impl Fn(u8) -> Profile) -> Result<(f64, f64)> {
    let (mut colour_sum, mut transient_sum, mut n) = (0.0, 0.0, 0usize);
    for c in cases {
        let candidate = render(c, profile(c.note), 0.9)?;
        let rate = f64::from(candidate.metadata().sample_rate);
        let (ours, ours_t) = attack_terms(candidate.samples(), rate, c.note);
        let (theirs, theirs_t) = attack_terms(
            c.clip.samples(),
            f64::from(c.clip.metadata().sample_rate),
            c.note,
        );
        colour_sum += ours
            .iter()
            .zip(&theirs)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f64>()
            / ours.len() as f64;
        transient_sum += (ours_t - theirs_t) * (ours_t - theirs_t);
        n += 1;
    }
    let n = n.max(1) as f64;
    Ok((colour_sum / n, transient_sum / n))
}

fn score_balance(cases: &[Case], profile: Profile) -> Result<f64> {
    let (mse, _) = score_with(cases, profile, false, render)?;
    Ok(mse)
}

fn report(
    cases: &[Case],
    name: &str,
    profile: Profile,
    base: Option<(f64, f64, f64)>,
) -> Result<(f64, f64, f64)> {
    // A configuration that cannot be scored is not a configuration that
    // passes. Refusing to render or losing its own onset is a rejection, and
    // it is reported as one rather than stopping the run.
    let (a, b, c) = match (
        score_balance(cases, profile),
        score_attack(cases, |_| profile),
    ) {
        (Ok(a), Ok((b, c))) => (a, b, c),
        _ => {
            println!("{name:<34}   no puntua: el ataque destruye la nota");
            return Ok((f64::INFINITY, f64::INFINITY, f64::INFINITY));
        }
    };
    match base {
        None => println!("{name:<34}{a:>10.2}{b:>12.1}{c:>12.1}"),
        Some((a0, b0, c0)) => println!(
            "{name:<34}{a:>10.2}{:>+8.1}%{b:>10.1}{:>+8.1}%{c:>10.1}{:>+8.1}%",
            (a / a0 - 1.0) * 100.0,
            (b / b0 - 1.0) * 100.0,
            (c / c0 - 1.0) * 100.0
        ),
    }
    Ok((a, b, c))
}

fn main() -> Result<()> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: attack_objective SAMPLES")?;
    let cases = load(Path::new(&source), &TRAINING)?;
    println!(
        "{} casos, notas de entrenamiento {:?}",
        cases.len(),
        TRAINING
    );
    println!("la reserva nueva (33, 45, 57, 70, 81, 93) no se carga\n");

    println!(
        "{:<34}{:>10}{:>12}{:>12}",
        "", "A balance", "B color", "C transit"
    );
    let base = report(&cases, "lo que envia", baseline(), None)?;

    println!("\nVALIDACION: los tres casos cuyo resultado ya conocemos\n");
    println!(
        "{:<34}{:>10}{:>9}{:>10}{:>9}{:>10}{:>9}",
        "", "A", "cambio", "B", "cambio", "C", "cambio"
    );
    // 1. The contact as it was before it was softened.
    report(
        &cases,
        "contacto duro (como era, 4e10)",
        Profile {
            contact_stiffness: 4.0e10,
            ..baseline()
        },
        Some(base),
    )?;
    // 2. The roughness law that blew the transient apart.
    report(
        &cases,
        "banco a ganancia que revienta",
        Profile {
            frame_gain: 0.2,
            ..baseline()
        },
        Some(base),
    )?;
    // 3. The tonebar's clamp, which moves 250 Hz toward the reference.
    report(
        &cases,
        "sujecion del tonebar en 0.8",
        Profile {
            tonebar_clamp: 0.8,
            ..baseline()
        },
        Some(base),
    )?;
    println!(
        "
CONTRA EL OIDO: configuraciones que el oyente ya ordeno
"
    );
    println!(
        "{:<34}{:>10}{:>9}{:>10}{:>9}{:>10}{:>9}",
        "", "A", "cambio", "B", "cambio", "C", "cambio"
    );
    // A term built to decide attack questions is worth nothing if it does not
    // rank the way the ear it serves ranks.
    for hz in [170.0, 195.0, 210.0, 220.0, 235.0, 260.0, 300.0] {
        let name = format!("referencia {hz:.0} Hz");
        report(
            &cases,
            &name,
            Profile {
                tonebar_clamp: 0.6,
                tonebar_clamp_reference_hz: hz,
                tonebar_mass_ratio: 2.0,
                tonebar_frequency_ratio: 1.75,
                tonebar_decay_seconds: 0.30,
                ..baseline()
            },
            Some(base),
        )?;
    }
    println!(
        "
el oyente prefirio 1.75x/0.30, y antes 1.50x/0.80"
    );
    println!("\nse espera: 1) A mejora y B empeora   2) C empeora mucho   3) B mejora un poco");
    Ok(())
}
