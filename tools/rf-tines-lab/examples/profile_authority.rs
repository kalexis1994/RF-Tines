//! What could make two presets different instruments, and what does not.
//!
//! The eight instrument presets differ by contact stiffness, bar-partial
//! strike weight, pickup gap and offset, the velocity law and the panel
//! electronics. Everything else about them is the same instrument: the same
//! three modal ratios, the same decay times, the same masses, the same pole.
//! The tine itself -- which the sources name as one of the three biggest
//! causes of the era differences -- is byte-identical across all eight,
//! because `Settings` has no control that reaches it.
//!
//! This sweeps the profile fields the presets cannot touch, over ranges a
//! real instrument could plausibly cover, and reports what each is worth
//! against the controls that are already exposed. It renders the DSP
//! directly, with no level compensation and no electronics, so the number is
//! the model's own opinion.
//!
//! `cargo run --release -p rf-tines-lab --example profile_authority`
use rf_tines_dsp::{Engine, PickupLaw, Profile};
use std::error::Error;

const RATE: f64 = 48_000.0;
const NOTE: u8 = 55;
const LAYERS: [f64; 2] = [0.25, 0.85];

fn baseline() -> Profile {
    Profile {
        pickup_law: PickupLaw::Aperture,
        pickup_gap_m: 0.0006,
        pickup_offset_m: 0.00045,
        decay_seconds: 20.0,
        bar_partial_decay_seconds: 2.3,
        bar_partial_strike_weight: -0.3 * 0.26 * 0.26,
        ..Profile::default()
    }
}

fn render(profile: Profile, velocity: f64) -> Result<Vec<f64>, Box<dyn Error>> {
    let mut engine = Engine::new(RATE, profile)?;
    engine.set_gain(0.5);
    engine.reset();
    engine.note_on(0, NOTE, velocity);
    let samples: Vec<f64> = (0..(RATE * 1.5) as usize)
        .map(|_| f64::from(engine.next_sample()))
        .collect();
    if engine.faults() != 0 {
        return Err("engine fault".into());
    }
    Ok(samples)
}

/// A crude real-input DFT over a log-spaced set of probe frequencies, each
/// normalised to the loudest, so a level difference is not a timbre
/// difference. No FFT dependency and no windowing subtleties to argue about.
fn colour(samples: &[f64], start: f64, length: f64) -> Vec<f64> {
    let onset = samples
        .iter()
        .position(|x| x.abs() > samples.iter().fold(0.0_f64, |m, y| m.max(y.abs())) * 0.05)
        .unwrap_or(0);
    let begin = onset + (RATE * start) as usize;
    let end = (begin + (RATE * length) as usize).min(samples.len());
    let block = &samples[begin.min(samples.len())..end];
    let mut out = Vec::new();
    for step in 0..48 {
        // 80 Hz to 12 kHz, four probes per octave.
        let frequency = 80.0 * 2.0_f64.powf(step as f64 / 7.0);
        if frequency > 12_000.0 {
            break;
        }
        let (mut re, mut im) = (0.0, 0.0);
        for (i, value) in block.iter().enumerate() {
            let phase = std::f64::consts::TAU * frequency * i as f64 / RATE;
            re += value * phase.cos();
            im += value * phase.sin();
        }
        out.push((re * re + im * im).sqrt() / block.len().max(1) as f64);
    }
    let peak = out.iter().fold(0.0_f64, |m, x| m.max(*x)).max(1e-30);
    out.iter()
        .map(|x| 20.0 * (x / peak).max(1e-6).log10())
        .collect()
}

