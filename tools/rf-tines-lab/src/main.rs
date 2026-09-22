//! Offline laboratory. All rendering, WAV writing and analysis runs in Rust.
mod action;
mod analysis;
mod assembly_check;
mod audition;
mod band_envelope;
mod band_events;
mod component_envelope;
mod convergence;
mod felt_damper;
mod hammer_memory;
mod mechanical_loss;
mod memory_contact_resolution;
mod memory_free_check;
mod memory_hammer_check;
mod memory_modal_check;
mod memory_modal_timing;
mod midi_render;
mod modal_assembly_check;
mod modal_families;
mod modal_observation;
mod modal_timing;
mod package;
mod partial_comparison;
mod pickup_convergence;
mod pickup_decay;
mod pickup_harmonics;
mod pickup_listening;
mod pickup_mixing;
mod pickup_pair;
mod pickup_set;
mod pickup_sweep;
mod pickup_transfer;
mod pitch_reference;
mod polarization;
mod register_families;
mod short_envelope;
mod source_envelope;
mod tine_modes;
mod tone_comparison;
mod transduction;
mod voicing_level;
mod wav;
mod web_ui;
use rf_tines_dsp::{Engine, FIRST_NOTE, LAST_NOTE, PICKUP_NAMES, PickupLaw, Profile};
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::Instant,
};

const HELP: &str = "RF-Tines research laboratory 0.1.2
Usage:
  rf-tines-lab render --output PATH.wav [options]
  rf-tines-lab demo --output PATH.wav
  rf-tines-lab stress [--sample-rate HZ] [--laboratory]
  rf-tines-lab inspect PATH.wav
  rf-tines-lab package
  rf-tines-lab build-ui
  rf-tines-lab audition [--prepare-only]
Render options:
  --note N          MIDI 28..100 (default 57 / A3)
  --velocity V      Greater than 0, up to 1 (default 0.7)
  --sample-rate HZ  44100, 48000, 96000 or 192000 (default 48000)
  --seconds S       Duration 0.05..60 (default 4)
  --hold S          Key hold, less than duration (default 2)
  --gap-mm X        Pickup gap 0.5..5 mm (default 1.5)
  --offset-mm X     Pickup offset -3..3 mm (default 0.5)
  --trace           Export output-rate physical probes as CSV
  --compensate      Apply the profile's pickup level compensation (raw otherwise)
  --pickup I        Render through laboratory pickup path I (0..3) instead of the
                    raw engine; needs the default gap/offset. Names: rf-tines-lab help
  --sustain S       original (default) or calibrated: first/bar partial T60 from
                    the retained recordings, 20 s / 2.3 s at A3
  --bar-ratio R     Second partial over the fundamental, 2..12 (default 6.267;
                    the recordings show 6.0)
  --contact-stiffness K  Quadratic contact law coefficient, 1e8..1e12 N/m^2
                    (default 4e10)
  --bar-strike W    Second partial strike weight, -1..1 (default -0.3)
  --law L           Pickup law: production (default) or aperture
  --pole-radius-mm R  Aperture pole radius 0.2..3 mm (default 2)
  --velocity-exponent E  Hammer speed follows velocity^E, 0.5..3 (default 1.4)
WAV is mono IEEE float, without normalization or clipping. Existing files are
never overwritten. Every render writes a JSON report. Parameters are uncalibrated.
Demo: three A3 intensities and a sustained E-minor chord, ten seconds.
Stress: 73 keys, 128-frame blocks, three seconds. No audio device is opened.
--laboratory measures the four-path pickup engine with frequent interrupted fades.
Laboratory pickup paths: 0 Current, 1 Close Original, 2 Close Point Pole, 3 Close Aperture.
";

