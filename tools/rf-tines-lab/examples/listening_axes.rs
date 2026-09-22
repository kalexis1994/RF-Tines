//! The four things a listener heard that the objective cannot.
//!
//! Over one session a listener identified four distinct faults by ear. Every
//! one turned out to be real and measurable, and not one of them is visible
//! to the calibration objective, which scores the levels of the second,
//! third and fourth harmonics in a 96 ms attack window and a body window.
//!
//! | heard                         | axis                          |
//! |-------------------------------|-------------------------------|
//! | "the attack is softer on records" | upper partials over the fundamental |
//! | "a short tick the real one lacks" | 5-10 kHz in the first 12 ms |
//! | "a presence hit, more inflated"   | 0.7-3.5 kHz in the first 15 ms |
//! | "more space, not so compressed"   | how far the envelope wanders from a straight line |
//!
//! So this measures all four against the reference instrument, for whatever
//! profile it is given. It is not a test and asserts nothing: three of the
//! four are deficits the model still has, and a test can only pin what is
//! already true. It exists so the next candidate is judged on the axes a
//! listener actually used, instead of only on the one the objective knows.
//!
//! Matt's file names run an octave below concert pitch, so his G2 is the
//! note this renders as MIDI 55.
//!
//! `cargo run --release -p rf-tines-lab --example listening_axes -- SAMPLES`
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;

use previous::{Result, baseline};
use rf_tines_analysis::AudioClip;
use rf_tines_dsp::{Engine, Profile};
use std::{f64::consts::TAU, path::Path};

const RATE: f64 = 48_000.0;
/// note, Matt's name, layer, model velocity, fundamental.
const CASES: [(u8, &str, &str, f64, f64); 6] = [
    (40, "E1", "p", 0.30, 82.41),
    (40, "E1", "f", 0.85, 82.41),
    (55, "G2", "p", 0.30, 196.00),
    (55, "G2", "f", 0.85, 196.00),
    (76, "E3", "p", 0.30, 659.26),
    (76, "E3", "f", 0.85, 659.26),
];

fn render(profile: Profile, note: u8, velocity: f64) -> Result<Vec<f64>> {
    let mut engine = Engine::new(RATE, profile)?;
    engine.set_gain(0.5);
    engine.set_level_compensation(true);
    engine.reset();
    engine.note_on(0, note, velocity);
    Ok((0..(RATE * 3.0) as usize)
        .map(|_| f64::from(engine.next_sample()))
        .collect())
}

fn onset(x: &[f64], share: f64) -> usize {
    let peak = x.iter().fold(0.0_f64, |m, v| m.max(v.abs())).max(1e-30);
    x.iter().position(|v| v.abs() >= peak * share).unwrap_or(0)
}

/// Energy in a band over a window, in dB, by direct evaluation.
fn band(x: &[f64], from: usize, length: usize, low: f64, high: f64) -> f64 {
    let end = (from + length).min(x.len());
    if from >= end {
        return f64::NAN;
    }
    let block = &x[from..end];
    let mut total = 0.0;
    for i in 0..48 {
        let frequency = low * (high / low).powf(i as f64 / 47.0);
        let (mut re, mut im) = (0.0, 0.0);
        for (n, value) in block.iter().enumerate() {
            let turn = n as f64 / block.len() as f64;
            let window = 0.5 - 0.5 * (TAU * turn).cos();
            let phase = TAU * frequency * n as f64 / RATE;
            re += value * window * phase.cos();
            im += value * window * phase.sin();
        }
        total += (re * re + im * im) / (block.len() * block.len()) as f64;
    }
    10.0 * total.max(1e-30).log10()
}

/// Everything above the fundamental over the fundamental, in the attack.
fn edge(x: &[f64], f0: f64) -> f64 {
    let start = onset(x, 0.05);
    let length = (RATE * 0.096) as usize;
    let root = band(x, start, length, f0 * 0.94, f0 * 1.06);
    let rest = band(x, start, length, f0 * 1.5, (f0 * 12.0).min(20_000.0));
    rest - root
}

/// A band in the first milliseconds, against the sustained note.
fn burst(x: &[f64], low: f64, high: f64, milliseconds: f64) -> f64 {
    let start = onset(x, 0.02);
    let attack = band(x, start, (RATE * milliseconds / 1000.0) as usize, low, high);
    let body = band(
        x,
        start + (RATE * 0.40) as usize,
        (RATE * 0.50) as usize,
        low,
        high,
    );
    attack - body
}

/// How far the envelope wanders from a straight line in dB after the attack.
/// One exponential leaves nothing behind; partials beating against each
/// other do.
fn ripple(x: &[f64]) -> f64 {
    let start = onset(x, 0.05) + (RATE * 0.10) as usize;
    let width = (RATE * 0.02) as usize;
    let points: Vec<f64> = (0..100)
        .filter_map(|i| {
            let from = start + i * width;
            let end = (from + width).min(x.len());
            (end > from + width / 2).then(|| {
                let mean = x[from..end].iter().map(|v| v * v).sum::<f64>()
                    / (end - from) as f64;
                10.0 * mean.max(1e-30).log10()
            })
        })
        .collect();
    if points.len() < 10 {
        return f64::NAN;
    }
    let n = points.len() as f64;
    let mean_x = (n - 1.0) / 2.0;
    let mean_y = points.iter().sum::<f64>() / n;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (i, y) in points.iter().enumerate() {
        let dx = i as f64 - mean_x;
        sxy += dx * (y - mean_y);
        sxx += dx * dx;
    }
    let slope = sxy / sxx;
    (points
        .iter()
        .enumerate()
        .map(|(i, y)| {
            let fit = mean_y + slope * (i as f64 - mean_x);
            (y - fit) * (y - fit)
        })
        .sum::<f64>()
        / n)
        .sqrt()
}

fn main() -> Result<()> {
    let source = std::env::args()
        .nth(1)
        .ok_or("usage: listening_axes SOURCE_SAMPLES")?;
    let source = Path::new(&source);
    let profile = baseline();

    println!("cada fila: el Rhodes real, el modelo, y lo que falta o sobra.
");
    println!(
        "{:<9}{:<8}{:>9}{:>9}{:>9}{:>9}",
        "caso", "eje", "real", "modelo", "falta", ""
    );
    for (note, matt, layer, velocity, f0) in CASES {
        let reference = AudioClip::open(source.join(format!("{matt}-{layer}.wav")), Some(0))?;
        let real: Vec<f64> = reference.samples().to_vec();
        let model = render(profile, note, velocity)?;
        let axes: [(&str, f64, f64); 4] = [
            ("filo", edge(&real, f0), edge(&model, f0)),
            (
                "tick",
                burst(&real, 5000.0, 10_000.0, 12.0),
                burst(&model, 5000.0, 10_000.0, 12.0),
            ),
            (
                "presencia",
                burst(&real, 700.0, 3500.0, 15.0),
                burst(&model, 700.0, 3500.0, 15.0),
            ),
            ("respira", ripple(&real), ripple(&model)),
        ];
        for (name, real_value, model_value) in axes {
            println!(
                "{:<9}{name:<8}{real_value:>9.2}{model_value:>9.2}{:>9.2}",
                format!("{matt}-{layer}"),
                real_value - model_value
            );
        }
        println!();
    }
    println!("filo/tick/presencia en dB; respira es el residuo contra una recta.");
    println!("'falta' positivo = al modelo le falta lo que el real tiene.");
    Ok(())
}
