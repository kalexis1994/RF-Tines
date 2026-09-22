//! Harmonic generation of the candidate pickup transfer laws under prescribed
//! sinusoidal tine motion, fitted to the harmonic balance measured on the
//! retained G3 recordings. No mechanics, circuit or audio is involved.
use rf_tines_dsp::{SpatialPickup, SpatialPickupProfile};
use serde_json::{Value, json};
use std::{error::Error, f64::consts::TAU, path::Path};

pub const HELP: &str = "Pickup harmonic generation:
  pickup-harmonics --output REPORT.json
Drives the production, point-pole and finite-aperture flux laws with a 196 Hz sinusoidal
tine displacement over a geometry grid, measures harmonic ratios of the induced voltage,
and fits a shared geometry with free amplitudes to the loud, medium and soft harmonic
balances retained in references/g3-playable-diagnostic. Writes one JSON receipt.
";

const SAMPLES: usize = 4096;
const HARMONICS: usize = 8;
const GAPS_MM: [f64; 6] = [0.5, 0.75, 1.0, 1.5, 2.0, 3.0];
const OFFSETS_MM: [f64; 6] = [0.0, 0.25, 0.5, 0.75, 1.0, 1.5];
const RADII_MM: [f64; 4] = [0.25, 0.5, 1.0, 2.0];
const AMPLITUDE_RANGE_MM: [f64; 2] = [0.02, 3.0];
const AMPLITUDE_STEPS: usize = 61;
/// Engine tine amplitude at the pickup 50-100 ms after a G3 strike, from the
/// playable engine's traces at velocities 1.0, 0.6 and 0.25 (mm).
const ENGINE_AMPLITUDES_MM: [f64; 3] = [0.8066, 0.3971, 0.1166];
/// Reference harmonic balance relative to the fundamental, 96 ms attack window,
/// jRhodes G3 layers 1, 3 and 5 (references/g3-playable-diagnostic tone reports).
/// The sixth harmonic is excluded: at G3 it coincides with the tine's second
/// bending mode and is not a pickup product.
const TARGETS: [(&str, &[(usize, f64)]); 3] = [
    ("loud", &[(2, 2.4), (3, 7.0), (4, -9.2), (5, 1.8)]),
    ("medium", &[(2, -17.4), (3, -13.6), (4, -17.3), (5, -24.6)]),
    ("soft", &[(2, -21.7), (3, -41.6)]),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Law {
    Production,
    PointPole,
    FiniteAperture,
}
impl Law {
    fn name(self) -> &'static str {
        match self {
            Law::Production => "production_inverse_sqrt",
            Law::PointPole => "point_pole_inverse_cube",
            Law::FiniteAperture => "finite_aperture_16_node",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub law: Law,
    pub gap_m: f64,
    pub offset_m: f64,
    pub pole_radius_m: f64,
}
/// Flux linkage as a function of lateral tine displacement, arbitrary scale.
pub fn flux(g: Geometry, x: f64) -> Result<f64, Box<dyn Error>> {
    let z = (g.offset_m + x) / g.gap_m;
    Ok(match g.law {
        Law::Production => (1.0 + z * z).powf(-0.5),
        Law::PointPole => (1.0 + z * z).powf(-1.5),
        Law::FiniteAperture => SpatialPickup::new(SpatialPickupProfile {
            gap_m: g.gap_m,
            offset_xy_m: [g.offset_m, 0.0],
            pole_radius_m: g.pole_radius_m,
            pole_wedge: 0.0,
            flux_scale_wb: 0.001,
        })?
        .flux_wb([x, 0.0])?,
    })
}
/// Harmonic amplitudes of the induced voltage for x(t) = A sin(wt): the
/// derivative multiplies the n-th flux harmonic by n*w, so ratios need only the
/// flux harmonics. Returns |V_n| for n = 1..=HARMONICS in arbitrary units.
pub fn voltage_harmonics(
    g: Geometry,
    amplitude_m: f64,
) -> Result<[f64; HARMONICS], Box<dyn Error>> {
    let mut phi = Vec::with_capacity(SAMPLES);
    for k in 0..SAMPLES {
        let theta = TAU * k as f64 / SAMPLES as f64;
        phi.push(flux(g, amplitude_m * theta.sin())?);
    }
    let mut out = [0.0; HARMONICS];
    for (n, slot) in out.iter_mut().enumerate() {
        let order = (n + 1) as f64;
        let (mut re, mut im) = (0.0, 0.0);
        for (k, value) in phi.iter().enumerate() {
            let theta = order * TAU * k as f64 / SAMPLES as f64;
            re += value * theta.cos();
            im -= value * theta.sin();
        }
        let magnitude = 2.0 * (re * re + im * im).sqrt() / SAMPLES as f64;
        *slot = order * magnitude;
    }
    Ok(out)
}
fn ratios_db(h: &[f64; HARMONICS]) -> Vec<f64> {
    (1..HARMONICS)
        .map(|n| 20.0 * (h[n].max(1e-300) / h[0].max(1e-300)).log10())
        .collect()
}
fn amplitude_grid() -> Vec<f64> {
    (0..AMPLITUDE_STEPS)
        .map(|i| {
            let t = i as f64 / (AMPLITUDE_STEPS - 1) as f64;
            1e-3 * AMPLITUDE_RANGE_MM[0] * (AMPLITUDE_RANGE_MM[1] / AMPLITUDE_RANGE_MM[0]).powf(t)
        })
        .collect()
}
fn error_db(ratios: &[f64], targets: &[(usize, f64)]) -> f64 {
    (targets
        .iter()
        .map(|(n, target)| (ratios[n - 2] - target).powi(2))
        .sum::<f64>()
        / targets.len() as f64)
        .sqrt()
}
struct Fit {
    amplitude_m: f64,
    error_db: f64,
    ratios: Vec<f64>,
}
fn fit_dynamic(g: Geometry, targets: &[(usize, f64)], grid: &[f64]) -> Result<Fit, Box<dyn Error>> {
    let mut best: Option<Fit> = None;
    for &a in grid {
        let ratios = ratios_db(&voltage_harmonics(g, a)?);
        let e = error_db(&ratios, targets);
        if best.as_ref().is_none_or(|b| e < b.error_db) {
            best = Some(Fit {
                amplitude_m: a,
                error_db: e,
                ratios,
            });
        }
    }
    let coarse = best.unwrap();
    // Refine around the coarse minimum with a finer geometric sweep.
    let mut refined = coarse;
    let lo = refined.amplitude_m / 1.1;
    let hi = refined.amplitude_m * 1.1;
    for i in 0..41 {
        let a = lo * (hi / lo).powf(i as f64 / 40.0);
        let ratios = ratios_db(&voltage_harmonics(g, a)?);
        let e = error_db(&ratios, targets);
        if e < refined.error_db {
            refined = Fit {
                amplitude_m: a,
                error_db: e,
                ratios,
            };
        }
    }
    Ok(refined)
}
fn geometries() -> Vec<Geometry> {
    let mut out = Vec::new();
    for law in [Law::Production, Law::PointPole, Law::FiniteAperture] {
        for gap in GAPS_MM {
            for offset in OFFSETS_MM {
                let radii: &[f64] = if law == Law::FiniteAperture {
                    &RADII_MM
                } else {
                    &[0.0]
                };
                for &radius in radii {
                    out.push(Geometry {
                        law,
                        gap_m: gap * 1e-3,
                        offset_m: offset * 1e-3,
                        pole_radius_m: radius * 1e-3,
                    });
                }
            }
        }
    }
    out
}
pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    if args.len() != 3 || args[1] != "--output" {
        return Err(HELP.into());
    }
    let output = Path::new(&args[2]);
    if output.exists() || output.extension().is_none_or(|x| x != "json") {
        return Err("pickup harmonics study requires a new JSON path".into());
    }
    let grid = amplitude_grid();
    let mut rows = Vec::new();
    let mut best_per_law: Vec<Value> = Vec::new();
    for law in [Law::Production, Law::PointPole, Law::FiniteAperture] {
        let mut best: Option<(f64, Value)> = None;
        for g in geometries().into_iter().filter(|g| g.law == law) {
            let mut dynamics = Vec::new();
            let mut total = 0.0;
            let mut amplitudes = Vec::new();
            for (name, targets) in TARGETS {
                let fit = fit_dynamic(g, targets, &grid)?;
                total += fit.error_db * fit.error_db;
                amplitudes.push(fit.amplitude_m);
                dynamics.push(json!({"dynamic":name,"amplitude_m":fit.amplitude_m,"amplitude_over_gap":fit.amplitude_m/g.gap_m,
                    "rms_error_db":fit.error_db,"ratios_db_h2_to_h8":fit.ratios,"targets_db":targets.iter().map(|(n,t)|json!({"harmonic":n,"db":t})).collect::<Vec<_>>()}));
            }
            let combined = (total / TARGETS.len() as f64).sqrt();
            let monotone = amplitudes[0] > amplitudes[1] && amplitudes[1] > amplitudes[2];
            // Harmonics of the geometry at the engine's own amplitudes.
            let engine: Vec<Value> = ENGINE_AMPLITUDES_MM
                .iter()
                .map(|a| {
                    let h = voltage_harmonics(g, a * 1e-3).unwrap();
                    json!({"amplitude_mm":a,"ratios_db_h2_to_h8":ratios_db(&h)})
                })
                .collect();
            let row = json!({"law":law.name(),"gap_mm":g.gap_m*1e3,"offset_mm":g.offset_m*1e3,"pole_radius_mm":g.pole_radius_m*1e3,
                "combined_rms_error_db":combined,"amplitudes_monotone_with_dynamics":monotone,
                "loud_over_soft_amplitude":amplitudes[0]/amplitudes[2],"loud_over_medium_amplitude":amplitudes[0]/amplitudes[1],
                "dynamics":dynamics,"at_engine_amplitudes":engine});
            if best.as_ref().is_none_or(|(e, _)| combined < *e) {
                best = Some((combined, row.clone()));
            }
            rows.push(row);
        }
        let (_, row) = best.unwrap();
        println!(
            "Pickup harmonics: {} best geometry gap {} mm offset {} mm radius {} mm, combined error {:.3} dB",
            law.name(),
            row["gap_mm"],
            row["offset_mm"],
            row["pole_radius_mm"],
            row["combined_rms_error_db"].as_f64().unwrap()
        );
        best_per_law.push(row);
    }
    let report = json!({"schema_version":1,"experiment":"pickup-harmonics-v1","passed":true,
        "targets":TARGETS.iter().map(|(name,t)|json!({"dynamic":name,"harmonics":t.iter().map(|(n,db)|json!({"harmonic":n,"db_relative_to_fundamental":db})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "target_provenance":"references/g3-playable-diagnostic tone-v1.0-L1, tone-v0.6-L3 and tone-v0.25-L5, attack_96_ms window, reference_relative_to_fundamental_db; harmonic 6 excluded because it coincides with the tine's second bending mode at G3",
        "engine_amplitudes_mm":ENGINE_AMPLITUDES_MM,"engine_amplitude_provenance":"playable 0.1.2 engine --trace, note 55, 50-100 ms after strike, velocities 1.0, 0.6 and 0.25",
        "drive_hz":196.0,"samples_per_period":SAMPLES,"harmonics":HARMONICS,"gaps_mm":GAPS_MM,"offsets_mm":OFFSETS_MM,"pole_radii_mm":RADII_MM,
        "amplitude_range_mm":AMPLITUDE_RANGE_MM,"amplitude_steps":AMPLITUDE_STEPS,"best_per_law":best_per_law,"geometries":rows,
        "protocol":"Each law's flux linkage is sampled over one period of x(t)=A sin(wt) at 4096 points; the induced voltage's n-th harmonic is n times the n-th flux Fourier coefficient. For every geometry on the grid and each dynamic, the amplitude minimizing the RMS dB error against the retained harmonic targets is found on a 61-point geometric grid from 0.02 to 3 mm and refined by a 41-point sweep within 10%. The combined error is the RMS of the three dynamic errors; amplitudes are free per dynamic and the geometry is shared. Harmonics at the engine's traced amplitudes are retained for every geometry. No mechanics, circuit, filter or audio is involved.",
        "scope":"Static transfer characterization with sinusoidal motion at one frequency and one lateral coordinate; real tine motion has two coordinates and decaying, non-sinusoidal content. Targets come from processed recordings with unknown capture gain and one recording per layer. Flux scales are arbitrary; only ratios are compared. No law or geometry is adopted."});
    crate::analysis::write_report(output, &report)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn harmonic_analysis_recovers_known_nonlinearities() {
        // A linear flux gives no harmonics; x + eps x^2 gives H2/H1 = eps*A.
        let amplitude = 0.4e-3;
        let sample = |f: &dyn Fn(f64) -> f64| {
            let mut phi = Vec::new();
            for k in 0..SAMPLES {
                phi.push(f(amplitude * (TAU * k as f64 / SAMPLES as f64).sin()));
            }
            let mut out = [0.0; HARMONICS];
            for (n, slot) in out.iter_mut().enumerate() {
                let order = (n + 1) as f64;
                let (mut re, mut im) = (0.0, 0.0);
                for (k, v) in phi.iter().enumerate() {
                    let theta = order * TAU * k as f64 / SAMPLES as f64;
                    re += v * theta.cos();
                    im -= v * theta.sin();
                }
                *slot = order * 2.0 * (re * re + im * im).sqrt() / SAMPLES as f64;
            }
            out
        };
        let linear = sample(&|x| 3.0 * x);
        assert!((linear[0] - 3.0 * amplitude).abs() < 1e-12);
        assert!(linear[1..].iter().all(|h| *h < 1e-12));
        let eps = 500.0;
        let quadratic = sample(&|x| x + eps * x * x);
        assert!((quadratic[1] / quadratic[0] - eps * amplitude).abs() < 1e-9);
        assert!(quadratic[2] < 1e-12);
    }
    #[test]
    fn centered_geometry_doubles_frequency_and_offset_restores_the_fundamental() {
        // An even flux of an odd motion holds only even harmonics: a centered
        // pickup outputs the octave. A lateral offset brings the fundamental back.
        for law in [Law::Production, Law::PointPole, Law::FiniteAperture] {
            let centered = Geometry {
                law,
                gap_m: 0.001,
                offset_m: 0.0,
                pole_radius_m: 0.0005,
            };
            let h = voltage_harmonics(centered, 0.8e-3).unwrap();
            assert!(h[1] > 0.0, "{law:?} {h:?}");
            assert!(
                h[0] < 1e-9 * h[1] && h[2] < 1e-9 * h[1],
                "{law:?} odd {h:?}"
            );
            let offset = Geometry {
                offset_m: 0.5e-3,
                ..centered
            };
            let h = voltage_harmonics(offset, 0.8e-3).unwrap();
            assert!(h[0] > h[1], "{law:?} with offset {h:?}");
            assert!(h[2] > 1e-3 * h[0], "{law:?} third with offset {h:?}");
        }
    }
    #[test]
    fn fitting_a_geometry_to_its_own_harmonics_recovers_the_amplitude() {
        let g = Geometry {
            law: Law::PointPole,
            gap_m: 0.001,
            offset_m: 0.25e-3,
            pole_radius_m: 0.0,
        };
        let truth = 0.6e-3;
        let r = ratios_db(&voltage_harmonics(g, truth).unwrap());
        let targets: Vec<(usize, f64)> = (2..=5).map(|n| (n, r[n - 2])).collect();
        let fit = fit_dynamic(g, &targets, &amplitude_grid()).unwrap();
        assert!((fit.amplitude_m / truth - 1.0).abs() < 0.01);
        assert!(fit.error_db < 0.1);
    }
}