#[derive(Debug)]
struct Options {
    command: String,
    output: Option<PathBuf>,
    note: u8,
    velocity: f64,
    rate: u32,
    seconds: f64,
    hold: f64,
    gap_mm: f64,
    offset_mm: f64,
    trace: bool,
    compensate: bool,
    laboratory: bool,
    pickup: Option<usize>,
    calibrated_sustain: bool,
    bar_ratio: Option<f64>,
    contact_stiffness: Option<f64>,
    bar_strike: Option<f64>,
    aperture_law: bool,
    pole_radius_mm: Option<f64>,
    velocity_exponent: Option<f64>,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        let Some(command) = args.first() else {
            return Err(HELP.into());
        };
        if !matches!(command.as_str(), "render" | "demo" | "stress") {
            return Err(format!("unknown command: {command}"));
        }
        let mut o = Self {
            command: command.clone(),
            output: None,
            note: 57,
            velocity: 0.7,
            rate: 48_000,
            seconds: 4.0,
            hold: 2.0,
            gap_mm: 1.5,
            offset_mm: 0.5,
            trace: false,
            compensate: false,
            laboratory: false,
            pickup: None,
            calibrated_sustain: false,
            bar_ratio: None,
            contact_stiffness: None,
            bar_strike: None,
            aperture_law: false,
            pole_radius_mm: None,
            velocity_exponent: None,
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut i = 1;
        while i < args.len() {
            let flag = args[i].as_str();
            if !seen.insert(flag) {
                return Err(format!("duplicate option: {flag}"));
            }
            if flag == "--laboratory" {
                if command != "stress" {
                    return Err("--laboratory is a stress option".into());
                }
                o.laboratory = true;
                i += 1;
                continue;
            }
            if flag == "--trace" {
                if command != "render" {
                    return Err("--trace is a render option".into());
                }
                o.trace = true;
                i += 1;
                continue;
            }
            if flag == "--compensate" {
                if command != "render" {
                    return Err("--compensate is a render option".into());
                }
                o.compensate = true;
                i += 1;
                continue;
            }
            let allowed = match command.as_str() {
                "stress" => flag == "--sample-rate",
                "demo" => matches!(flag, "--sample-rate" | "--output"),
                _ => matches!(
                    flag,
                    "--output"
                        | "--note"
                        | "--velocity"
                        | "--sample-rate"
                        | "--seconds"
                        | "--hold"
                        | "--gap-mm"
                        | "--offset-mm"
                        | "--pickup"
                        | "--sustain"
                        | "--bar-ratio"
                        | "--contact-stiffness"
                        | "--bar-strike"
                        | "--law"
                        | "--pole-radius-mm"
                        | "--velocity-exponent"
                ),
            };
            if !allowed {
                return Err(format!("unknown or inapplicable option: {flag}"));
            }
            let value = args
                .get(i + 1)
                .ok_or_else(|| format!("missing value for {flag}"))?;
            let invalid = || format!("invalid value for {flag}: {value}");
            match flag {
                "--output" => o.output = Some(value.into()),
                "--note" => o.note = value.parse().map_err(|_| invalid())?,
                "--velocity" => o.velocity = value.parse().map_err(|_| invalid())?,
                "--sample-rate" => o.rate = value.parse().map_err(|_| invalid())?,
                "--seconds" => o.seconds = value.parse().map_err(|_| invalid())?,
                "--hold" => o.hold = value.parse().map_err(|_| invalid())?,
                "--gap-mm" => o.gap_mm = value.parse().map_err(|_| invalid())?,
                "--offset-mm" => o.offset_mm = value.parse().map_err(|_| invalid())?,
                "--pickup" => o.pickup = Some(value.parse().map_err(|_| invalid())?),
                "--bar-ratio" => o.bar_ratio = Some(value.parse().map_err(|_| invalid())?),
                "--bar-strike" => o.bar_strike = Some(value.parse().map_err(|_| invalid())?),
                "--law" => {
                    o.aperture_law = match value.as_str() {
                        "production" => false,
                        "aperture" => true,
                        _ => return Err(invalid()),
                    }
                }
                "--pole-radius-mm" => {
                    o.pole_radius_mm = Some(value.parse().map_err(|_| invalid())?)
                }
                "--velocity-exponent" => {
                    o.velocity_exponent = Some(value.parse().map_err(|_| invalid())?)
                }
                "--contact-stiffness" => {
                    o.contact_stiffness = Some(value.parse().map_err(|_| invalid())?)
                }
                "--sustain" => {
                    o.calibrated_sustain = match value.as_str() {
                        "original" => false,
                        "calibrated" => true,
                        _ => return Err(invalid()),
                    }
                }
                _ => unreachable!(),
            }
            i += 2;
        }
        if ![44_100, 48_000, 96_000, 192_000].contains(&o.rate) {
            return Err("unsupported sample rate".into());
        }
        if command != "stress"
            && o.output
                .as_ref()
                .is_none_or(|p| p.extension().is_none_or(|e| e != "wav"))
        {
            return Err("--output must name a .wav file".into());
        }
        if !(FIRST_NOTE..=LAST_NOTE).contains(&o.note)
            || !o.velocity.is_finite()
            || o.velocity <= 0.0
            || o.velocity > 1.0
            || !o.seconds.is_finite()
            || !(0.05..=60.0).contains(&o.seconds)
            || !o.hold.is_finite()
            || o.hold < 0.0
            || o.hold >= o.seconds
        {
            return Err("note, velocity, duration or hold is outside its allowed range".into());
        }
        if let Some(index) = o.pickup {
            if command != "render" || index >= PICKUP_NAMES.len() {
                return Err("--pickup is a render option with a path index 0..3".into());
            }
            if o.gap_mm != 1.5 || o.offset_mm != 0.5 {
                return Err(
                    "--pickup renders the laboratory engine at the default geometry".into(),
                );
            }
        }
        o.profile()
            .validate(o.rate as f64)
            .map_err(|e| e.to_string())?;
        Ok(o)
    }
    fn profile(&self) -> Profile {
        let base = if self.calibrated_sustain {
            Profile::calibrated_sustain()
        } else {
            Profile::default()
        };
        Profile {
            pickup_gap_m: self.gap_mm * 0.001,
            pickup_offset_m: self.offset_mm * 0.001,
            bar_partial_ratio: self.bar_ratio.unwrap_or(base.bar_partial_ratio),
            contact_stiffness: self.contact_stiffness.unwrap_or(base.contact_stiffness),
            bar_partial_strike_weight: self.bar_strike.unwrap_or(base.bar_partial_strike_weight),
            pickup_law: if self.aperture_law {
                PickupLaw::Aperture
            } else {
                PickupLaw::Production
            },
            pickup_pole_radius_m: self
                .pole_radius_mm
                .map_or(base.pickup_pole_radius_m, |r| r * 0.001),
            velocity_exponent: self.velocity_exponent.unwrap_or(base.velocity_exponent),
            ..base
        }
    }
}

