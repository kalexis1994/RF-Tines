//! Independent uniform-disk boundary integration versus the retained 16-node proxy.
use rf_tines_analysis::{AudioClip, ToneComparisonOptions, compare_tone, detect_timbre_onset};
use rf_tines_dsp::{AxialAperture, ProductionDecimator, Profile, SpatialPickupProfile, Voice};
use serde_json::{Value, json};
use std::{error::Error, f64::consts::TAU, fs, io::Write};

// Divergence theorem: d/dq integral_disk F(q-r) dA = -integral_boundary F n_x ds.
// For uniform disk area pi R^2, this becomes -2/R * mean_theta(F cos(theta)).
fn boundary_slope(q: f64, gap: f64, offset: f64, radius: f64, nodes: usize) -> f64 {
    let sum: f64 = (0..nodes)
        .map(|i| {
            let (s, c) = (TAU * (i as f64 + 0.5) / nodes as f64).sin_cos();
            let r2 = gap * gap + (q + offset - radius * c).powi(2) + (radius * s).powi(2);
            c / (r2 * r2.sqrt())
        })
        .sum();
    -2.0 * 0.001 * gap.powi(3) / radius * sum / nodes as f64
}

// Independent area midpoint rule, uniform in squared radius.
fn area_slope(q: f64, gap: f64, offset: f64, radius: f64, nr: usize, na: usize) -> f64 {
    let mut sum = 0.0;
    for i in 0..nr {
        let r = radius * ((i as f64 + 0.5) / nr as f64).sqrt();
        for j in 0..na {
            let (s, c) = (TAU * (j as f64 + 0.5) / na as f64).sin_cos();
            let u = q + offset - r * c;
            let r2 = gap * gap + u * u + (r * s).powi(2);
            sum += u / (r2 * r2 * r2.sqrt());
        }
    }
    -3.0 * 0.001 * gap.powi(3) * sum / (nr * na) as f64
}

