//! Level compensation of continuous pickup voicing: does the reference-motion
//! RMS gain hold a medium note's level across gap, offset and law?
use rf_tines_dsp::{Engine, PickupLaw, Profile};
use serde_json::json;
use std::{error::Error, path::Path};

pub const HELP: &str = "Voicing level compensation:
  voicing-level --output REPORT.json
Renders G3 at velocities 0.25/0.6/1.0 and D3/B3 at 0.6 for both pickup laws over a
grid of six gaps and six offsets, each engine carrying its own level compensation, and
reports every note's level against the default pickup. Qualifies when the reference
note, G3 at 0.6, stays within 3 dB in every cell; other notes and dynamics are
reported as the law's own amplitude dependence. Writes one new JSON receipt.
";

const GAPS_MM: [f64; 6] = [0.5, 0.75, 1.0, 1.5, 2.0, 3.0];
const OFFSETS_MM: [f64; 6] = [0.0, 0.25, 0.5, 0.75, 1.0, 1.5];
const RATE: f64 = 44_100.0;
const SECONDS: f64 = 1.0;
const HOLD: f64 = 0.9;
const REFERENCE_LIMIT_DB: f64 = 3.0;

fn rms(profile: Profile, note: u8, velocity: f64) -> Result<f64, Box<dyn Error>> {
    let mut engine = Engine::new(RATE, profile)?;
    engine.set_level_compensation(true);
    engine.reset();
    let frames = (RATE * SECONDS) as usize;
    let release = (RATE * HOLD) as usize;
    let mut power = 0.0;
    engine.note_on(0, note, velocity);
    for frame in 0..frames {
        if frame == release {
            engine.note_off(0, note);
        }
        let sample = f64::from(engine.next_sample());
        if !sample.is_finite() || engine.faults() != 0 {
            return Err("voicing render encountered a numerical fault".into());
        }
        power += sample * sample;
    }
    Ok((power / frames as f64).sqrt())
}

fn db(ratio: f64) -> f64 {
    20.0 * ratio.log10()
}

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    if args.len() != 3 || args[1] != "--output" {
        return Err(HELP.into());
    }
    let output = Path::new(&args[2]);
    if output.extension().is_none_or(|x| x != "json") {
        return Err("output must be a new .json path".into());
    }
    if output.exists() {
        return Err(format!("refusing to overwrite {}", output.display()).into());
    }
    let cases: [(u8, f64); 5] = [(55, 0.25), (55, 0.6), (55, 1.0), (50, 0.6), (59, 0.6)];
    let default = Profile::default();
    let mut reference = Vec::new();
    for (note, velocity) in cases {
        reference.push(rms(default, note, velocity)?);
    }
    let mut cells = Vec::new();
    let mut worst_reference = 0.0_f64;
    let mut worst_medium = 0.0_f64;
    let mut worst_any = 0.0_f64;
    for law in [PickupLaw::Production, PickupLaw::Aperture] {
        for gap in GAPS_MM {
            for offset in OFFSETS_MM {
                let profile = Profile {
                    pickup_gap_m: gap * 1e-3,
                    pickup_offset_m: offset * 1e-3,
                    pickup_law: law,
                    ..default
                };
                profile.validate(RATE)?;
                let compensation = profile.level_compensation();
                let mut notes = Vec::new();
                for ((note, velocity), reference) in cases.iter().zip(&reference) {
                    let level = rms(profile, *note, *velocity)?;
                    let relative = db(level / reference);
                    if *velocity == 0.6 {
                        worst_medium = worst_medium.max(relative.abs());
                        if *note == 55 {
                            worst_reference = worst_reference.max(relative.abs());
                        }
                    }
                    worst_any = worst_any.max(relative.abs());
                    notes.push(json!({
                        "note": note, "velocity": velocity, "rms": level,
                        "raw_rms": level / compensation, "relative_db": relative,
                    }));
                }
                cells.push(json!({
                    "law": match law { PickupLaw::Production => "production", PickupLaw::Aperture => "aperture", PickupLaw::Reluctance => "reluctance", PickupLaw::RegisterAperture => "register-aperture" },
                    "gap_mm": gap, "offset_mm": offset,
                    "sensitivity": profile.pickup_sensitivity(), "level_compensation": compensation,
                    "notes": notes,
                }));
            }
        }
    }
    let passed = worst_reference <= REFERENCE_LIMIT_DB;
    let report = json!({
        "schema_version": 1, "experiment": "voicing-level-v1", "passed": passed,
        "protocol": {
            "sample_rate": RATE, "seconds": SECONDS, "hold_seconds": HOLD,
            "cases": cases.iter().map(|(n, v)| json!({"note": n, "velocity": v})).collect::<Vec<_>>(),
            "gaps_mm": GAPS_MM, "offsets_mm": OFFSETS_MM, "pole_radius_mm": default.pickup_pole_radius_m * 1e3,
            "reference": "default profile: production law, gap 1.5 mm, offset 0.5 mm; every cell's engine applies its own level_compensation, the ratio of reference-sine RMS sensitivities",
            "reference_motion": {"amplitude_m": rf_tines_dsp::LEVEL_REFERENCE_AMPLITUDE_M, "frequency_hz": rf_tines_dsp::LEVEL_REFERENCE_FREQUENCY_HZ},
            "reference_case": {"note": 55, "velocity": 0.6}, "reference_limit_db": REFERENCE_LIMIT_DB,
        },
        "reference_rms": reference,
        "worst_reference_relative_db": worst_reference, "worst_medium_relative_db": worst_medium,
        "worst_any_relative_db": worst_any,
        "cells": cells,
        "scope": "One-second isolated notes of the default mechanics; RMS over the whole render; no loudness model, no plugin change.",
    });
    crate::analysis::write_report(output, &report)?;
    println!(
        "Voicing level: worst reference {:.2} dB, worst medium {:.2} dB, worst any {:.2} dB, passed {}",
        worst_reference, worst_medium, worst_any, passed
    );
    if !passed {
        return Err("voicing level compensation failed qualification".into());
    }
    Ok(())
}
