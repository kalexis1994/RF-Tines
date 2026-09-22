//! Does the tine ever come near the pole piece?
//!
//! The third candidate mechanism in docs/STRUCK-FRAME.md is a threshold at
//! the pickup: at a hard enough blow the tine swings far enough that the
//! pole is no longer far away. The model already knows both numbers, so this
//! costs nothing to ask, and it has to be asked before anything is built.
//!
//! A caution written before the run. Whatever the tine does near the pole it
//! does once per cycle, so it lands on the harmonics. The content this whole
//! search is chasing lands *between* them. If the clearance turns out to be
//! small, that is a real nonlinearity worth knowing about, but it is not the
//! one the attack noise needs.
//!
//! `cargo run --release -p rf-tines-lab --example pole_clearance`
#[allow(dead_code)]
#[path = "matts_fit.rs"]
mod previous;
use previous::{Result, baseline};
use rf_tines_dsp::{Engine, Profile};

const RATE: f64 = 48_000.0;

fn swing(profile: Profile, note: u8, velocity: f64) -> Result<f64> {
    let mut engine = Engine::new(RATE, profile)?;
    engine.set_gain(0.5);
    engine.set_level_compensation(true);
    engine.reset();
    engine.note_on(0, note, velocity);
    let mut peak = 0.0_f64;
    for _ in 0..(RATE * 0.3) as usize {
        engine.next_sample();
        peak = peak.max(engine.probe(note).map_or(0.0, |p| p.displacement_m.abs()));
    }
    Ok(peak)
}

fn main() -> Result<()> {
    println!("excursion de la punta contra el hueco del pickup\n");
    println!(
        "{:>6}{:>12}{:>12}{:>12}{:>12}",
        "nota", "hueco um", "piano um", "forte um", "forte/hueco"
    );
    for note in [30_u8, 38, 50, 52, 55, 59, 67, 76, 88] {
        let profile = baseline().for_note(note);
        let gap = profile.pickup_gap_m;
        let soft = swing(baseline(), note, 0.25)?;
        let loud = swing(baseline(), note, 0.85)?;
        println!(
            "{note:>6}{:>12.0}{:>12.1}{:>12.1}{:>12.3}",
            gap * 1e6,
            soft * 1e6,
            loud * 1e6,
            loud / gap
        );
    }
    println!("\nuna razon cerca de 1 significa que la punta llega al polo");
    Ok(())
}
