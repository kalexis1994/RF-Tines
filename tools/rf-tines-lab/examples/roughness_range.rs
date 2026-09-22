//! Does the roughness stay the size of a real attack, across the keyboard?
//!
//! The spectral statistic the fit minimises is a ratio between two spectral
//! sums and cannot see absolute level. It has now misled this work twice: the
//! force-cubed law reached five hundred times the clean attack in the bass
//! while scoring well, and the velocity law's best score puts a transient six
//! to fifteen times the body. The instrument does neither.
//!
//! So this measures what the fit cannot: the attack's peak over the body's
//! peak, which on the reference averages 1.33 across nine notes at forte and
//! never exceeds 2.27.
//!
//! `cargo run --release -p rf-tines-lab --example roughness_range`
//!
//! The roughness law this was named for was removed; it now drives
//! `frame_gain`, the resonance bank that replaced it.
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;
use previous::{Result, baseline};
use rf_tines_dsp::{Engine, Profile};

const RATE: f64 = 48_000.0;
/// Measured on the reference: mean 1.33, from 0.78 at note 30 to 2.27 at 88.
const REFERENCE_RATIO: f64 = 1.33;
const CEILING: f64 = 2.27;

fn ratio(profile: Profile, note: u8) -> Result<f64> {
    let mut engine = Engine::new(RATE, profile)?;
    engine.set_gain(0.5);
    engine.set_level_compensation(true);
    engine.reset();
    engine.note_on(0, note, 0.85);
    let signal: Vec<f64> = (0..(RATE * 0.5) as usize)
        .map(|_| f64::from(engine.next_sample()))
        .collect();
    let attack = signal[..(RATE * 0.02) as usize]
        .iter()
        .fold(0.0_f64, |m, v| m.max(v.abs()));
    let body = signal[(RATE * 0.1) as usize..]
        .iter()
        .fold(0.0_f64, |m, v| m.max(v.abs()));
    Ok(attack / body.max(1e-30))
}

fn main() -> Result<()> {
    println!("forte: pico del ataque sobre pico del cuerpo, por ganancia");
    println!("el Rhodes real promedia {REFERENCE_RATIO:.2}, y su peor nota llega a {CEILING:.2}\n");
    println!(
        "{:>9}{:>9}{:>9}{:>9}{:>9}{:>9}{:>8}",
        "ganancia", "n30", "n52", "n67", "n88", "peor", ""
    );
    let notes = [30_u8, 52, 67, 88];
    for step in 0..10 {
        let gain = if step == 0 {
            0.0
        } else {
            0.002 * 3.0_f64.powi(step - 1)
        };
        let profile = Profile {
            frame_gain: gain,
            ..baseline()
        };
        let mut worst = 0.0_f64;
        print!("{gain:>9.4}");
        for note in notes {
            let r = ratio(profile, note)?;
            worst = worst.max(r);
            print!("{r:>9.2}");
        }
        let verdict = if worst > CEILING { "  pasado" } else { "" };
        println!("{worst:>9.2}{verdict:>8}");
    }
    Ok(())
}