fn distance(a: Profile, b: Profile) -> Result<(f64, f64), Box<dyn Error>> {
    let mut attack = 0.0_f64;
    let mut body = 0.0_f64;
    for velocity in LAYERS {
        let (left, right) = (render(a, velocity)?, render(b, velocity)?);
        for (window, slot) in [((0.0, 0.096), &mut attack), ((0.4, 0.5), &mut body)] {
            let (x, y) = (
                colour(&left, window.0, window.1),
                colour(&right, window.0, window.1),
            );
            let rms = (x
                .iter()
                .zip(&y)
                .map(|(p, q)| (p - q) * (p - q))
                .sum::<f64>()
                / x.len() as f64)
                .sqrt();
            *slot = slot.max(rms);
        }
    }
    Ok((attack, body))
}

fn main() -> Result<(), Box<dyn Error>> {
    let base = baseline();
    let cases: Vec<(&str, Profile, Profile)> = vec![
        (
            "razon del parcial de barra",
            Profile {
                bar_partial_ratio: 5.6,
                ..base
            },
            Profile {
                bar_partial_ratio: 7.0,
                ..base
            },
        ),
        (
            "masa del martillo",
            Profile {
                hammer_mass_kg: 0.003,
                ..base
            },
            Profile {
                hammer_mass_kg: 0.006,
                ..base
            },
        ),
        (
            "masa modal de la pua",
            Profile {
                modal_mass_kg: 0.001,
                ..base
            },
            Profile {
                modal_mass_kg: 0.002,
                ..base
            },
        ),
        (
            "cola del fundamental",
            Profile {
                decay_seconds: 12.0,
                ..base
            },
            Profile {
                decay_seconds: 28.0,
                ..base
            },
        ),
        (
            "cola del parcial de barra",
            Profile {
                bar_partial_decay_seconds: 1.2,
                ..base
            },
            Profile {
                bar_partial_decay_seconds: 3.5,
                ..base
            },
        ),
        (
            "cola del tercer parcial",
            Profile {
                third_partial_decay_seconds: 0.03,
                ..base
            },
            Profile {
                third_partial_decay_seconds: 0.09,
                ..base
            },
        ),
        (
            "radio del polo",
            Profile {
                pickup_pole_radius_m: 0.001,
                ..base
            },
            Profile {
                pickup_pole_radius_m: 0.003,
                ..base
            },
        ),
        (
            "ejes de la pua (dos planos)",
            base,
            Profile {
                tine_boundary_angle_rad: 0.4,
                tine_transverse_frequency_ratio: 1.03,
                ..base
            },
        ),
        // For scale: two controls the presets already use, over the span the
        // eight of them actually cover.
        (
            "[ya expuesto] hueco del pickup",
            Profile {
                pickup_gap_m: 0.0006,
                ..base
            },
            Profile {
                pickup_gap_m: 0.00088,
                ..base
            },
        ),
        (
            "[ya expuesto] peso de golpe de barra",
            Profile {
                bar_partial_strike_weight: -0.3 * 0.15 * 0.15,
                ..base
            },
            Profile {
                bar_partial_strike_weight: -0.3 * 0.52 * 0.52,
                ..base
            },
        ),
        (
            "[ya expuesto] rigidez de contacto",
            Profile {
                contact_stiffness: 4.0e10 * 25.0_f64.powf(2.0 * 0.22 - 1.0),
                ..base
            },
            Profile {
                contact_stiffness: 4.0e10 * 25.0_f64.powf(2.0 * 0.66 - 1.0),
                ..base
            },
        ),
    ];

    println!("G3, sin compensacion de nivel ni electronica. dB RMS de color.\n");
    println!("{:<34}{:>10}{:>10}", "parametro", "ataque", "cuerpo");
    for (name, a, b) in cases {
        match distance(a, b) {
            Ok((attack, body)) => println!("{name:<34}{attack:>10.2}{body:>10.2}"),
            Err(error) => println!("{name:<34}{:>20}", format!("({error})")),
        }
    }
    println!("\nlos marcados [ya expuesto] son el patron de comparacion: es lo que");
    println!("los ocho presets ya hacen. Lo de arriba es lo que hoy no pueden tocar.");
    Ok(())
}