/// The laboratory builds whole engines as values on the stack.
///
/// An `Engine` holds all 73 voices inline, which is about 175 KB, and
/// constructing one moves that array at least twice; the four-pickup
/// laboratory path builds more on top. The plugin never notices because it
/// keeps its engine in a `Box`, but this tool does not, and the main thread's
/// default stack is not generous enough on every platform. It overflowed the
/// day the tine gained its tonebar and each voice grew by a third, which the
/// CLI tests caught as five renders failing at once.
///
/// Giving the work its own thread with room to stand is the fix that does not
/// change how the DSP allocates, which matters because the same code runs
/// under a realtime callback where an allocation is not free.
const STACK_BYTES: usize = 64 * 1024 * 1024;

fn main() {
    let worker = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(|| {
            if let Err(error) = run() {
                eprintln!("error: {error}");
                std::process::exit(1);
            }
        })
        .expect("spawn the laboratory thread");
    if worker.join().is_err() {
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() || matches!(args[0].as_str(), "--help" | "-h") {
        print!("{HELP}");
        print!("{}", analysis::HELP);
        print!("{}", midi_render::HELP);
        print!("{}", pickup_harmonics::HELP);
        print!("{}", voicing_level::HELP);
        print!("{}", component_envelope::HELP);
        print!("{}", source_envelope::HELP);
        print!("{}", band_envelope::HELP);
        print!("{}", band_events::HELP);
        print!("{}", mechanical_loss::HELP);
        print!("{}", mechanical_loss::reduced::HELP);
        print!("{}", mechanical_loss::observer::HELP);
        print!("{}", mechanical_loss::observer::loss::HELP);
        print!("{}", mechanical_loss::observer::loss::geometry::HELP);
        print!("{}", mechanical_loss::observer::loss::continuity::HELP);
        print!("{}", mechanical_loss::observer::loss::magnetic::HELP);
        print!(
            "{}",
            mechanical_loss::observer::loss::magnetic::nonlinear::HELP
        );
        print!(
            "{}",
            mechanical_loss::observer::loss::magnetic::nonlinear::loss_profile::HELP
        );
        print!(
            "{}",
            mechanical_loss::observer::loss::magnetic::nonlinear::loss_profile::noise::HELP
        );
        print!(
            "{}",
            mechanical_loss::observer::loss::magnetic::nonlinear::robustness::HELP
        );
        print!(
            "{}",
            mechanical_loss::observer::loss::magnetic::nonlinear::robustness::combined::HELP
        );
        print!(
            "{}",
            mechanical_loss::observer::loss::magnetic::nonlinear::loss_profile::resolution::HELP
        );
        print!(
            "{}",
            mechanical_loss::observer::loss::magnetic::nonlinear::loss_profile::weighting::HELP
        );
        print!("{}", mechanical_loss::observer::loss::magnetic::nonlinear::loss_profile::resolution::weighted::HELP);
        print!("{}", felt_damper::HELP);
        print!("{}", action::HELP);
        print!("{}", polarization::HELP);
        print!("{}", transduction::HELP);
        print!("{}", transduction::tuning::HELP);
        print!("{}", transduction::bank::HELP);
        print!("{}", source_envelope::short::HELP);
        print!("{}", short_envelope::HELP);
        print!("{}", partial_comparison::HELP);
        print!("{}", pickup_sweep::HELP);
        print!("{}", pickup_set::HELP);
        print!("{}", pickup_pair::HELP);
        print!("{}", pickup_listening::HELP);
        print!("{}", pickup_convergence::HELP);
        print!("{}", tone_comparison::HELP);
        print!("{}", modal_observation::HELP);
        print!("{}", modal_families::HELP);
        print!("{}", register_families::HELP);
        print!("{}", pickup_transfer::HELP);
        print!("{}", pickup_mixing::HELP);
        print!("{}", pickup_decay::HELP);
        print!("{}", convergence::HELP);
        print!("{}", assembly_check::HELP);
        print!("{}", tine_modes::HELP);
        print!("{}", modal_assembly_check::HELP);
        print!("{}", modal_assembly_check::HAMMER_HELP);
        print!("{}", hammer_memory::HELP);
        print!("{}", memory_hammer_check::HELP);
        print!("{}", memory_free_check::HELP);
        print!("{}", memory_contact_resolution::HELP);
        print!("{}", memory_modal_check::HELP);
        print!("{}", memory_modal_check::REFINEMENT_HELP);
        print!("{}", memory_modal_check::TAIL_HELP);
        print!("{}", memory_modal_check::CHECKPOINT_HELP);
        print!("{}", memory_modal_check::APPROACH_HELP);
        print!("{}", memory_modal_check::RECOVERY_HELP);
        print!("{}", memory_modal_check::AUDIO_HELP);
        print!("{}", memory_modal_check::HAMMER_COMPARISON_HELP);
        print!("{}", pitch_reference::HELP);
        print!("{}", memory_modal_check::TUNING_HELP);
        print!("{}", memory_modal_check::GEOMETRY_HELP);
        print!("{}", memory_modal_check::SPAN_HELP);
        print!("{}", memory_modal_check::TAPER_HELP);
        print!("{}", memory_modal_check::TRANSITION_HELP);
        print!("{}", memory_modal_check::TAIL_TIMING_HELP);
        print!("{}", memory_modal_timing::HELP);
        print!("{}", modal_timing::HELP);
        return Ok(());
    }
    if args[0] == "observe-families" {
        return modal_families::run(&args);
    }
    if args[0] == "observe-register" {
        return register_families::run(&args);
    }
    if args[0] == "pickup-mixing" {
        return pickup_mixing::run(&args);
    }
    if args[0] == "pickup-decay" {
        return pickup_decay::run(&args);
    }
    if matches!(args[0].as_str(), "component-envelope" | "validate-envelope") {
        return component_envelope::run(&args);
    }
    if args[0] == "observe-source-envelopes" {
        return source_envelope::run(&args);
    }
    if args[0] == "validate-band-envelope" {
        return band_envelope::run(&args);
    }
    if args[0] == "study-band-events" {
        return band_events::run(&args);
    }
    if args[0] == "mechanical-loss" {
        return mechanical_loss::run(&args);
    }
    if args[0] == "reduced-mechanical-loss" {
        return mechanical_loss::reduced::run(&args);
    }
    if args[0] == "pickup-state" {
        return mechanical_loss::observer::run(&args);
    }
    if args[0] == "pickup-loss" {
        return mechanical_loss::observer::loss::run(&args);
    }
    if args[0] == "pickup-loss-geometry" {
        return mechanical_loss::observer::loss::geometry::run(&args);
    }
    if args[0] == "pickup-loss-continuity" {
        return mechanical_loss::observer::loss::continuity::run(&args);
    }
    if args[0] == "magnetic-pickup-loss" {
        return mechanical_loss::observer::loss::magnetic::run(&args);
    }
    if args[0] == "magnetic-state" {
        return mechanical_loss::observer::loss::magnetic::nonlinear::run(&args);
    }
    if args[0] == "magnetic-loss-profile" {
        return mechanical_loss::observer::loss::magnetic::nonlinear::loss_profile::run(&args);
    }
    if args[0] == "magnetic-loss-noise" {
        return mechanical_loss::observer::loss::magnetic::nonlinear::loss_profile::noise::run(
            &args,
        );
    }
    if args[0] == "magnetic-state-robustness" {
        return mechanical_loss::observer::loss::magnetic::nonlinear::robustness::run(&args);
    }
    if args[0] == "magnetic-weighted-loss-resolution" {
        return mechanical_loss::observer::loss::magnetic::nonlinear::loss_profile::resolution::weighted::run(&args);
    }
    if args[0] == "render-midi" {
        return midi_render::run(&args);
    }
    if args[0] == "voicing-level" {
        return voicing_level::run(&args);
    }
    if args[0] == "pickup-harmonics" {
        return pickup_harmonics::run(&args);
    }
    if args[0] == "felt-damper" {
        return felt_damper::run(&args);
    }
    if args[0] == "action-cycle" {
        return action::run(&args);
    }
    if args[0] == "polarized-action" {
        return polarization::run(&args);
    }
    if args[0] == "electromechanical" {
        return transduction::run(&args);
    }
    if args[0] == "stationary-rest" {
        return transduction::rest::run(&args);
    }
    if args[0] == "loaded-loss-budget" {
        return transduction::losses::run(&args);
    }
    if args[0] == "calibrate-loaded-loss" {
        return transduction::calibration::run(&args);
    }
    if args[0] == "loaded-voicing" {
        return transduction::voicing::run(&args);
    }
    if args[0] == "loaded-dynamics" {
        return transduction::dynamics::run(&args);
    }
    if args[0] == "loaded-strike-threshold" {
        return transduction::threshold::run(&args);
    }
    if args[0] == "loaded-bridle" {
        return transduction::bridle::run(&args);
    }
    if args[0] == "loaded-hammer-return" {
        return transduction::return_motion::run(&args);
    }
    if args[0] == "loaded-repetition" {
        return transduction::repetition::run(&args);
    }
    if args[0] == "loaded-launch" {
        return transduction::launch::run(&args);
    }
    if args[0] == "loaded-drive-release" {
        return transduction::drive::run(&args);
    }
    if args[0] == "loaded-drive-onset" {
        return transduction::onset::run(&args);
    }
    if args[0] == "loaded-key-inertia" {
        return transduction::key::run(&args);
    }
    if args[0] == "loaded-letoff" {
        return transduction::letoff::run(&args);
    }
    if args[0] == "loaded-flight-budget" {
        return transduction::flight::run(&args);
    }
    if args[0] == "loaded-gravity" {
        return transduction::gravity::run(&args);
    }
    if args[0] == "loaded-landing" {
        return transduction::landing::run(&args);
    }
    if args[0] == "loaded-damper-seating" {
        return transduction::damper::run(&args);
    }
    if args[0] == "loaded-damper-lift" {
        return transduction::lift::run(&args);
    }
    if args[0] == "electromechanical-render" {
        return transduction::render(&args);
    }
    if args[0] == "tune-electromechanical" {
        return transduction::tuning::run(&args);
    }
    if args[0] == "compare-loaded-bank" {
        return transduction::bank::run(&args);
    }
    if args[0] == "magnetic-loss-weighting" {
        return mechanical_loss::observer::loss::magnetic::nonlinear::loss_profile::weighting::run(
            &args,
        );
    }
    if args[0] == "magnetic-loss-resolution" {
        return mechanical_loss::observer::loss::magnetic::nonlinear::loss_profile::resolution::run(
            &args,
        );
    }
    if args[0] == "magnetic-state-combined" {
        return mechanical_loss::observer::loss::magnetic::nonlinear::robustness::combined::run(
            &args,
        );
    }
    if args[0] == "observe-short-source-envelopes" {
        return source_envelope::short::run(&args);
    }
    if matches!(
        args[0].as_str(),
        "short-envelope" | "validate-short-envelope"
    ) {
        return short_envelope::run(&args);
    }
    if args[0] == "inspect" {
        if args.len() != 2 {
            return Err("inspect needs exactly one WAV path".into());
        }
        println!("{}", wav::inspect(File::open(&args[1])?)?);
        return Ok(());
    }
    if args[0] == "render-memory-modal" {
        return memory_modal_check::render_audio(&args);
    }
    if args[0] == "compare-modal-hammers" {
        return memory_modal_check::compare_hammers(&args);
    }
    if args[0] == "prepare-pitch-reference" {
        return pitch_reference::run(&args);
    }
    if args[0] == "tune-modal-pitch" {
        return memory_modal_check::tune_pitch(&args);
    }
    if args[0] == "sweep-tuned-geometry"
        || args[0] == "sweep-spring-span"
        || args[0] == "sweep-tine-taper"
        || args[0] == "sweep-tine-transition"
    {
        return memory_modal_check::sweep_geometry(&args);
    }
    if args[0] == "assembly-check" {
        return assembly_check::run(&args);
    }
    if args[0] == "assembly-refinement" {
        return assembly_check::run_refinement(&args);
    }
    if args[0] == "tine-modes" {
        return tine_modes::run(&args);
    }
    if args[0] == "modal-assembly-check" {
        return modal_assembly_check::run(&args);
    }
    if args[0] == "modal-hammer-check" {
        return modal_assembly_check::run_hammer(&args);
    }
    if args[0] == "hammer-memory-check" {
        return hammer_memory::run(&args);
    }
    if args[0] == "memory-hammer-check" {
        return memory_hammer_check::run(&args);
    }
    if args[0] == "memory-modal-checkpoint-check" {
        return memory_modal_check::run_checkpoint(&args);
    }
    if args[0] == "memory-modal-approach-check" {
        return memory_modal_check::run_approach(&args);
    }
    if args[0] == "memory-modal-recovery-check" {
        return memory_modal_check::run_recovery(&args);
    }
    if matches!(
        args[0].as_str(),
        "memory-free-check" | "memory-contact-check"
    ) {
        return memory_free_check::run(&args);
    }
    if args[0] == "memory-contact-resolution" {
        return memory_contact_resolution::run(&args);
    }
    if matches!(
        args[0].as_str(),
        "memory-modal-check"
            | "memory-modal-free-check"
            | "memory-modal-adaptive-check"
            | "memory-modal-economical-check"
            | "memory-modal-rk4-check"
    ) {
        return memory_modal_check::run(&args);
    }
    if args[0] == "memory-modal-rk4-refinement" {
        return memory_modal_check::run_refinement(&args);
    }
    if matches!(
        args[0].as_str(),
        "memory-modal-tail-check" | "memory-modal-reimpact-check"
    ) {
        return memory_modal_check::run_tail(&args);
    }
    if matches!(
        args[0].as_str(),
        "memory-modal-tail-timing"
            | "memory-modal-stiffness-timing"
            | "memory-modal-trial-reuse-timing"
            | "memory-modal-damping-timing"
    ) {
        return memory_modal_check::run_tail_timing(&args);
    }
    if args[0] == "memory-modal-timing" {
        return memory_modal_timing::run(&args);
    }
    if matches!(
        args[0].as_str(),
        "memory-modal-free-timing"
            | "memory-modal-adaptive-timing"
            | "memory-modal-economical-timing"
            | "memory-modal-rk4-timing"
    ) {
        return memory_modal_timing::run_free(&args);
    }
    if args[0] == "modal-timing" {
        return modal_timing::run(&args);
    }
    if args[0] == "compare-partials" {
        return partial_comparison::run(&args);
    }
    if args[0] == "compare-tone" {
        return tone_comparison::run(&args);
    }
    if args[0] == "observe-modes" {
        return modal_observation::run(&args);
    }
    if args[0] == "pickup-transfer" {
        return pickup_transfer::run(&args);
    }
    if args[0] == "sweep-pickup" {
        return pickup_sweep::run(&args);
    }
    if args[0] == "fit-pickup-set" {
        return pickup_set::run(&args);
    }
    if args[0] == "render-pickup-pair" {
        return pickup_pair::run(&args);
    }
    if args[0] == "pickup-listening" {
        return pickup_listening::run(&args);
    }
    if args[0] == "converge-pickup" {
        return pickup_convergence::run(&args);
    }
    if matches!(args[0].as_str(), "analyze" | "compare") {
        return analysis::run(&args);
    }
    if args[0] == "converge" {
        return convergence::run(&args);
    }
    if args[0] == "build-ui" {
        if args.len() != 1 {
            return Err("build-ui takes no arguments".into());
        }
        return web_ui::build();
    }
    if args[0] == "package" {
        if args.len() != 1 {
            return Err("package takes no arguments".into());
        }
        return package::build();
    }
    if args[0] == "audition" {
        return audition::run(&args[1..]);
    }
    let options = Options::parse(&args)?;
    if options.command == "stress" {
        stress(&options)
    } else {
        render(&options)
    }
}

fn new_file(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new().write(true).create_new(true).open(path)
}

fn render(o: &Options) -> Result<(), Box<dyn Error>> {
    let output = o.output.as_ref().expect("validated output");
    let report_path = output.with_extension("json");
    let trace_path = output.with_extension("csv");
    for path in [
        Some(output),
        Some(&report_path),
        o.trace.then_some(&trace_path),
    ]
    .into_iter()
    .flatten()
    {
        if path.exists() {
            return Err(format!("refusing to overwrite {}", path.display()).into());
        }
    }
    let demo = o.command == "demo";
    let seconds = if demo { 10.0 } else { o.seconds };
    let frames = (seconds * o.rate as f64).round() as u32;
    let mut engine = match o.pickup {
        Some(index) => {
            let mut engine = Engine::new_laboratory_with(o.rate as f64, o.profile())?;
            if !engine.set_pickup(index) {
                return Err("invalid laboratory pickup path".into());
            }
            engine.reset();
            engine
        }
        None => Engine::new(o.rate as f64, o.profile())?,
    };
    if o.compensate {
        engine.set_level_compensation(true);
        engine.reset();
    }
    let mut audio = wav::FloatWav::new(BufWriter::new(new_file(output)?), o.rate, frames)?;
    let mut trace = if o.trace {
        Some(BufWriter::new(new_file(&trace_path)?))
    } else {
        None
    };
    if let Some(csv) = &mut trace {
        writeln!(
            csv,
            "time_s,displacement_m,transverse_displacement_m,velocity_m_s,contact_force_n,mechanical_energy_j,pickup_signal,contact_active,output"
        )?;
    }
    let mut peak = 0.0_f64;
    let mut square_sum = 0.0;
    let started = Instant::now();
    for frame in 0..frames {
        if demo {
            let second = o.rate;
            for (at, velocity) in [(0, 0.2), (2 * second, 0.55), (4 * second, 1.0)] {
                if frame == at {
                    engine.note_on(0, 57, velocity);
                }
                if frame == at + second {
                    engine.note_off(0, 57);
                }
            }
            if frame == 6 * second {
                engine.control_change(0, 64, 1.0);
                for note in [40, 47, 55, 59, 64, 66] {
                    engine.note_on(0, note, 0.65);
                }
            }
            if frame == 7 * second {
                engine.control_change(0, 123, 0.0);
            }
            if frame == 9 * second {
                engine.control_change(0, 64, 0.0);
            }
        } else {
            if frame == 0 {
                engine.note_on(0, o.note, o.velocity);
            }
            if frame == (o.hold * o.rate as f64).round() as u32 {
                engine.note_off(0, o.note);
            }
        }
        let sample = engine.next_sample();
        if !sample.is_finite() || engine.faults() != 0 {
            return Err("render encountered a numerical fault".into());
        }
        peak = peak.max(sample.abs() as f64);
        square_sum += (sample as f64).powi(2);
        audio.sample(sample)?;
        if let Some(csv) = &mut trace {
            let p = engine.probe(o.note).expect("validated note");
            writeln!(
                csv,
                "{:.9},{:.12e},{:.12e},{:.12e},{:.12e},{:.12e},{:.12e},{},{:.9e}",
                (frame + 1) as f64 / o.rate as f64,
                p.displacement_m,
                p.transverse_displacement_m,
                p.velocity_m_s,
                p.contact_force_n,
                p.mechanical_energy_j,
                p.pickup_signal,
                u8::from(p.contact_active),
                sample
            )?;
        }
    }
    audio.finish()?;
    if let Some(csv) = &mut trace {
        csv.flush()?;
    }
    let elapsed = started.elapsed().as_secs_f64();
    let report = format!(
        "{{\n  \"schema_version\": 1,\n  \"model\": \"research-0.1.1-uncalibrated\",\n  \"mode\": \"{}\",\n  \"sample_rate\": {},\n  \"frames\": {},\n  \"note\": {},\n  \"velocity\": {},\n  \"hold_seconds\": {},\n  \"pickup_gap_mm\": {},\n  \"pickup_offset_mm\": {},\n  \"pickup_path\": {},\n  \"pickup_name\": {},\n  \"sustain\": \"{}\",\n  \"bar_partial_ratio\": {},\n  \"contact_stiffness\": {:e},\n  \"bar_partial_strike_weight\": {},\n  \"pickup_law\": \"{}\",\n  \"pickup_pole_radius_mm\": {},\n  \"velocity_exponent\": {},\n  \"level_compensation\": {:.9},\n  \"level_compensated\": {},\n  \"oversampling\": 4,\n  \"peak\": {:.9},\n  \"rms\": {:.9},\n  \"faults\": {},\n  \"render_wall_seconds_including_io\": {:.6}\n}}\n",
        o.command,
        o.rate,
        frames,
        o.note,
        o.velocity,
        o.hold,
        o.gap_mm,
        o.offset_mm,
        o.pickup.map_or("null".to_string(), |i| i.to_string()),
        o.pickup
            .map_or("null".to_string(), |i| format!("\"{}\"", PICKUP_NAMES[i])),
        if o.calibrated_sustain {
            "calibrated"
        } else {
            "original"
        },
        o.profile().bar_partial_ratio,
        o.profile().contact_stiffness,
        o.profile().bar_partial_strike_weight,
        if o.aperture_law {
            "aperture"
        } else {
            "production"
        },
        o.profile().pickup_pole_radius_m * 1e3,
        o.profile().velocity_exponent,
        o.profile().level_compensation(),
        o.compensate,
        peak,
        (square_sum / frames as f64).sqrt(),
        engine.faults(),
        elapsed
    );
    new_file(&report_path)?.write_all(report.as_bytes())?;
    println!("Wrote {}\n{}", output.display(), report);
    if peak > 1.0 {
        eprintln!(
            "Float output exceeds full scale; lower monitor gain. WAV samples were not clipped."
        );
    }
    Ok(())
}

fn stress(o: &Options) -> Result<(), Box<dyn Error>> {
    let mut engine = if o.laboratory {
        Engine::new_laboratory(o.rate as f64)?
    } else {
        Engine::new(o.rate as f64, o.profile())?
    };
    if o.laboratory {
        engine.set_gain(0.1);
        engine.reset();
    }
    engine.note_on(0, 57, 0.5);
    for _ in 0..2048 {
        std::hint::black_box(engine.next_sample());
    }
    engine.reset();
    let mut times = Vec::new();
    let blocks = (3 * o.rate as usize).div_ceil(128);
    let mut peak = 0.0_f32;
    for block in 0..blocks {
        let start = Instant::now();
        if o.laboratory && block % 3 == 0 {
            engine.set_pickup((block / 3) % PICKUP_NAMES.len());
        }
        if block % 188 == 0 {
            engine.control_change(0, 64, 1.0);
            for note in FIRST_NOTE..=LAST_NOTE {
                engine.note_on(0, note, 1.0);
            }
        }
        for _ in 0..128 {
            peak = peak.max(std::hint::black_box(engine.next_sample()).abs());
        }
        times.push(start.elapsed().as_secs_f64());
    }
    times.sort_by(f64::total_cmp);
    let deadline = 128.0 / o.rate as f64;
    let worst = times.last().copied().unwrap_or(0.0);
    let p99 = times[(times.len() * 99 / 100).min(times.len() - 1)];
    let misses = times.iter().filter(|t| **t > deadline).count();
    println!(
        "{{\"sample_rate\":{},\"keys\":73,\"block_frames\":128,\"blocks\":{},\"worst_ms\":{:.6},\"p99_ms\":{:.6},\"deadline_ms\":{:.6},\"deadline_misses\":{},\"peak\":{:.6},\"faults\":{},\"laboratory\":{}}}",
        o.rate,
        blocks,
        worst * 1000.0,
        p99 * 1000.0,
        deadline * 1000.0,
        misses,
        peak,
        engine.faults(),
        o.laboratory
    );
    if engine.faults() != 0 {
        return Err("numerical fault during stress run".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }
    #[test]
    fn cli_rejects_invalid_and_ambiguous_input() {
        assert!(parse(&["render", "--output", "x.wav"]).is_ok());
        for args in [
            vec!["render", "--output", "x.wav", "--velocity", "NaN"],
            vec!["render", "--output", "x.wav", "--seconds", "1"],
            vec![
                "render", "--output", "x.wav", "--note", "57", "--note", "60",
            ],
            vec!["render", "--output", "x.csv"],
            vec!["demo", "--output", "x.wav", "--hold", "1"],
            vec!["render", "--output", "x.wav", "--unknown", "1"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
}
