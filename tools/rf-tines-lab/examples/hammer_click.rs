//! Where does the click in the first twelve milliseconds come from?
//!
//! A listener described "un golpecito muy corto" at the start of every note
//! that the real instrument does not have. Measured against the reference at
//! G3 forte, the model puts **+67 dB** more 5-10 kHz energy into the first
//! 12 ms than the real Rhodes does, and +40 dB more above 10 kHz.
//!
//! The suspect is the contact. Traced, the hammer is on the tine for 0.09 ms
//! at forte, which is one or two samples at the output rate: a force that
//! brief excites everything up to Nyquist. The piano literature puts hammer
//! contact at a third of a millisecond even in the treble.
//!
//! This was already swept once, in docs/PICKUP-GEOMETRY-CEILING.md, and
//! called inert across four decades of stiffness — but that was measured on
//! the attack-over-body envelope, which is deaf to a high band that lasts
//! ten milliseconds. Same parameter, right axis this time.
//!
//! `cargo run --release -p rf-tines-lab --example hammer_click`
use rf_tines_dsp::{Engine, PickupLaw, Profile, Voice};
use std::error::Error;

const RATE: f64 = 48_000.0;
const NOTE: u8 = 55;

fn render(stiffness: f64) -> Result<Vec<f64>, Box<dyn Error>> {
    let profile = Profile {
        pickup_law: PickupLaw::Aperture,
        pickup_gap_m: 0.0005,
        pickup_offset_m: 0.0005,
        contact_stiffness: stiffness,
        ..Profile::calibrated()
    };
    let mut engine = Engine::new(RATE, profile)?;
    engine.set_gain(0.5);
    engine.set_level_compensation(true);
    engine.reset();
    engine.note_on(0, NOTE, 0.85);
    Ok((0..(RATE * 1.5) as usize)
        .map(|_| f64::from(engine.next_sample()))
        .collect())
}

fn contact_microseconds(stiffness: f64) -> Result<f64, Box<dyn Error>> {
    let profile = Profile {
        contact_stiffness: stiffness,
        ..Profile::calibrated()
    };
    let mut voice = Voice::new(192_000.0, NOTE, profile)?;
    voice.strike(0.85);
    let mut samples = 0.0;
    for _ in 0..20_000 {
        voice.tick();
        if voice.probe().contact_active {
            samples += 1.0;
        } else if samples > 0.0 {
            break;
        }
    }
    Ok(samples / (192_000.0 * 4.0) * 1e6)
}

/// Energy in a band over a window, in dB.
fn band(signal: &[f64], from: usize, length: usize, low: f64, high: f64) -> f64 {
    let block = &signal[from.min(signal.len())..(from + length).min(signal.len())];
    if block.len() < 32 {
        return f64::NAN;
    }
    let mut total = 0.0;
    let bins = 64;
    for i in 0..bins {
        let frequency = low * (high / low).powf(i as f64 / (bins - 1) as f64);
        let (mut re, mut im) = (0.0, 0.0);
        for (n, value) in block.iter().enumerate() {
            let turn = n as f64 / block.len() as f64;
            let window = 0.5 - 0.5 * (std::f64::consts::TAU * turn).cos();
            let phase = std::f64::consts::TAU * frequency * n as f64 / RATE;
            re += value * window * phase.cos();
            im += value * window * phase.sin();
        }
        total += (re * re + im * im) / (block.len() * block.len()) as f64;
    }
    10.0 * total.max(1e-30).log10()
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("G3 fuerte. 'exceso' = los primeros 12 ms contra el cuerpo, en 5-10 kHz.");
    println!("El Rhodes real, medido igual, esta 67 dB por debajo del modelo actual.
");
    println!("{:<14}{:>12}{:>12}{:>12}", "rigidez", "contacto us", "exceso dB", "cuerpo dB");
    for stiffness in [1.0e8, 4.0e8, 1.6e9, 6.4e9, 4.0e10, 1.0e12] {
        let signal = render(stiffness)?;
        let onset = signal
            .iter()
            .position(|x| x.abs() > 1e-4)
            .unwrap_or(0);
        let knock = band(&signal, onset, (RATE * 0.012) as usize, 5000.0, 10_000.0);
        let body = band(
            &signal,
            onset + (RATE * 0.40) as usize,
            (RATE * 0.50) as usize,
            5000.0,
            10_000.0,
        );
        println!(
            "{stiffness:<14.0e}{:>12.0}{:>12.1}{body:>12.1}",
            contact_microseconds(stiffness)?,
            knock - body
        );
    }
    Ok(())
}
