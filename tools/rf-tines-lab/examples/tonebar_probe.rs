//! Why did joining the prongs not bend the decay?
//!
//! docs/PLAYABLE-TONEBAR.md predicted that a coupled second prong would stop
//! the fundamental decaying as one exponential. It did not: at the default
//! ratio the straight-line residual went from 0.077 to 0.090 dB, where the
//! prediction asked for a doubling.
//!
//! Two oscillators only trade energy freely when they are near resonance, and
//! the measured Rhodes fork is deliberately far from it -- several hundred to
//! more than 1400 cents apart. If that is the explanation, then a tonebar
//! tuned close to the tine should ripple the decay strongly and the real
//! spacing should not, whatever the coupling. This walks both axes and says
//! which.
//!
//! `cargo run --release -p rf-tines-lab --example tonebar_probe`
use rf_tines_dsp::{PickupLaw, Profile, Voice};
use std::error::Error;

const RATE: f64 = 48_000.0;
const NOTE: u8 = 55;

fn envelope_bend(profile: Profile) -> Result<(f64, f64), Box<dyn Error>> {
    let mut voice = Voice::new(RATE, NOTE, profile)?;
    voice.strike(0.7);
    let block = (RATE * 0.02) as usize * 4; // 20 ms of internal steps
    let mut points = Vec::new();
    for _ in 0..100 {
        let mut power = 0.0;
        for _ in 0..block {
            let value = voice.tick();
            power += value * value;
        }
        points.push(10.0 * (power / block as f64).max(1e-30).log10());
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
    let residual = (points
        .iter()
        .enumerate()
        .map(|(i, y)| {
            let fit = mean_y + slope * (i as f64 - mean_x);
            (y - fit) * (y - fit)
        })
        .sum::<f64>()
        / n)
        .sqrt();
    Ok((residual, slope * 50.0))
}

fn main() -> Result<(), Box<dyn Error>> {
    let base = Profile {
        pickup_law: PickupLaw::Aperture,
        pickup_gap_m: 0.0006,
        pickup_offset_m: 0.00045,
        ..Profile::calibrated()
    };
    println!("G3. 'curvatura' = dB RMS que deja una recta sobre la envolvente.");
    println!("Una exponencial sola deja cero. 'pendiente' = dB por segundo.\n");
    println!(
        "{:<10}{:>10}{:>12}{:>12}",
        "razon", "union", "curvatura", "pendiente"
    );
    for ratio in [1.005, 1.02, 1.08, 1.25, 1.5, 2.0] {
        for coupling in [0.0, 0.2, 0.8, 2.0] {
            let profile = Profile {
                tonebar_frequency_ratio: ratio,
                tonebar_coupling: coupling,
                ..base
            };
            let (bend, slope) = envelope_bend(profile)?;
            println!("{ratio:<10}{coupling:>10}{bend:>12.3}{slope:>12.2}");
        }
        println!();
    }
    Ok(())
}