fn harmonics(samples: &[f64]) -> Value {
    let amps: Vec<f64> = (1..=6)
        .map(|h| {
            let (mut re, mut im) = (0.0, 0.0);
            for (i, &v) in samples.iter().enumerate() {
                let (s, c) = (TAU * h as f64 * i as f64 / samples.len() as f64).sin_cos();
                re += v * c;
                im += v * s;
            }
            2.0 * re.hypot(im) / samples.len() as f64
        })
        .collect();
    json!({"amplitudes":amps,"relative_h1_db":amps.iter().map(|a|20.0*(a/amps[0]).log10()).collect::<Vec<_>>()})
}
fn rel_error(a: &[f64], b: &[f64]) -> f64 {
    (a.iter().zip(b).map(|(x, y)| (x - y).powi(2)).sum::<f64>()
        / b.iter().map(|v| v * v).sum::<f64>())
    .sqrt()
}
fn frozen_motion(source: &str, profile: Profile) -> Result<Vec<Value>, Box<dyn Error>> {
    let mut rows = Vec::new();
    let gap = 0.0005_f64;
    let radius = 0.002_f64;
    let offset = 0.0005;
    let terms: Vec<_> = (0..128)
        .map(|i| {
            let (s, c) = (TAU * (i as f64 + 0.5) / 128.0).sin_cos();
            (radius * c, gap * gap + (radius * s).powi(2), c)
        })
        .collect();
    let dense = |q: f64| {
        -2.0 * 0.001 * gap.powi(3) / radius / 128.0
            * terms
                .iter()
                .map(|&(nx, lateral, c)| {
                    let r2 = lateral + (q + offset - nx).powi(2);
                    c / (r2 * r2.sqrt())
                })
                .sum::<f64>()
    };
    let pickup = AxialAperture::new(SpatialPickupProfile {
        gap_m: gap,
        offset_xy_m: [offset, 0.0],
        pole_radius_m: radius,
        pole_wedge: 0.0,
        flux_scale_wb: 0.001,
    })?;
    for (note, name) in [(55, "G2"), (64, "E3")] {
        for (layer, velocity) in [("p", 0.25), ("mp", 0.45), ("mf", 0.65), ("f", 0.85)] {
            let reference = AudioClip::open(
                std::path::Path::new(source).join(format!("{name}-{layer}.wav")),
                None,
            )?;
            let reference_onset = detect_timbre_onset(&reference)?;
            let mut voice = Voice::new(48000.0, note, profile)?;
            voice.strike(velocity);
            let mut filters = [ProductionDecimator::new(), ProductionDecimator::new()];
            let mut samples = [Vec::new(), Vec::new()];
            for _ in 0..33600 {
                for _ in 0..4 {
                    voice.tick();
                    let p = voice.probe();
                    filters[0]
                        .push(-15.0 * pickup.slope_wb_per_m(p.displacement_m) * p.velocity_m_s);
                    filters[1].push(-15.0 * dense(p.displacement_m) * p.velocity_m_s);
                }
                for i in 0..2 {
                    samples[i].push(filters[i].output() * 0.012);
                }
            }
            for (name, samples) in ["discrete_16", "uniform_disk_128"].into_iter().zip(samples) {
                let clip = AudioClip::from_samples(48000, samples)?;
                let onset = detect_timbre_onset(&clip)?;
                let comparison = compare_tone(
                    &reference,
                    &clip,
                    ToneComparisonOptions {
                        note,
                        reference_start_seconds: reference_onset,
                        candidate_start_seconds: onset,
                    },
                )?;
                let windows: Vec<_> = comparison
                    .windows
                    .iter()
                    .map(|w| {
                        json!({"label":w.label,
                    "harmonics":w.harmonics.iter().take(6).collect::<Vec<_>>()})
                    })
                    .collect();
                rows.push(
                    json!({"note":note,"layer":layer,"velocity":velocity,"pickup":name,
                    "reference_onset":reference_onset,"candidate_onset":onset,"windows":windows}),
                );
            }
        }
    }
    Ok(rows)
}
fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("usage: aperture_diagnostic NEW_REPORT_JSON [SOURCE_SAMPLES]")?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let profile = Profile {
        decay_seconds: 20.0,
        bar_partial_decay_seconds: 2.3,
        bar_partial_strike_weight: -0.3 * 0.2582 * 0.2582,
        ..Profile::default()
    };
    let mut rows = Vec::new();
    for note in [55, 64] {
        for velocity in [0.25, 0.45, 0.65, 0.85] {
            let mut voice = Voice::new(48000.0, note, profile)?;
            voice.strike(velocity);
            let mut amplitude = 0.0_f64;
            for _ in 0..(48000 * 4 / 10) {
                voice.tick();
                amplitude = amplitude.max(voice.probe().displacement_m.abs());
            }
            for gap in [0.0005, 0.0005665742265334133] {
                let discrete = AxialAperture::new(SpatialPickupProfile {
                    gap_m: gap,
                    offset_xy_m: [0.0005, 0.0],
                    pole_radius_m: 0.002,
                    pole_wedge: 0.0,
                    flux_scale_wb: 0.001,
                })?;
                let mut signals = [Vec::new(), Vec::new(), Vec::new()];
                for i in 0..2048 {
                    let (s, c) = (TAU * i as f64 / 2048.0).sin_cos();
                    let q = amplitude * s;
                    let v = amplitude * c;
                    // Unit angular speed: harmonic ratios do not depend on frequency or gain.
                    signals[0].push(-discrete.slope_wb_per_m(q) * v);
                    signals[1].push(-boundary_slope(q, gap, 0.0005, 0.002, 128) * v);
                    signals[2].push(-boundary_slope(q, gap, 0.0005, 0.002, 256) * v);
                }
                rows.push(json!({"note":note,"velocity":velocity,"peak_tip_displacement_m":amplitude,
                    "gap_m":gap,"discrete":harmonics(&signals[0]),"uniform_disk":harmonics(&signals[2]),
                    "discrete_vs_disk_relative_rms":rel_error(&signals[0],&signals[2]),
                    "boundary_128_vs_256_relative_rms":rel_error(&signals[1],&signals[2])}));
            }
        }
    }
    let mut slopes = Vec::new();
    for q in [-0.001, -0.0005, 0.0, 0.0005, 0.001, 0.002] {
        slopes.push(json!({"displacement_m":q,
            "boundary_512":boundary_slope(q,0.0005,0.0005,0.002,512),
            "area_128_512":area_slope(q,0.0005,0.0005,0.002,128,512),
            "area_256_1024":area_slope(q,0.0005,0.0005,0.002,256,1024)}));
    }
    let frozen = std::env::args()
        .nth(2)
        .map(|source| frozen_motion(&source, profile))
        .transpose()?;
    let report = json!({"schema_version":1,
        "method":"Analytic sine motion with peak amplitude traced from first 100 ms of existing Voice; 2048 phase samples; independent uniform-disk boundary integration",
        "scope":"Numerical proxy diagnostic only; no source fit, engine change or physical magnetic-field claim. Optional frozen-motion comparisons share the existing mechanics and production filter; no level compensation needed for harmonic ratios.",
        "fresh_reserved_validation_notes":[40,47,62,69,79,88],
        "reserved_notes_used":false,"rows":rows,"independent_area_checks":slopes,"frozen_motion":frozen});
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.write_all(b"\n")?;
    println!(
        "Completed {} transfer cases and {} independent area checks",
        rows.len(),
        slopes.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pickup_choice_preserves_the_mechanical_trajectory() {
        let base = Profile {
            decay_seconds: 20.0,
            bar_partial_decay_seconds: 2.3,
            bar_partial_strike_weight: -0.3 * 0.2582 * 0.2582,
            ..Profile::default()
        };
        let mut a = Voice::new(48000.0, 64, base).unwrap();
        let mut b = Voice::new(
            48000.0,
            64,
            Profile {
                pickup_law: rf_tines_dsp::PickupLaw::Aperture,
                pickup_gap_m: 0.0005,
                ..base
            },
        )
        .unwrap();
        a.strike(0.85);
        b.strike(0.85);
        for _ in 0..19200 {
            a.tick();
            b.tick();
            assert_eq!(a.probe().displacement_m, b.probe().displacement_m);
            assert_eq!(a.probe().velocity_m_s, b.probe().velocity_m_s);
        }
    }
    #[test]
    fn boundary_matches_independent_area_integral() {
        for q in [-0.001, 0.0, 0.001, 0.002] {
            let a = area_slope(q, 0.0005, 0.0005, 0.002, 256, 512);
            let b = boundary_slope(q, 0.0005, 0.0005, 0.002, 256);
            assert!((a - b).abs() < 2e-5, "q={q}: {a} versus {b}");
        }
    }
    #[test]
    fn boundary_recovers_small_disk_point_limit_and_symmetry() {
        let (g, o) = (0.0015_f64, 0.0005_f64);
        let point = -3.0 * 0.001 * g.powi(3) * o / (g * g + o * o).powf(2.5);
        let small = boundary_slope(0.0, g, o, 0.000001, 256);
        assert!((small / point - 1.0).abs() < 1e-6);
        assert!(boundary_slope(-o, g, o, 0.002, 256).abs() < 1e-12);
        assert!(
            (boundary_slope(0.001, g, 0.0, 0.002, 256)
                + boundary_slope(-0.001, g, 0.0, 0.002, 256))
            .abs()
                < 1e-12
        );
    }
    #[test]
    fn fourier_ratios_are_gain_invariant() {
        let signal: Vec<_> = (0..2048)
            .map(|i| {
                let t = TAU * i as f64 / 2048.0;
                t.sin() + 0.3 * (2.0 * t).sin()
            })
            .collect();
        let h = harmonics(&signal);
        assert!((h["relative_h1_db"][1].as_f64().unwrap() - 20.0 * 0.3_f64.log10()).abs() < 1e-10);
        let scaled: Vec<_> = signal.iter().map(|x| x * 7.0).collect();
        assert!(
            (harmonics(&scaled)["relative_h1_db"][1].as_f64().unwrap()
                - h["relative_h1_db"][1].as_f64().unwrap())
            .abs()
                < 1e-10
        );
    }
}
