use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct Scratch(PathBuf);
static SCRATCH_ID: AtomicU64 = AtomicU64::new(0);

#[test]
fn voicing_level_receipt_holds_the_reference_note_and_shows_the_laws_amplitude_dependence() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let r: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("voicing-level-validation.json")).unwrap())
            .unwrap();
    assert_eq!(r["experiment"], "voicing-level-v1");
    assert_eq!(r["passed"], true);
    assert!(r["worst_reference_relative_db"].as_f64().unwrap() < 3.0);
    let cells = r["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 72);
    let level = |cell: &serde_json::Value, note: u64, velocity: f64| -> f64 {
        cell["notes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["note"] == note && n["velocity"] == velocity)
            .unwrap()["relative_db"]
            .as_f64()
            .unwrap()
    };
    let find = |law: &str, gap: f64, offset: f64| -> &serde_json::Value {
        cells
            .iter()
            .find(|c| c["law"] == law && c["gap_mm"] == gap && c["offset_mm"] == offset)
            .unwrap()
    };
    // The default pickup is the reference: compensation exactly one, every note 0 dB.
    let default = find("production", 1.5, 0.5);
    assert_eq!(default["level_compensation"], 1.0);
    for (note, velocity) in [(55, 0.25), (55, 0.6), (55, 1.0), (50, 0.6), (59, 0.6)] {
        assert_eq!(level(default, note, velocity), 0.0);
    }
    for cell in cells {
        let compensation = cell["level_compensation"].as_f64().unwrap();
        assert!(compensation.is_finite() && compensation > 0.0);
        // The reference note holds within 3 dB in every cell; off-centre cells of
        // the production law hold it within 1 dB.
        let reference = level(cell, 55, 0.6);
        assert!(reference.abs() < 3.0, "{cell}");
        if cell["law"] == "production" && cell["offset_mm"] != 0.0 && cell["offset_mm"] != 0.25 {
            assert!(reference.abs() < 1.0, "{cell}");
        }
        // Compensation equalizes the medium note, not the dynamics: a centred pickup
        // plays the soft note 9 dB or more below the reference and the loud note above it.
        if cell["offset_mm"] == 0.0 {
            assert!(level(cell, 55, 0.25) < -9.0, "{cell}");
            assert!(level(cell, 55, 1.0) > 0.0, "{cell}");
        }
    }
    // The production law's compensation grows with the gap at the default offset.
    let mut previous = 0.0;
    for gap in [0.5, 0.75, 1.0, 1.5, 2.0, 3.0] {
        let compensation = find("production", gap, 0.5)["level_compensation"]
            .as_f64()
            .unwrap();
        assert!(compensation > previous, "gap {gap}");
        previous = compensation;
    }
    // The Close Aperture geometry's compensation matches the frozen level factor
    // of the laboratory path within the difference between a reference sine and the
    // 24-second performance.
    let aperture = find("aperture", 0.5, 0.5)["level_compensation"]
        .as_f64()
        .unwrap();
    let frozen = rf_tines_dsp::PICKUP_LEVEL_MATCH[3];
    assert!(
        (aperture / frozen - 1.0).abs() < 0.1,
        "{aperture} vs {frozen}"
    );
}

#[test]
fn voicing_level_cli_rejects_invalid_arguments_and_preserves_outputs() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["voicing-level"],
        vec!["voicing-level", "--output", "keep.json"],
        vec!["voicing-level", "--output", "bad.wav"],
        vec!["voicing-level", "--output", "bad.json", "--extra"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists());
}

#[test]
fn render_pickup_law_and_velocity_exponent_options_reach_the_profile() {
    let scratch = Scratch::new();
    let base = [
        "render",
        "--note",
        "55",
        "--velocity",
        "0.7",
        "--sample-rate",
        "44100",
        "--seconds",
        "0.5",
        "--hold",
        "0.4",
    ];
    let with = |extra: &[&str]| {
        let mut args: Vec<&str> = base.to_vec();
        args.extend(extra);
        args.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    };
    for extra in [
        vec!["--output", "default.wav"],
        vec![
            "--output",
            "explicit.wav",
            "--law",
            "production",
            "--velocity-exponent",
            "1.4",
        ],
        vec![
            "--output",
            "aperture.wav",
            "--law",
            "aperture",
            "--gap-mm",
            "0.5",
            "--offset-mm",
            "0.5",
        ],
        vec![
            "--output",
            "radius.wav",
            "--law",
            "aperture",
            "--pole-radius-mm",
            "1",
        ],
        vec!["--output", "curve.wav", "--velocity-exponent", "2"],
        vec!["--output", "wide.wav", "--gap-mm", "3", "--compensate"],
        vec!["--output", "wide-raw.wav", "--gap-mm", "3"],
    ] {
        let owned = with(&extra);
        scratch.success(&owned.iter().map(String::as_str).collect::<Vec<_>>());
    }
    let default = fs::read(scratch.0.join("default.wav")).unwrap();
    assert_eq!(fs::read(scratch.0.join("explicit.wav")).unwrap(), default);
    for name in ["aperture.wav", "radius.wav", "curve.wav", "wide.wav"] {
        assert_ne!(fs::read(scratch.0.join(name)).unwrap(), default, "{name}");
    }
    let report = scratch.json("default.json");
    assert_eq!(report["pickup_law"], "production");
    assert_eq!(report["velocity_exponent"], 1.4);
    assert_eq!(report["level_compensation"], 1.0);
    assert_eq!(report["level_compensated"], false);
    let aperture = scratch.json("aperture.json");
    assert_eq!(aperture["pickup_law"], "aperture");
    assert_eq!(aperture["pickup_pole_radius_mm"], 2.0);
    assert!(aperture["level_compensation"].as_f64().unwrap() > 1.5);
    assert_eq!(scratch.json("radius.json")["pickup_pole_radius_mm"], 1.0);
    assert_eq!(scratch.json("curve.json")["velocity_exponent"], 2.0);
    // A compensated wide gap keeps the medium note's RMS near the default's.
    let wide = scratch.json("wide.json");
    assert!(wide["level_compensation"].as_f64().unwrap() > 2.0);
    assert_eq!(wide["level_compensated"], true);
    let ratio = wide["rms"].as_f64().unwrap() / report["rms"].as_f64().unwrap();
    assert!((0.7..1.42).contains(&ratio), "{ratio}");
    // Without the flag the wide gap renders raw and quieter, as it always did.
    let raw = scratch.json("wide-raw.json");
    assert_eq!(raw["level_compensated"], false);
    assert_eq!(raw["level_compensation"], wide["level_compensation"]);
    assert!(raw["rms"].as_f64().unwrap() < 0.6 * wide["rms"].as_f64().unwrap());
    for extra in [
        vec!["--law", "coil"],
        vec!["--pole-radius-mm", "0.1"],
        vec!["--velocity-exponent", "0.1"],
        vec!["--velocity-exponent", "x"],
        vec!["--law", "aperture", "--pickup", "3"],
    ] {
        let mut args = with(&["--output", "bad.wav"]);
        args.extend(extra.iter().map(|s| s.to_string()));
        assert!(
            !scratch
                .run(&args.iter().map(String::as_str).collect::<Vec<_>>())
                .status
                .success(),
            "{extra:?}"
        );
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn playable_bar_partial_receipt_bounds_the_second_partial_and_identifies_the_sixth_harmonic() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references/playable-bar-partial");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let summary = read("summary.json");
    assert_eq!(summary["experiment"], "playable-bar-partial-v1");
    let retained = summary["retained_files"].as_array().unwrap();
    assert_eq!(retained.len(), 18);
    for entry in retained {
        let name = entry["file"].as_str().unwrap();
        let bytes = fs::read(root.join(name)).unwrap();
        assert_eq!(
            bytes.len() as u64,
            entry["bytes"].as_u64().unwrap(),
            "{name}"
        );
        assert_eq!(
            sha256_hex(&bytes),
            entry["sha256"].as_str().unwrap(),
            "{name}"
        );
    }
    let chosen = summary["chosen_strike_weight"].as_f64().unwrap();
    assert_eq!(
        chosen,
        rf_tines_dsp::Profile::calibrated().bar_partial_strike_weight
    );
    assert_eq!(
        rf_tines_dsp::Profile::calibrated().bar_partial_ratio,
        rf_tines_dsp::Profile::default().bar_partial_ratio
    );
    // The recordings' line sits at six times the fundamental within 0.5% at all
    // three notes: a harmonic of the pickup, not a bending partial with its own ratio.
    for note in ["d3", "g3", "b3"] {
        for ratio in summary["recording_line_6_0_ratios"][note]
            .as_array()
            .unwrap()
        {
            let ratio = ratio.as_f64().unwrap();
            assert!((5.99..=6.03).contains(&ratio), "{note} {ratio}");
        }
    }
    let sweep = summary["sweep"].as_array().unwrap();
    assert_eq!(sweep.len(), 36);
    let level = |x: &serde_json::Value, side: &str, key: &str| x[side][key].as_f64();
    for note in ["d3", "g3", "b3"] {
        for velocity in [1.0, 0.6, 0.25] {
            let rows: Vec<&serde_json::Value> = sweep
                .iter()
                .filter(|x| x["note"] == note && x["velocity"] == velocity)
                .collect();
            assert_eq!(rows.len(), 4);
            // At the medium and soft dynamics the engine's 6.27 partial falls
            // monotonically as the strike weight shrinks and ends at least 43 dB below
            // the fundamental with the chosen weight; at the loud dynamic it is buried
            // under the sixth harmonic and only its bound is held.
            let mut previous = f64::INFINITY;
            for row in &rows {
                if let Some(mode) = level(row, "engine", "line_6_27_relative_db") {
                    if velocity < 1.0 {
                        assert!(
                            mode < previous + 0.5,
                            "{note} v{velocity} {mode} after {previous}"
                        );
                        previous = mode;
                    }
                    if row["strike_weight"] == chosen {
                        assert!(mode < -43.0, "{note} v{velocity} chosen {mode}");
                    }
                }
            }
            let default_row = rows.iter().find(|r| r["strike_weight"] == -0.3).unwrap();
            let default_mode = level(default_row, "engine", "line_6_27_relative_db").unwrap();
            // At the medium and soft dynamics the default partial stands 20 to 35 dB
            // above anything in the recording at that frequency.
            if velocity < 1.0 {
                assert!(
                    default_mode > -30.0,
                    "{note} v{velocity} default {default_mode}"
                );
                if let Some(recorded) = level(default_row, "recording", "line_6_0_relative_db") {
                    assert!(default_mode - recorded > 3.0, "{note} v{velocity}");
                }
            }
            // The engine's own sixth harmonic at the loud dynamic is within 12 dB of the
            // recording's line, whatever the strike weight.
            if velocity == 1.0 {
                for row in &rows {
                    let engine = level(row, "engine", "line_6_0_relative_db").unwrap();
                    let recorded = level(row, "recording", "line_6_0_relative_db").unwrap();
                    assert!(
                        (engine - recorded).abs() < 12.0,
                        "{note} {engine} vs {recorded}"
                    );
                }
            }
        }
    }
    // Contact durations: the default contact lasts 0.09 to 0.34 ms and shortens with
    // velocity; a hundredfold softer contact only reaches a millisecond at soft strikes.
    let contact = summary["contact_durations"].as_array().unwrap();
    let duration = |file: &str| {
        contact.iter().find(|c| c["file"] == file).unwrap()["contact_ms"]
            .as_f64()
            .unwrap()
    };
    for note in [50, 55, 59] {
        let mut previous = f64::INFINITY;
        for velocity in ["0.1", "0.25", "0.6", "1.0"] {
            let d = duration(&format!("n{note}-v{velocity}"));
            assert!(
                (0.09..=0.34).contains(&d) && d < previous,
                "n{note} v{velocity} {d}"
            );
            previous = d;
        }
    }
    assert!(duration("k4e8-v0.25") > 1.0 && duration("k4e8-v1.0") < 0.6);
    assert!(duration("k1e8-v0.25") > 4.0);
    for note in ["d3", "g3", "b3"] {
        for (velocity, layer) in [("1.0", 1), ("0.6", 3), ("0.25", 5)] {
            let render = read(&format!("engine/render-{note}-v{velocity}.json"));
            assert_eq!(render["bar_partial_strike_weight"], chosen);
            assert_eq!(render["bar_partial_ratio"], 6.267);
            assert_eq!(render["sustain"], "calibrated");
            assert_eq!(render["pickup_path"], 3);
            assert_eq!(render["faults"], 0);
            let attack = read(&format!("engine/attack-{note}-v{velocity}-L{layer}.json"));
            assert!(
                attack["partial_comparison"]["candidate_tracking"]["tracks"]
                    .as_array()
                    .unwrap()
                    .len()
                    > 3
            );
        }
    }
}

#[test]
fn render_bar_partial_options_reach_the_profile_and_default_to_the_retained_engine() {
    let scratch = Scratch::new();
    let base = [
        "render",
        "--note",
        "55",
        "--velocity",
        "0.7",
        "--sample-rate",
        "44100",
        "--seconds",
        "0.5",
        "--hold",
        "0.4",
    ];
    let with = |extra: &[&str]| {
        let mut args: Vec<&str> = base.to_vec();
        args.extend(extra);
        args.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    };
    for extra in [
        vec!["--output", "default.wav"],
        vec![
            "--output",
            "uniform.wav",
            "--bar-ratio",
            "6.267",
            "--bar-strike",
            "-0.3",
        ],
        vec!["--output", "tuned.wav", "--bar-ratio", "6.0"],
        vec!["--output", "soft.wav", "--contact-stiffness", "4e9"],
        vec!["--output", "quiet.wav", "--bar-strike", "-0.02"],
        vec![
            "--output",
            "calibrated.wav",
            "--sustain",
            "calibrated",
            "--bar-strike",
            "-0.02",
            "--pickup",
            "3",
        ],
    ] {
        let owned = with(&extra);
        scratch.success(&owned.iter().map(String::as_str).collect::<Vec<_>>());
    }
    let default = fs::read(scratch.0.join("default.wav")).unwrap();
    assert_eq!(fs::read(scratch.0.join("uniform.wav")).unwrap(), default);
    for name in ["tuned.wav", "soft.wav", "quiet.wav"] {
        assert_ne!(fs::read(scratch.0.join(name)).unwrap(), default, "{name}");
    }
    let report = scratch.json("default.json");
    assert_eq!(report["bar_partial_ratio"], 6.267);
    // The profile default moved from 4e10 to 4e8, which is 116 us of hammer
    // contact against 547: see docs/WHAT-A-LISTENER-HEARD.md.
    assert_eq!(report["contact_stiffness"], 4.0e8);
    assert_eq!(report["bar_partial_strike_weight"], -0.3);
    assert_eq!(scratch.json("tuned.json")["bar_partial_ratio"], 6.0);
    assert_eq!(scratch.json("soft.json")["contact_stiffness"], 4.0e9);
    assert_eq!(
        scratch.json("quiet.json")["bar_partial_strike_weight"],
        -0.02
    );
    let calibrated = scratch.json("calibrated.json");
    assert_eq!(calibrated["bar_partial_strike_weight"], -0.02);
    assert_eq!(calibrated["sustain"], "calibrated");
    assert_eq!(calibrated["pickup_path"], 3);
    for extra in [
        vec!["--bar-ratio", "1.5"],
        vec!["--bar-ratio", "13"],
        vec!["--bar-ratio", "x"],
        vec!["--contact-stiffness", "1e6"],
        vec!["--contact-stiffness", "nan"],
        vec!["--contact-stiffness"],
        vec!["--bar-strike", "1.5"],
        vec!["--bar-strike", "inf"],
    ] {
        let mut args = with(&["--output", "bad.wav"]);
        args.extend(extra.iter().map(|s| s.to_string()));
        assert!(
            !scratch
                .run(&args.iter().map(String::as_str).collect::<Vec<_>>())
                .status
                .success(),
            "{extra:?}"
        );
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn playable_sustain_receipts_derive_the_calibrated_t60_anchors_and_close_the_decay_gap() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references/playable-sustain");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let manifest = read("manifest.json");
    let files = manifest["files"].as_array().unwrap();
    assert_eq!(files.len(), 15 + 10 + 9 + 10);
    for entry in files {
        let name = entry["file"].as_str().unwrap();
        let bytes = fs::read(root.join(name)).unwrap();
        assert_eq!(
            bytes.len() as u64,
            entry["bytes"].as_u64().unwrap(),
            "{name}"
        );
        assert_eq!(
            sha256_hex(&bytes),
            entry["sha256"].as_str().unwrap(),
            "{name}"
        );
    }
    let profile = rf_tines_dsp::Profile::calibrated_sustain();
    assert_eq!(
        manifest["profile"]["decay_seconds_at_a3"],
        profile.decay_seconds
    );
    assert_eq!(
        manifest["profile"]["bar_partial_decay_seconds_at_a3"],
        profile.bar_partial_decay_seconds
    );
    let mean = |track: &serde_json::Value, key: &str| -> Option<f64> {
        let values: Vec<f64> = track["observations"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|o| o[key].as_f64())
            .collect();
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    };
    // Anchors from the recordings: late slope of the 0.5..4.5 s window, T60 = 60/|slope|,
    // divided by sqrt(220 / f0) to refer every note to A3.
    let mut fundamental = Vec::new();
    let mut bar = Vec::new();
    for (note, f0, dir) in [(50u8, 146.83, "D3"), (55, 196.0, "G3"), (59, 246.94, "B3")] {
        for layer in 1..=5 {
            let report = read(&format!("recordings/A_0{note}__{dir}_{layer}-sustain.json"));
            let tracking = &report["partial_comparison"]["reference_tracking"];
            for track in tracking["tracks"].as_array().unwrap() {
                let Some(frequency) = mean(track, "frequency_hz") else {
                    continue;
                };
                let Some(late) = track["decay"]["late_slope_db_per_second"].as_f64() else {
                    continue;
                };
                if track["observations"].as_array().unwrap().len() < 10 || late >= 0.0 {
                    continue;
                }
                let anchor = (60.0 / -late) / (220.0_f64 / f0).sqrt();
                let ratio = frequency / f0;
                if (ratio - 1.0).abs() < 0.03 {
                    fundamental.push(anchor);
                } else if (5.9..6.1).contains(&ratio) {
                    bar.push(anchor);
                }
            }
        }
    }
    let geomean =
        |values: &[f64]| (values.iter().map(|v| v.ln()).sum::<f64>() / values.len() as f64).exp();
    assert!(fundamental.len() >= 12, "{}", fundamental.len());
    assert!(bar.len() >= 5, "{}", bar.len());
    let fundamental_anchor = geomean(&fundamental);
    let bar_anchor = geomean(&bar);
    assert!(
        (fundamental_anchor / profile.decay_seconds - 1.0).abs() < 0.15,
        "{fundamental_anchor}"
    );
    assert!(
        (bar_anchor / profile.bar_partial_decay_seconds - 1.0).abs() < 0.15,
        "{bar_anchor}"
    );
    // The default profile's 5 s first partial is a quarter of the recordings'.
    assert!(fundamental_anchor > 3.5 * rf_tines_dsp::Profile::default().decay_seconds);
    // Engine against the recordings: qualified matched fundamental T60 differences
    // sit within 5 s where the diagnostic had 17 to 21 s deficits.
    let mut qualified = 0;
    for (tag, f0) in [("g3", 196.0), ("d3", 146.83), ("b3", 246.94)] {
        for (velocity, layer) in [("1.0", 1), ("0.6", 3), ("0.25", 5)] {
            let render = read(&format!("engine/render-{tag}-v{velocity}.json"));
            assert_eq!(render["sustain"], "calibrated");
            assert_eq!(render["pickup_path"], 3);
            assert_eq!(render["faults"], 0);
            let p = &read(&format!("engine/sustain-{tag}-v{velocity}-L{layer}.json"))["partial_comparison"];
            for m in p["matches"].as_array().map(|m| m.as_slice()).unwrap_or(&[]) {
                let ratio = m["mean_reference_frequency_hz"].as_f64().unwrap() / f0;
                if (ratio - 1.0).abs() < 0.03 && m["decay"]["status"] == "qualified" {
                    let difference = m["decay"]["candidate_minus_reference_t60_seconds"]
                        .as_f64()
                        .unwrap();
                    assert!(difference.abs() < 5.0, "{tag} v{velocity} {difference}");
                    qualified += 1;
                }
            }
            // The engine's fundamental now decays between 2 and 3.5 dB/s in the
            // sustain window at the medium and soft dynamics; at the loud one both
            // sides still rise during the first second, so its slope is not bounded.
            let tracks = p["candidate_tracking"]["tracks"].as_array().unwrap();
            let slope = tracks
                .iter()
                .filter(|t| mean(t, "frequency_hz").is_some_and(|f| (f / f0 - 1.0).abs() < 0.03))
                .filter_map(|t| t["decay"]["late_slope_db_per_second"].as_f64())
                .next()
                .unwrap();
            assert!(
                velocity == "1.0" || (-3.5..=-2.0).contains(&slope),
                "{tag} v{velocity} slope {slope}"
            );
            // The bar partial, still at the uniform 6.27 ratio, decays at 15 to 30 dB/s
            // instead of vanishing inside the first window.
            if velocity != "1.0" {
                let bar_slope = tracks
                    .iter()
                    .filter(|t| {
                        mean(t, "frequency_hz").is_some_and(|f| (6.2..6.35).contains(&(f / f0)))
                    })
                    .filter_map(|t| t["decay"]["late_slope_db_per_second"].as_f64())
                    .next()
                    .unwrap();
                assert!(
                    (-30.0..=-15.0).contains(&bar_slope),
                    "{tag} v{velocity} bar {bar_slope}"
                );
            }
        }
    }
    assert!(qualified >= 4, "{qualified}");
    // The default pickup with calibrated sustain shows the same fundamental decay.
    let default = read("engine/render-g3-v0.6-default-pickup.json");
    assert_eq!(default["pickup_path"], serde_json::Value::Null);
    assert_eq!(default["sustain"], "calibrated");
}

#[test]
fn render_sustain_option_selects_the_calibrated_profile() {
    let scratch = Scratch::new();
    let base = [
        "render",
        "--note",
        "55",
        "--velocity",
        "0.7",
        "--sample-rate",
        "44100",
        "--seconds",
        "0.5",
        "--hold",
        "0.4",
    ];
    let with = |extra: &[&str]| {
        let mut args: Vec<&str> = base.to_vec();
        args.extend(extra);
        args.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    };
    for extra in [
        vec!["--output", "original.wav"],
        vec!["--output", "explicit.wav", "--sustain", "original"],
        vec!["--output", "calibrated.wav", "--sustain", "calibrated"],
        vec![
            "--output",
            "both.wav",
            "--sustain",
            "calibrated",
            "--pickup",
            "3",
        ],
    ] {
        let owned = with(&extra);
        scratch.success(&owned.iter().map(String::as_str).collect::<Vec<_>>());
    }
    let original = fs::read(scratch.0.join("original.wav")).unwrap();
    assert_eq!(fs::read(scratch.0.join("explicit.wav")).unwrap(), original);
    let calibrated = fs::read(scratch.0.join("calibrated.wav")).unwrap();
    assert_ne!(calibrated, original);
    assert_eq!(scratch.json("original.json")["sustain"], "original");
    assert_eq!(scratch.json("calibrated.json")["sustain"], "calibrated");
    assert_eq!(scratch.json("both.json")["pickup_path"], 3);
    // Longer sustain leaves more energy at the end of the render.
    let tail_power = |bytes: &[u8]| {
        let samples = bytes[58..].as_chunks::<4>().0;
        samples[samples.len() * 3 / 4..]
            .iter()
            .map(|c| f64::from(f32::from_le_bytes(*c)).powi(2))
            .sum::<f64>()
    };
    assert!(tail_power(&calibrated) > tail_power(&original));
    for extra in [vec!["--sustain", "long"], vec!["--sustain"]] {
        let mut args = with(&["--output", "bad.wav"]);
        args.extend(extra.iter().map(|s| s.to_string()));
        assert!(
            !scratch
                .run(&args.iter().map(String::as_str).collect::<Vec<_>>())
                .status
                .success()
        );
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn aperture_listening_receipt_freezes_the_fourth_level_match_and_reproduces_the_retained_three() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("pickup-aperture-listening-summary.json");
    let prior = read("pickup-listening-summary.json");
    assert_eq!(r["retained_paths_reproduced_exactly"], true);
    let frozen = r["frozen_level_match"].as_array().unwrap();
    assert_eq!(frozen.len(), rf_tines_dsp::PICKUP_LEVEL_MATCH.len());
    for (value, constant) in frozen.iter().zip(rf_tines_dsp::PICKUP_LEVEL_MATCH) {
        assert_eq!(
            value.as_f64().unwrap(),
            constant,
            "frozen factor is the constant"
        );
    }
    let prior_gain = prior["studies"][0]["report"]["matching"]["rms_gain"]
        .as_array()
        .unwrap();
    for (value, previous) in frozen.iter().zip(prior_gain) {
        assert_eq!(value, previous);
    }
    let studies = r["studies"].as_array().unwrap();
    assert_eq!(studies.len(), 2);
    for (study, rate) in studies.iter().zip([44100, 192000]) {
        let report = &study["report"];
        assert_eq!(report["sample_rate"], rate);
        assert_eq!(report["faults"], 0);
        assert_eq!(
            report["track_order"],
            serde_json::json!([
                "current",
                "close-original",
                "close-point-pole",
                "close-aperture"
            ])
        );
        let geometry = &report["aperture_geometry_mm"];
        assert_eq!(geometry["gap"], 0.5);
        assert_eq!(geometry["offset"], 0.5);
        assert_eq!(geometry["pole_radius"], 2.0);
        assert_eq!(
            report["matching"]["rms_gain"],
            report["matching"]["rms_gain"]
        );
        // The raw aperture track is the quietest and needs the only gain above unity.
        let raw = report["raw_program_levels"].as_array().unwrap();
        let rms = |i: usize| raw[i]["rms"].as_f64().unwrap();
        assert!(rms(3) < rms(0) && rms(0) < rms(1) && rms(1) < rms(2));
        let gain = report["matching"]["rms_gain"][3].as_f64().unwrap();
        assert!((2.4..2.6).contains(&gain), "{gain}");
        // Isolated and stress peaks of the raw aperture track stay below the current track's.
        let peaks = report["diagnostics"]["maximum_isolated_peaks"]
            .as_array()
            .unwrap();
        assert!(peaks[3].as_f64().unwrap() < peaks[0].as_f64().unwrap());
        for key in ["repeated_chord_levels", "repeated_all_73_keys_levels"] {
            let levels = report["diagnostics"][key].as_array().unwrap();
            assert!(levels[3]["peak"].as_f64().unwrap() < levels[0]["peak"].as_f64().unwrap());
        }
    }
    // 192 kHz reproduces the 44.1 kHz factor within 0.002%.
    for difference in r["level_match_192k_relative_difference"]
        .as_array()
        .unwrap()
    {
        assert!(difference.as_f64().unwrap().abs() < 2e-5);
    }
    let audio = r["audio_files"].as_array().unwrap();
    assert_eq!(audio.len(), 4);
    for file in audio {
        assert_eq!(file["sha256"].as_str().unwrap().len(), 64);
        assert_eq!(file["bytes"], 4233658);
    }
}

#[test]
fn g3_aperture_pickup_receipts_hold_their_hashes_and_recover_the_recorded_harmonic_balance() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let manifest = read("g3-aperture-pickup/manifest.json");
    assert_eq!(manifest["pickup_path"], 3);
    let files = manifest["files"].as_array().unwrap();
    assert_eq!(files.len(), 12);
    for entry in files {
        let name = entry["file"].as_str().unwrap();
        let bytes = fs::read(root.join("g3-aperture-pickup").join(name)).unwrap();
        assert_eq!(
            bytes.len() as u64,
            entry["bytes"].as_u64().unwrap(),
            "{name}"
        );
        assert_eq!(
            sha256_hex(&bytes),
            entry["sha256"].as_str().unwrap(),
            "{name}"
        );
    }
    let attack = |dir: &str, velocity: &str, layer: u8| -> serde_json::Value {
        read(&format!("{dir}/tone-v{velocity}-L{layer}.json"))["tone_comparison"]["windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["label"] == "attack_96_ms")
            .unwrap()
            .clone()
    };
    let level = |window: &serde_json::Value, harmonic: u64, key: &str| -> Option<f64> {
        window["harmonics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["harmonic"] == harmonic)
            .and_then(|h| h[key].as_f64())
    };
    for (velocity, layer, limit) in [("1.0", 1, 5.5), ("0.6", 3, 5.0), ("0.25", 5, 5.5)] {
        let render = read(&format!("g3-aperture-pickup/render-v{velocity}.json"));
        assert_eq!(render["pickup_path"], 3);
        assert_eq!(render["pickup_name"], "Close Aperture");
        assert_eq!(render["faults"], 0);
        assert_eq!(render["sample_rate"], 44100);
        let new = attack("g3-aperture-pickup", velocity, layer);
        let old = attack("g3-playable-diagnostic", velocity, layer);
        // Same recording on both sides.
        for h in 2..=4 {
            assert_eq!(
                level(&new, h, "reference_relative_to_fundamental_db"),
                level(&old, h, "reference_relative_to_fundamental_db")
            );
        }
        let last = if layer == 5 { 4 } else { 5 };
        for h in 2..=last {
            let balance = level(&new, h, "candidate_minus_reference_balance_db").unwrap();
            assert!(balance.abs() < limit, "v{velocity} H{h} balance {balance}");
            if let Some(previous) = level(&old, h, "candidate_minus_reference_balance_db")
                && layer != 5
                && h >= 3
            {
                assert!(balance.abs() < previous.abs(), "v{velocity} H{h}");
            }
        }
    }
    // The loud third harmonic rises above the fundamental, as in the recording.
    let loud = attack("g3-aperture-pickup", "1.0", 1);
    assert!(level(&loud, 3, "candidate_relative_to_fundamental_db").unwrap() > 5.0);
    assert!(level(&loud, 6, "candidate_relative_to_fundamental_db").unwrap() > -4.0);
    // The soft attack stays within 2.6 dB on the second and third harmonics.
    let soft = attack("g3-aperture-pickup", "0.25", 5);
    for h in [2, 3] {
        let balance = level(&soft, h, "candidate_minus_reference_balance_db").unwrap();
        assert!(balance.abs() < 2.6, "soft H{h} {balance}");
    }
}

#[test]
fn render_pickup_path_zero_replays_the_raw_engine_and_other_paths_differ() {
    let scratch = Scratch::new();
    let base = [
        "render",
        "--note",
        "55",
        "--velocity",
        "0.9",
        "--sample-rate",
        "44100",
        "--seconds",
        "0.3",
        "--hold",
        "0.2",
    ];
    let with = |output: &str, extra: &[&str]| {
        let mut args: Vec<&str> = base.to_vec();
        args.extend(["--output", output]);
        args.extend(extra);
        args.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    };
    let owned = with("raw.wav", &[]);
    scratch.success(&owned.iter().map(String::as_str).collect::<Vec<_>>());
    let mut wavs = Vec::new();
    for index in 0..4 {
        let output = format!("path{index}.wav");
        let owned = with(&output, &["--pickup", &index.to_string()]);
        scratch.success(&owned.iter().map(String::as_str).collect::<Vec<_>>());
        let report = scratch.json(&format!("path{index}.json"));
        assert_eq!(report["pickup_path"], index);
        assert_eq!(report["pickup_name"], rf_tines_dsp::PICKUP_NAMES[index]);
        assert_eq!(report["faults"], 0);
        wavs.push(fs::read(scratch.0.join(output)).unwrap());
    }
    let raw = fs::read(scratch.0.join("raw.wav")).unwrap();
    assert_eq!(
        scratch.json("raw.json")["pickup_path"],
        serde_json::Value::Null
    );
    assert_eq!(wavs[0], raw, "path 0 is the raw engine sample for sample");
    for (i, wav) in wavs.iter().enumerate().skip(1) {
        assert_ne!(*wav, raw, "path {i}");
        assert_eq!(wav.len(), raw.len());
        let power = |bytes: &[u8]| {
            bytes[58..]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| f64::from(f32::from_le_bytes(*c)).powi(2))
                .sum::<f64>()
        };
        // Level matching keeps every path within one order of magnitude of the raw engine.
        let ratio = (power(wav) / power(&raw)).sqrt();
        assert!((0.1..10.0).contains(&ratio), "path {i} rms ratio {ratio}");
    }
    for extra in [
        vec!["--pickup", "4"],
        vec!["--pickup", "-1"],
        vec!["--pickup", "x"],
        vec!["--pickup", "3", "--gap-mm", "1.0"],
        vec!["--pickup", "3", "--offset-mm", "0.25"],
    ] {
        let owned = with("bad.wav", &extra);
        assert!(
            !scratch
                .run(&owned.iter().map(String::as_str).collect::<Vec<_>>())
                .status
                .success(),
            "{extra:?}"
        );
        assert!(!scratch.0.join("bad.wav").exists());
    }
    assert!(
        !scratch
            .run(&["demo", "--output", "demo.wav", "--pickup", "1"])
            .status
            .success()
    );
}

#[test]
fn render_midi_pickup_path_selects_a_laboratory_path() {
    let scratch = Scratch::new();
    let mut track = vec![0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20];
    track.extend([
        0x00, 0x90, 55, 100, 0x60, 0x80, 55, 0, 0x00, 0xFF, 0x2F, 0x00,
    ]);
    let mut midi = b"MThd".to_vec();
    midi.extend(6u32.to_be_bytes());
    midi.extend(0u16.to_be_bytes());
    midi.extend(1u16.to_be_bytes());
    midi.extend(96u16.to_be_bytes());
    midi.extend(b"MTrk");
    midi.extend((track.len() as u32).to_be_bytes());
    midi.extend(track);
    fs::write(scratch.0.join("one.mid"), &midi).unwrap();
    // The input and --output must lead; the remaining options follow.
    let tail = ["--tail", "0.2", "--sample-rate", "44100"];
    let mut args = vec!["render-midi", "one.mid", "--output", "raw.wav"];
    args.extend(tail);
    scratch.success(&args);
    let mut args = vec![
        "render-midi",
        "one.mid",
        "--output",
        "zero.wav",
        "--pickup",
        "0",
    ];
    args.extend(tail);
    scratch.success(&args);
    let mut args = vec![
        "render-midi",
        "one.mid",
        "--output",
        "aperture.wav",
        "--pickup",
        "3",
    ];
    args.extend(tail);
    scratch.success(&args);
    let raw = fs::read(scratch.0.join("raw.wav")).unwrap();
    assert_eq!(fs::read(scratch.0.join("zero.wav")).unwrap(), raw);
    assert_ne!(fs::read(scratch.0.join("aperture.wav")).unwrap(), raw);
    let receipt = scratch.json("aperture.json");
    assert_eq!(receipt["pickup_path"], 3);
    assert_eq!(receipt["pickup_name"], "Close Aperture");
    assert_eq!(receipt["faults"], 0);
    assert_eq!(
        scratch.json("raw.json")["pickup_path"],
        serde_json::Value::Null
    );
    for bad in [
        vec!["--pickup", "4"],
        vec!["--pickup", "a"],
        vec!["--pickup"],
    ] {
        let mut args = vec!["render-midi", "one.mid", "--output", "bad.wav"];
        args.extend(tail);
        args.extend(bad.clone());
        assert!(!scratch.run(&args).status.success(), "{bad:?}");
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn pickup_harmonics_receipt_explains_the_engine_and_finds_an_aperture_geometry() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("pickup-harmonics-validation.json");
    assert_eq!(r["experiment"], "pickup-harmonics-v1");
    assert_eq!(r["passed"], true);
    // Targets are the diagnostic's reference balance in the 96 ms attack window.
    for (dynamic, file) in [
        ("loud", "tone-v1.0-L1.json"),
        ("medium", "tone-v0.6-L3.json"),
        ("soft", "tone-v0.25-L5.json"),
    ] {
        let tone = read(&format!("g3-playable-diagnostic/{file}"));
        let attack = tone["tone_comparison"]["windows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["label"] == "attack_96_ms")
            .unwrap();
        let target = r["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["dynamic"] == dynamic)
            .unwrap();
        for h in target["harmonics"].as_array().unwrap() {
            let n = h["harmonic"].as_u64().unwrap();
            assert!((2..=5).contains(&n), "harmonic 6 and above are excluded");
            let measured = attack["harmonics"]
                .as_array()
                .unwrap()
                .iter()
                .find(|x| x["harmonic"] == n)
                .unwrap()["reference_relative_to_fundamental_db"]
                .as_f64()
                .unwrap();
            assert!((h["db_relative_to_fundamental"].as_f64().unwrap() - measured).abs() < 0.05);
        }
    }
    let geometries = r["geometries"].as_array().unwrap();
    assert_eq!(geometries.len(), 6 * 6 * (1 + 1 + 4));
    // The production law at the default geometry predicts the diagnostic's
    // engine balance at the engine's traced amplitudes within 0.5 dB.
    let default = geometries
        .iter()
        .find(|g| {
            g["law"] == "production_inverse_sqrt" && g["gap_mm"] == 1.5 && g["offset_mm"] == 0.5
        })
        .unwrap();
    let measured_engine: [&[f64]; 3] = [
        &[-3.3, -12.4, -32.8, -32.9],
        &[-10.6, -24.5, -56.2, -57.6],
        &[-21.6, -45.7],
    ];
    for (i, row) in default["at_engine_amplitudes"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let predicted = row["ratios_db_h2_to_h8"].as_array().unwrap();
        for (j, m) in measured_engine[i].iter().enumerate() {
            assert!((predicted[j].as_f64().unwrap() - m).abs() < 0.5, "{i} {j}");
        }
    }
    let best = r["best_per_law"].as_array().unwrap();
    assert_eq!(best.len(), 3);
    let aperture = best
        .iter()
        .find(|b| b["law"] == "finite_aperture_16_node")
        .unwrap();
    assert_eq!(aperture["gap_mm"], 0.5);
    assert_eq!(aperture["offset_mm"], 0.5);
    assert_eq!(aperture["pole_radius_mm"], 2.0);
    assert!(aperture["combined_rms_error_db"].as_f64().unwrap() < 2.5);
    assert_eq!(aperture["amplitudes_monotone_with_dynamics"], true);
    let engine = r["engine_amplitudes_mm"].as_array().unwrap();
    for (i, d) in aperture["dynamics"].as_array().unwrap().iter().enumerate() {
        let ratio = d["amplitude_m"].as_f64().unwrap() * 1e3 / engine[i].as_f64().unwrap();
        assert!(
            (ratio - 1.0).abs() < 0.15,
            "dynamic {i} amplitude ratio {ratio}"
        );
    }
    // Both point laws need more than twice the engine's loud amplitude.
    for law in ["production_inverse_sqrt", "point_pole_inverse_cube"] {
        let b = best.iter().find(|b| b["law"] == law).unwrap();
        let loud = b["dynamics"][0]["amplitude_m"].as_f64().unwrap() * 1e3;
        assert!(
            loud > 2.0 * engine[0].as_f64().unwrap(),
            "{law} loud {loud}"
        );
        assert!(
            b["combined_rms_error_db"].as_f64().unwrap()
                > aperture["combined_rms_error_db"].as_f64().unwrap()
        );
    }
    // Every best geometry reaches a loud third harmonic above the fundamental.
    for b in best {
        assert!(b["dynamics"][0]["ratios_db_h2_to_h8"][1].as_f64().unwrap() > 5.0);
    }
}

#[test]
fn pickup_harmonics_cli_rejects_invalid_arguments_and_preserves_outputs() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["pickup-harmonics"],
        vec!["pickup-harmonics", "--output", "keep.json"],
        vec!["pickup-harmonics", "--output", "bad.wav"],
        vec!["pickup-harmonics", "--output", "bad.json", "--extra"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists());
}

#[test]
fn playable_g3_diagnostic_receipts_hold_their_hashes_and_measured_deficits() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references/g3-playable-diagnostic");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let manifest = read("manifest.json");
    let files = manifest["files"].as_array().unwrap();
    assert_eq!(files.len(), 12);
    for entry in files {
        let name = entry["file"].as_str().unwrap();
        let bytes = fs::read(root.join(name)).unwrap();
        assert_eq!(
            bytes.len() as u64,
            entry["bytes"].as_u64().unwrap(),
            "{name}"
        );
        // SHA-256 computed here without a dependency.
        assert_eq!(
            sha256_hex(&bytes),
            entry["sha256"].as_str().unwrap(),
            "{name}"
        );
    }
    let mean = |track: &serde_json::Value, key: &str| -> f64 {
        let o = track["observations"].as_array().unwrap();
        o.iter().map(|x| x[key].as_f64().unwrap()).sum::<f64>() / o.len() as f64
    };
    let fundamental = |tracking: &serde_json::Value| -> serde_json::Value {
        tracking["tracks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| (mean(t, "frequency_hz") - 196.0).abs() < 5.0)
            .unwrap()
            .clone()
    };
    for (velocity, layer) in [("1.0", 1), ("0.6", 3), ("0.25", 5)] {
        let sustain = read(&format!("sustain-v{velocity}-L{layer}.json"));
        let p = &sustain["partial_comparison"];
        let reference = fundamental(&p["reference_tracking"]);
        let candidate = fundamental(&p["candidate_tracking"]);
        // The recording's fundamental decays near 2.5 dB/s; the engine's near 11.3 dB/s.
        for key in ["early_slope_db_per_second", "late_slope_db_per_second"] {
            let r = reference["decay"][key].as_f64().unwrap();
            let c = candidate["decay"][key].as_f64().unwrap();
            assert!((-3.0..=-2.0).contains(&r), "reference {key} {r}");
            assert!((-11.5..=-11.0).contains(&c), "candidate {key} {c}");
        }
        assert!((mean(&candidate, "frequency_hz") - 196.0).abs() < 0.5);
        assert!((mean(&reference, "frequency_hz") - 196.4).abs() < 0.5);
        if layer != 1 {
            assert_eq!(p["detection_complete"], true);
            let matches = p["matches"].as_array().unwrap();
            let fundamental_match = matches
                .iter()
                .find(|m| (m["mean_reference_frequency_hz"].as_f64().unwrap() - 196.0).abs() < 2.0)
                .unwrap();
            assert!(
                fundamental_match["mean_candidate_minus_reference_cents"]
                    .as_f64()
                    .unwrap()
                    .abs()
                    < 5.0
            );
            assert_eq!(fundamental_match["decay"]["status"], "qualified");
            let t60 = fundamental_match["decay"]["candidate_minus_reference_t60_seconds"]
                .as_f64()
                .unwrap();
            assert!(t60 < -15.0, "fundamental T60 difference {t60}");
        } else {
            assert_eq!(p["detection_complete"], false);
        }
        let tone = read(&format!("tone-v{velocity}-L{layer}.json"));
        let windows = tone["tone_comparison"]["windows"].as_array().unwrap();
        let attack = windows
            .iter()
            .find(|w| w["label"] == "attack_96_ms")
            .unwrap();
        let third = attack["harmonics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["harmonic"] == 3)
            .unwrap();
        let balance = third["candidate_minus_reference_balance_db"]
            .as_f64()
            .unwrap();
        match layer {
            1 => assert!(balance < -15.0, "loud third harmonic balance {balance}"),
            3 => assert!(balance < -8.0, "medium third harmonic balance {balance}"),
            _ => assert!(balance.abs() < 6.0, "soft third harmonic balance {balance}"),
        }
    }
}

fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    let bits = (data.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_be_bytes());
    for chunk in message.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[test]
fn midi_render_renders_a_small_file_with_receipt_and_rejects_invalid_or_existing_outputs() {
    let scratch = Scratch::new();
    // Format 0 file: 120 bpm, C4 for one quarter with sustain, then release.
    let mut midi = b"MThd".to_vec();
    midi.extend_from_slice(&6u32.to_be_bytes());
    midi.extend_from_slice(&0u16.to_be_bytes());
    midi.extend_from_slice(&1u16.to_be_bytes());
    midi.extend_from_slice(&96u16.to_be_bytes());
    let body: Vec<u8> = vec![
        0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20, // tempo 500000 us
        0x00, 0xB0, 64, 127, // sustain down
        0x00, 0x90, 60, 100, // C4 on
        0x60, 0x80, 60, 0, // C4 off after 96 ticks (0.5 s)
        0x60, 0xB0, 64, 0, // sustain up at 1.0 s
        0x00, 0xFF, 0x2F, 0x00,
    ];
    midi.extend_from_slice(b"MTrk");
    midi.extend_from_slice(&(body.len() as u32).to_be_bytes());
    midi.extend_from_slice(&body);
    fs::write(scratch.0.join("tiny.mid"), &midi).unwrap();
    fs::write(scratch.0.join("keep.wav"), b"preserve").unwrap();
    for args in [
        vec!["render-midi"],
        vec!["render-midi", "tiny.mid"],
        vec!["render-midi", "tiny.mid", "--output", "keep.wav"],
        vec!["render-midi", "tiny.mid", "--output", "bad.json"],
        vec![
            "render-midi",
            "tiny.mid",
            "--output",
            "bad.wav",
            "--gain",
            "9",
        ],
        vec![
            "render-midi",
            "tiny.mid",
            "--output",
            "bad.wav",
            "--tail",
            "40",
        ],
        vec![
            "render-midi",
            "tiny.mid",
            "--output",
            "bad.wav",
            "--unknown",
        ],
        vec!["render-midi", "missing.mid", "--output", "bad.wav"],
    ] {
        assert!(!scratch.run(&args).status.success(), "{args:?}");
    }
    assert_eq!(fs::read(scratch.0.join("keep.wav")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.wav").exists() && !scratch.0.join("bad.json").exists());
    scratch.success(&[
        "render-midi",
        "tiny.mid",
        "--output",
        "out.wav",
        "--normalize",
        "--tail",
        "0.5",
    ]);
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(scratch.0.join("out.json")).unwrap()).unwrap();
    assert_eq!(receipt["experiment"], "midi-render-v1");
    assert_eq!(receipt["notes_played"], 1);
    assert_eq!(receipt["sustain_events"], 2);
    assert_eq!(receipt["notes_outside_range_dropped"], 0);
    assert_eq!(receipt["faults"], 0);
    assert_eq!(receipt["sample_rate"], 48000);
    assert!((receipt["seconds"].as_f64().unwrap() - 1.5).abs() < 1e-4);
    let peak = receipt["peak"].as_f64().unwrap();
    assert!(peak > 0.0 && peak < 1.0);
    assert!((receipt["normalized_peak_dbfs"].as_f64().unwrap() + 1.0).abs() < 1e-12);
    assert!(scratch.0.join("out-norm.wav").exists());
    let wav = fs::read(scratch.0.join("out.wav")).unwrap();
    assert_eq!(&wav[..4], b"RIFF");
    // Reruns never overwrite.
    assert!(
        !scratch
            .run(&["render-midi", "tiny.mid", "--output", "out.wav"])
            .status
            .success()
    );
}

#[test]
fn loaded_damper_lift_receipt_traces_lift_geometry_and_replays_the_settled_hammer() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-damper-lift-validation.json");
    let prior = read("loaded-damper-seating-validation.json");
    assert_eq!(r["experiment"], "loaded-damper-lift-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    let attack = |first: &serde_json::Value, base: &serde_json::Value| -> bool {
        let a = first["entries"].as_array().unwrap();
        let b = base["entries"].as_array().unwrap();
        let impact_ok = ["impulse_n_s", "peak_force_n", "active_contact_seconds"]
            .iter()
            .all(|key| {
                let reference = base["impact"][key].as_f64().unwrap();
                reference > 0.0
                    && (first["impact"][key].as_f64().unwrap() / reference - 1.0).abs() < 0.05
            });
        a.len() == 1 && b.len() == 1 && impact_ok && {
            let va = a[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            let vb = b[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            (va / vb - 1.0).abs() < 0.05
                && (a[0]["seconds"].as_f64().unwrap() - b[0]["seconds"].as_f64().unwrap()).abs()
                    < 0.001
        }
    };
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let mut takes_seen = 0;
    let settled_felt = cases[1]["rows"][0]["takes"][1]["lift"]["rest"]["felt_force_n"]
        .as_f64()
        .unwrap();
    for (case_index, case) in cases.iter().enumerate() {
        // The control and the settled hammer replay the damper seating study.
        let old = &prior["cases"][if case_index == 0 { 0 } else { 1 }];
        let s = &case["settings"];
        assert_eq!(s["gravity_m_s2"], if case_index == 0 { 0.0 } else { 9.81 });
        assert_eq!(
            s["pedestal_rate_loss_s_m"],
            if case_index == 0 { 2.0 } else { 30.0 }
        );
        let ratio = [0.8, 0.8, 0.5, 0.8, 0.8, 0.5][case_index];
        let slack = [0.002, 0.002, 0.002, 0.004, 0.002, 0.002][case_index];
        let stiffness = [200.0, 200.0, 200.0, 200.0, 100.0, 100.0][case_index];
        assert_eq!(s["bridle_ratio"], ratio);
        assert_eq!(s["bridle_slack_m"], slack);
        assert_eq!(s["arm_stiffness_n_m"], stiffness);
        let soft = stiffness == 100.0;
        assert_eq!(s["damper_closed_m"] == 0.0002, !soft);
        let rows = case["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(row["measurement_qualified"], true);
            for key in [
                "convergence",
                "launch_convergence",
                "key_convergence",
                "letoff_convergence",
                "flight_qualification",
                "damper_convergence",
                "lift_convergence",
            ] {
                assert_eq!(row[key]["passed"], true);
            }
            let old_row = &old["rows"][index];
            assert_eq!(old_row["nominal_speed_m_s"], row["nominal_speed_m_s"]);
            let takes = row["takes"].as_array().unwrap();
            assert_eq!(takes.len(), 2);
            for (take, old_take) in takes.iter().zip(old_row["takes"].as_array().unwrap()) {
                takes_seen += 1;
                assert_eq!(take["passed"], true);
                if case_index <= 1 {
                    let mut replay = take.clone();
                    replay.as_object_mut().unwrap().remove("lift");
                    assert_eq!(replay, *old_take);
                    for key in ["convergence", "launch_convergence", "damper_convergence"] {
                        assert_eq!(row[key], old_row[key]);
                    }
                }
                let lift = &take["lift"];
                let rest = &lift["rest"];
                let felt = rest["felt_force_n"].as_f64().unwrap();
                assert!(felt > 0.0);
                if soft {
                    // Matched seating keeps the settled rest felt force.
                    assert!((felt / settled_felt - 1.0).abs() < 1e-6);
                }
                assert!(rest["bridle_compression_m"].as_f64().unwrap() < 0.0);
                for i in 0..2 {
                    let held = lift["held_max_felt_lift_m"][i].as_f64().unwrap();
                    assert!(held > 0.001 && held < 0.02);
                    assert!(lift["held_max_arm_speed_m_s"][i].as_f64().unwrap() > 0.0);
                    assert!(lift["held_min_felt_force_n"][i].as_f64().unwrap() >= 0.0);
                }
                // A smaller bridle ratio or a larger slack lifts the felt less.
                if case_index == 2 || case_index == 3 {
                    let settled = &cases[1]["rows"][index]["takes"]
                        [if take["steps_per_frame"] == 128 { 0 } else { 1 }]["lift"];
                    assert!(
                        lift["held_max_felt_lift_m"][0].as_f64().unwrap()
                            < settled["held_max_felt_lift_m"][0].as_f64().unwrap()
                    );
                }
                assert_eq!(take["damper"]["passed"], true);
            }
            let first = &takes[1]["repetition"]["phases"][0];
            let base = &cases[0]["rows"][index]["takes"][1]["repetition"]["phases"][0];
            assert_eq!(row["first_vs_control"]["passed"], attack(first, base));
            if case_index >= 2 {
                let settled = &cases[1]["rows"][index]["takes"][1]["repetition"]["phases"][0];
                assert_eq!(
                    row["first_vs_settled_hammer"]["passed"],
                    attack(first, settled)
                );
            } else {
                assert!(row["first_vs_settled_hammer"].is_null());
            }
        }
    }
    assert_eq!(takes_seen, 24);
}

#[test]
fn loaded_damper_lift_cli_preserves_outputs_and_rejects_extra_arguments() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-damper-lift"],
        vec!["loaded-damper-lift", "--output", "keep.json"],
        vec!["loaded-damper-lift", "--output", "bad.wav"],
        vec!["loaded-damper-lift", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists() && !scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_damper_seating_receipt_traces_felt_reseating_and_replays_the_settled_hammer() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-damper-seating-validation.json");
    let prior = read("loaded-landing-validation.json");
    assert_eq!(r["experiment"], "loaded-damper-seating-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    let attack = |first: &serde_json::Value, base: &serde_json::Value| -> bool {
        let a = first["entries"].as_array().unwrap();
        let b = base["entries"].as_array().unwrap();
        let impact_ok = ["impulse_n_s", "peak_force_n", "active_contact_seconds"]
            .iter()
            .all(|key| {
                let reference = base["impact"][key].as_f64().unwrap();
                reference > 0.0
                    && (first["impact"][key].as_f64().unwrap() / reference - 1.0).abs() < 0.05
            });
        a.len() == 1 && b.len() == 1 && impact_ok && {
            let va = a[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            let vb = b[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            (va / vb - 1.0).abs() < 0.05
                && (a[0]["seconds"].as_f64().unwrap() - b[0]["seconds"].as_f64().unwrap()).abs()
                    < 0.001
        }
    };
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let mut takes_seen = 0;
    for (case_index, case) in cases.iter().enumerate() {
        // The control and the settled hammer replay the landing study.
        let old = &prior["cases"][if case_index == 0 { 0 } else { 3 }];
        let s = &case["settings"];
        assert_eq!(s["gravity_m_s2"], if case_index == 0 { 0.0 } else { 9.81 });
        assert_eq!(
            s["pedestal_rate_loss_s_m"],
            if case_index == 0 { 2.0 } else { 30.0 }
        );
        assert_eq!(s["hammer_mass_kg"], 0.004);
        let felt_loss = [5.0, 5.0, 15.0, 40.0, 5.0, 5.0][case_index];
        let arm_damping = [0.5, 0.5, 0.5, 0.5, 2.0, 0.5][case_index];
        let arm_mass = [0.001, 0.001, 0.001, 0.001, 0.001, 0.002][case_index];
        assert_eq!(s["felt_rate_loss_s_m"], felt_loss);
        assert_eq!(s["arm_damping_n_s_m"], arm_damping);
        assert_eq!(s["arm_mass_kg"], arm_mass);
        let rows = case["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(row["measurement_qualified"], true);
            for key in [
                "convergence",
                "launch_convergence",
                "key_convergence",
                "letoff_convergence",
                "flight_qualification",
                "damper_convergence",
            ] {
                assert_eq!(row[key]["passed"], true);
            }
            let old_row = &old["rows"][index];
            assert_eq!(old_row["nominal_speed_m_s"], row["nominal_speed_m_s"]);
            let takes = row["takes"].as_array().unwrap();
            assert_eq!(takes.len(), 2);
            for (take, old_take) in takes.iter().zip(old_row["takes"].as_array().unwrap()) {
                takes_seen += 1;
                assert_eq!(take["passed"], true);
                if case_index <= 1 {
                    let mut replay = take.clone();
                    replay.as_object_mut().unwrap().remove("damper");
                    let mut old_replay = old_take.clone();
                    old_replay.as_object_mut().unwrap().remove("landing");
                    assert_eq!(replay, old_replay);
                    for key in ["convergence", "launch_convergence", "flight_convergence"] {
                        assert_eq!(row[key], old_row[key]);
                    }
                }
                let damper = &take["damper"];
                assert_eq!(damper["passed"], true);
                let windows = damper["windows"].as_array().unwrap();
                assert_eq!(windows.len(), 2);
                assert_eq!(windows[0]["key_up_seconds"], 0.15);
                assert_eq!(windows[1]["key_up_seconds"], 0.33);
                for w in windows {
                    assert_eq!(w["overflow"], false);
                    let events = w["events"].as_array().unwrap();
                    assert!(events.len() <= 32);
                    // The felt is lifted while the key is held, so the first event
                    // after key-up is an entry; events then alternate.
                    let mut expect_entry = true;
                    for event in events {
                        assert_eq!(event["kind"], if expect_entry { "entry" } else { "exit" });
                        expect_entry = !expect_entry;
                    }
                    let reseat = w["first_reseat_seconds_after_key_up"].as_f64().unwrap();
                    assert!(reseat > 0.0 && reseat < 0.06);
                    if let Some(settled) = w["settled_seconds_after_key_up"].as_f64() {
                        assert!(settled >= reseat - 1e-12);
                    }
                    let fraction = w["felt_contact_fraction"].as_f64().unwrap();
                    assert!((0.0..=1.0).contains(&fraction));
                    assert!(w["max_felt_lift_m"].as_f64().unwrap() > 0.0001);
                    assert!(w["reference_voltage_rms_v"].as_f64().unwrap() > 0.0);
                    let bins = w["output_level_db_per_ms"].as_array().unwrap();
                    assert!(bins.len() >= 59);
                    let t20 = w["onset_20db_seconds_after_key_up"].as_f64();
                    let t40 = w["onset_40db_seconds_after_key_up"].as_f64();
                    if let Some(t40) = t40 {
                        assert!(t20.is_some_and(|t20| t20 <= t40));
                    }
                    // The retained onsets agree with the retained level bins.
                    if let Some(t20) = t20 {
                        let bin = (t20 / 0.001).round() as usize;
                        assert!(bins[bin - 1].as_f64().unwrap() <= -20.0);
                        assert!(bins[..bin - 1].iter().all(|d| d.as_f64().unwrap() > -20.0));
                    }
                }
                // Readiness includes a seated felt for 90% of the last 20 ms.
                let ready = take["repetition"]["readiness"]["passed"] == true;
                if ready {
                    assert!(
                        take["repetition"]["readiness"]["felt_contact_fraction"]
                            .as_f64()
                            .unwrap()
                            >= 0.9
                    );
                }
            }
            let first = &takes[1]["repetition"]["phases"][0];
            let base = &cases[0]["rows"][index]["takes"][1]["repetition"]["phases"][0];
            assert_eq!(row["first_vs_control"]["passed"], attack(first, base));
            if case_index >= 2 {
                let settled = &cases[1]["rows"][index]["takes"][1]["repetition"]["phases"][0];
                assert_eq!(
                    row["first_vs_settled_hammer"]["passed"],
                    attack(first, settled)
                );
            } else {
                assert!(row["first_vs_settled_hammer"].is_null());
            }
        }
    }
    assert_eq!(takes_seen, 24);
}

#[test]
fn loaded_damper_seating_cli_preserves_outputs_and_rejects_extra_arguments() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-damper-seating"],
        vec!["loaded-damper-seating", "--output", "keep.json"],
        vec!["loaded-damper-seating", "--output", "bad.wav"],
        vec!["loaded-damper-seating", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists() && !scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_landing_receipt_traces_reseating_and_replays_the_control() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-landing-validation.json");
    let prior = read("loaded-gravity-validation.json");
    assert_eq!(r["experiment"], "loaded-landing-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    let attack = |first: &serde_json::Value, base: &serde_json::Value| -> bool {
        let a = first["entries"].as_array().unwrap();
        let b = base["entries"].as_array().unwrap();
        let impact_ok = ["impulse_n_s", "peak_force_n", "active_contact_seconds"]
            .iter()
            .all(|key| {
                let reference = base["impact"][key].as_f64().unwrap();
                reference > 0.0
                    && (first["impact"][key].as_f64().unwrap() / reference - 1.0).abs() < 0.05
            });
        a.len() == 1 && b.len() == 1 && impact_ok && {
            let va = a[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            let vb = b[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            (va / vb - 1.0).abs() < 0.05
                && (a[0]["seconds"].as_f64().unwrap() - b[0]["seconds"].as_f64().unwrap()).abs()
                    < 0.001
        }
    };
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let mut takes_seen = 0;
    for (case_index, case) in cases.iter().enumerate() {
        // The control and the weighted 4 g case replay the gravity study.
        let old = &prior["cases"][if case_index == 0 { 0 } else { 1 }];
        let s = &case["settings"];
        assert_eq!(s["gravity_m_s2"], if case_index == 0 { 0.0 } else { 9.81 });
        assert_eq!(s["hammer_mass_kg"], 0.004);
        let loss = [2.0, 2.0, 10.0, 30.0, 2.0, 30.0][case_index];
        let damping = [0.025, 0.025, 0.025, 0.025, 0.1, 0.1][case_index];
        assert_eq!(s["pedestal_rate_loss_s_m"], loss);
        assert_eq!(s["return_damping_n_s_m"], damping);
        let rows = case["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(row["measurement_qualified"], true);
            for key in [
                "convergence",
                "launch_convergence",
                "key_convergence",
                "letoff_convergence",
                "flight_qualification",
                "landing_convergence",
            ] {
                assert_eq!(row[key]["passed"], true);
            }
            let old_row = &old["rows"][index];
            assert_eq!(old_row["nominal_speed_m_s"], row["nominal_speed_m_s"]);
            let takes = row["takes"].as_array().unwrap();
            assert_eq!(takes.len(), 2);
            for (take, old_take) in takes.iter().zip(old_row["takes"].as_array().unwrap()) {
                takes_seen += 1;
                assert_eq!(take["passed"], true);
                if case_index <= 1 {
                    let mut replay = take.clone();
                    replay.as_object_mut().unwrap().remove("landing");
                    let mut old_replay = old_take.clone();
                    old_replay.as_object_mut().unwrap().remove("gravity");
                    assert_eq!(replay, old_replay);
                    for key in ["convergence", "launch_convergence", "flight_convergence"] {
                        assert_eq!(row[key], old_row[key]);
                    }
                }
                let landing = &take["landing"];
                assert_eq!(landing["passed"], true);
                let windows = landing["windows"].as_array().unwrap();
                assert_eq!(windows.len(), 2);
                assert_eq!(windows[0]["key_up_seconds"], 0.15);
                assert_eq!(windows[0]["end_seconds"], 0.21);
                assert_eq!(windows[1]["key_up_seconds"], 0.33);
                assert_eq!(windows[1]["end_seconds"], 0.4);
                for w in windows {
                    assert_eq!(w["overflow"], false);
                    let events = w["events"].as_array().unwrap();
                    assert!(events.len() <= 32);
                    // The hammer rests on the held pedestal at key-up, so the first event
                    // is an exit; events then alternate.
                    let mut expect_entry = false;
                    for event in events {
                        assert_eq!(event["kind"], if expect_entry { "entry" } else { "exit" });
                        expect_entry = !expect_entry;
                    }
                    if let Some(t) = w["first_reseat_seconds_after_key_up"].as_f64() {
                        assert!(t > 0.0 && t < 0.07);
                        let landing_speed = w["landing_speed_m_s"].as_f64().unwrap();
                        let rebound = w["rebound_speed_m_s"].as_f64().unwrap();
                        assert!(landing_speed > 0.0 && rebound >= 0.0);
                        assert!(
                            (w["restitution"].as_f64().unwrap() - rebound / landing_speed).abs()
                                < 1e-12
                        );
                        // The first reseat happens with the pedestal back at rest.
                        let reseat = events
                            .iter()
                            .find(|e| {
                                e["kind"] == "entry"
                                    && (e["seconds"].as_f64().unwrap() - 0.15 - t).abs() < 1e-12
                                    || e["kind"] == "entry"
                                        && (e["seconds"].as_f64().unwrap() - 0.33 - t).abs() < 1e-12
                            })
                            .unwrap();
                        assert_eq!(reseat["pedestal_position_m"], -0.012);
                    } else {
                        assert!(w["landing_speed_m_s"].is_null());
                        assert_eq!(w["pedestal_exits_after_reseat"], 0);
                    }
                    if let Some(settled) = w["settled_seconds_after_key_up"].as_f64() {
                        let reseat = w["first_reseat_seconds_after_key_up"].as_f64().unwrap();
                        assert!(settled >= reseat - 1e-12);
                    }
                    assert!(w["lowest_hammer_position_m"].as_f64().unwrap() >= -0.0125);
                }
                // Readiness requires a settled hammer before the repeat command.
                let ready = take["repetition"]["readiness"]["passed"] == true;
                if ready {
                    assert!(
                        windows[0]["settled_seconds_after_key_up"]
                            .as_f64()
                            .is_some()
                    );
                }
            }
            let first = &takes[1]["repetition"]["phases"][0];
            let base = &cases[0]["rows"][index]["takes"][1]["repetition"]["phases"][0];
            assert_eq!(row["first_vs_control"]["passed"], attack(first, base));
        }
    }
    assert_eq!(takes_seen, 24);
}

#[test]
fn loaded_landing_cli_preserves_outputs_and_rejects_extra_arguments() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-landing"],
        vec!["loaded-landing", "--output", "keep.json"],
        vec!["loaded-landing", "--output", "bad.wav"],
        vec!["loaded-landing", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists() && !scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_gravity_receipt_seats_weighted_rests_and_replays_the_flight_control() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-gravity-validation.json");
    let prior = read("loaded-flight-budget-validation.json");
    assert_eq!(r["experiment"], "loaded-gravity-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    let attack = |first: &serde_json::Value, base: &serde_json::Value| -> bool {
        let a = first["entries"].as_array().unwrap();
        let b = base["entries"].as_array().unwrap();
        let impact_ok = ["impulse_n_s", "peak_force_n", "active_contact_seconds"]
            .iter()
            .all(|key| {
                let reference = base["impact"][key].as_f64().unwrap();
                reference > 0.0
                    && (first["impact"][key].as_f64().unwrap() / reference - 1.0).abs() < 0.05
            });
        a.len() == 1 && b.len() == 1 && impact_ok && {
            let va = a[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            let vb = b[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            (va / vb - 1.0).abs() < 0.05
                && (a[0]["seconds"].as_f64().unwrap() - b[0]["seconds"].as_f64().unwrap()).abs()
                    < 0.001
        }
    };
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let old = &prior["cases"][0];
    let mut takes_seen = 0;
    for (case_index, case) in cases.iter().enumerate() {
        let s = &case["settings"];
        let g = s["gravity_m_s2"].as_f64().unwrap();
        assert_eq!(g, if case_index == 0 { 0.0 } else { 9.81 });
        let mass = s["hammer_mass_kg"].as_f64().unwrap();
        assert_eq!(mass, [0.004, 0.004, 0.008, 0.012, 0.012, 0.012][case_index]);
        assert!((s["hammer_weight_n"].as_f64().unwrap() - mass * g).abs() < 1e-15);
        assert_eq!(
            s["return_stiffness_n_m"],
            if case_index == 5 { 2.0 } else { 4.0 }
        );
        let base = 0.0002
            + if case_index == 4 {
                0.001 * 9.81 / 200.0
            } else {
                0.0
            };
        assert!((s["damper_closed_m"].as_f64().unwrap() - base).abs() < 1e-15);
        let rows = case["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(row["measurement_qualified"], true);
            for key in [
                "convergence",
                "launch_convergence",
                "key_convergence",
                "letoff_convergence",
                "flight_qualification",
                "gravity_convergence",
            ] {
                assert_eq!(row[key]["passed"], true);
            }
            for launch in row["flight_convergence"]["launches"].as_array().unwrap() {
                assert_eq!(launch["release_to_impact"]["passed"], true);
            }
            let old_row = &old["rows"][index];
            assert_eq!(old_row["nominal_speed_m_s"], row["nominal_speed_m_s"]);
            let takes = row["takes"].as_array().unwrap();
            assert_eq!(takes.len(), 2);
            for (take, old_take) in takes.iter().zip(old_row["takes"].as_array().unwrap()) {
                takes_seen += 1;
                assert_eq!(take["passed"], true);
                if case_index == 0 {
                    let mut replay = take.clone();
                    replay.as_object_mut().unwrap().remove("gravity");
                    assert_eq!(replay, *old_take);
                    for key in ["convergence", "launch_convergence", "flight_convergence"] {
                        assert_eq!(row[key], old_row[key]);
                    }
                }
                let gr = &take["gravity"];
                let rest = &gr["rest"];
                let sag = rest["hammer_sag_m"].as_f64().unwrap();
                let pedestal = rest["pedestal_force_n"].as_f64().unwrap();
                let weight = rest["hammer_weight_n"].as_f64().unwrap();
                assert!((weight - mass * g).abs() < 1e-15);
                if case_index == 0 {
                    assert_eq!(sag, 0.0);
                    assert_eq!(pedestal, 0.0);
                    assert_eq!(rest["arm_weight_n"], 0.0);
                } else {
                    // The hammer sags into the pedestal, which carries the weight less
                    // the sagged return spring; the arm sags against its own spring.
                    assert!(sag < 0.0 && sag > -1e-4);
                    let expected = weight + s["return_stiffness_n_m"].as_f64().unwrap() * sag;
                    assert!((pedestal - expected).abs() < 1e-9 * expected);
                    assert!((rest["arm_weight_n"].as_f64().unwrap() - 0.001 * 9.81).abs() < 1e-15);
                }
                assert!(rest["felt_force_n"].as_f64().unwrap() > 0.0);
                assert!(gr["minimum_felt_force_key_up_n"].as_f64().unwrap() >= 0.0);
                let returns = gr["returns"].as_array().unwrap();
                assert_eq!(returns.len(), 2);
                assert_eq!(returns[0]["key_up_seconds"], 0.15);
                assert_eq!(returns[1]["key_up_seconds"], 0.33);
                for ret in returns {
                    if let Some(t) = ret["reseated_seconds_after_key_up"].as_f64() {
                        assert!(t > 0.0 && t < 0.2);
                    }
                    // Landing compresses the pedestal by up to a few tenths of a millimetre.
                    assert!(ret["lowest_hammer_position_m"].as_f64().unwrap() >= -0.0125);
                }
                let at_repeat = &returns[0]["at_repeat"];
                assert!(at_repeat["hammer_position_m"].as_f64().unwrap() <= -0.0015);
                // Flight identities close with the gravitational potential included.
                for launch in take["flight"]["launches"].as_array().unwrap() {
                    assert_eq!(launch["passed"], true);
                    for key in ["release_to_impact", "release_to_window_end"] {
                        let b = &launch[key];
                        if b.is_null() {
                            continue;
                        }
                        assert!(b["relative_hammer_defect"].as_f64().unwrap() < 1e-8);
                        assert_eq!(b["gravity_potential_change_j"].is_null(), case_index == 0);
                        if case_index > 0 {
                            let distance = b["distance_m"].as_f64().unwrap();
                            let expected = mass * g * distance;
                            assert!(
                                (b["gravity_potential_change_j"].as_f64().unwrap() - expected)
                                    .abs()
                                    < 1e-12
                            );
                        }
                    }
                }
            }
            let first = &takes[1]["repetition"]["phases"][0];
            let base = &cases[0]["rows"][index]["takes"][1]["repetition"]["phases"][0];
            assert_eq!(row["first_vs_control"]["passed"], attack(first, base));
            if case_index >= 4 {
                let heavy = &cases[3]["rows"][index]["takes"][1]["repetition"]["phases"][0];
                assert_eq!(row["first_vs_gravity_12g"]["passed"], attack(first, heavy));
            } else {
                assert!(row["first_vs_gravity_12g"].is_null());
            }
        }
    }
    assert_eq!(takes_seen, 24);
}

#[test]
fn loaded_gravity_cli_preserves_outputs_and_rejects_extra_arguments() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-gravity"],
        vec!["loaded-gravity", "--output", "keep.json"],
        vec!["loaded-gravity", "--output", "bad.wav"],
        vec!["loaded-gravity", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists() && !scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_flight_budget_receipt_closes_release_to_impact_identities_and_replays_control() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-flight-budget-validation.json");
    let prior = read("loaded-letoff-validation.json");
    assert_eq!(r["experiment"], "loaded-flight-budget-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    let attack = |first: &serde_json::Value, base: &serde_json::Value| -> bool {
        let a = first["entries"].as_array().unwrap();
        let b = base["entries"].as_array().unwrap();
        let impact_ok = ["impulse_n_s", "peak_force_n", "active_contact_seconds"]
            .iter()
            .all(|key| {
                let reference = base["impact"][key].as_f64().unwrap();
                reference > 0.0
                    && (first["impact"][key].as_f64().unwrap() / reference - 1.0).abs() < 0.05
            });
        a.len() == 1 && b.len() == 1 && impact_ok && {
            let va = a[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            let vb = b[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            (va / vb - 1.0).abs() < 0.05
                && (a[0]["seconds"].as_f64().unwrap() - b[0]["seconds"].as_f64().unwrap()).abs()
                    < 0.001
        }
    };
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let old = prior["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "baseline")
        .unwrap();
    let mut takes_seen = 0;
    for (case_index, case) in cases.iter().enumerate() {
        let s = &case["settings"];
        let mass = s["hammer_mass_kg"].as_f64().unwrap();
        assert_eq!(mass, [0.004, 0.008, 0.012, 0.004, 0.004, 0.004][case_index]);
        assert_eq!(
            s["arm_damping_n_s_m"],
            if case_index == 3 { 0.25 } else { 0.5 }
        );
        assert_eq!(
            s["bridle_rate_loss_s_m"],
            if case_index == 4 { 1.0 } else { 2.0 }
        );
        assert_eq!(
            s["arm_mass_kg"],
            if case_index == 5 { 0.0005 } else { 0.001 }
        );
        let rows = case["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(row["measurement_qualified"], true);
            for key in [
                "convergence",
                "launch_convergence",
                "key_convergence",
                "letoff_convergence",
                "flight_convergence",
            ] {
                assert_eq!(row[key]["passed"], true);
            }
            let old_row = &old["drivers"][1]["rows"][index];
            assert_eq!(old_row["nominal_speed_m_s"], row["nominal_speed_m_s"]);
            let takes = row["takes"].as_array().unwrap();
            assert_eq!(takes.len(), 2);
            for (take, old_take) in takes.iter().zip(old_row["takes"].as_array().unwrap()) {
                takes_seen += 1;
                assert_eq!(take["passed"], true);
                assert_eq!(take["driver"]["shape"], "key_letoff_sharp");
                if case_index == 0 {
                    let mut replay = take.clone();
                    replay.as_object_mut().unwrap().remove("flight");
                    assert_eq!(replay, *old_take);
                    for key in ["convergence", "launch_convergence", "key_convergence"] {
                        assert_eq!(row[key], old_row[key]);
                    }
                }
                let flight = &take["flight"];
                assert_eq!(flight["passed"], true);
                let launches = flight["launches"].as_array().unwrap();
                assert_eq!(launches.len(), 2);
                for (launch, start) in launches.iter().zip([0.03, 0.21]) {
                    assert_eq!(launch["passed"], true);
                    assert_eq!(launch["released"], true);
                    assert_eq!(launch["observed"], true);
                    assert_eq!(launch["start_seconds"], start);
                    let release = launch["release_seconds"].as_f64().unwrap();
                    assert!(release > start + 0.005 && release < start + 0.03);
                    let speed = launch["release_speed_m_s"].as_f64().unwrap();
                    assert!(speed > 0.5 && speed < 2.0);
                    // Release near the let-off top lies within the contact compression; a
                    // collision launch below it is retained as functional evidence.
                    let position = launch["release_position_m"].as_f64().unwrap();
                    assert!(position <= -0.0015);
                    assert_eq!(launch["released_at_letoff"], position >= -0.0021);
                    if launch["released_at_letoff"] == true {
                        assert!(position > -0.0018);
                    }
                    for d in launch["max_coupling_defects"].as_array().unwrap() {
                        assert!(d.as_f64().unwrap() < 1e-8);
                    }
                    let check = |b: &serde_json::Value, pre_impact: bool| {
                        let terms: Vec<f64> = b["terms_j"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|x| x.as_f64().unwrap())
                            .collect();
                        let coupling: Vec<f64> = b["coupling_j"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|x| x.as_f64().unwrap())
                            .collect();
                        let from = b["release_kinetic_j"].as_f64().unwrap();
                        let to = b["arrival_kinetic_j"].as_f64().unwrap();
                        let sum: f64 = terms.iter().sum();
                        let scale = from + terms.iter().map(|x| x.abs()).sum::<f64>();
                        assert!((from - to - sum).abs() <= 1e-8 * scale);
                        let coupling_scale =
                            terms[0].abs() + coupling.iter().map(|x| x.abs()).sum::<f64>();
                        assert!(
                            (terms[0] - coupling[0] - coupling[1] - coupling[2]).abs()
                                <= 1e-8 * coupling_scale
                        );
                        assert!(
                            (coupling[2] - coupling[3] - coupling[4] - coupling[5]).abs()
                                <= 1e-8 * coupling_scale
                        );
                        assert!((from - 0.5 * mass * speed * speed).abs() < 1e-12);
                        for key in [
                            "relative_hammer_defect",
                            "relative_bridle_defect",
                            "relative_arm_defect",
                        ] {
                            assert!(b[key].as_f64().unwrap() < 1e-8);
                        }
                        // Before impact the hammer is in free flight: no pedestal or tine work.
                        if pre_impact {
                            assert_eq!(terms[3], 0.0);
                            assert_eq!(terms[4], 0.0);
                        }
                        assert!(b["flight_seconds"].as_f64().unwrap() > 0.0);
                    };
                    check(&launch["release_to_window_end"], false);
                    if launch["struck"] == true {
                        let flight = &launch["release_to_impact"];
                        check(flight, true);
                        assert!(flight["arrival_speed_m_s"].as_f64().unwrap() > 0.0);
                        assert!(flight["distance_m"].as_f64().unwrap() > 0.001);
                        assert!(flight["bridle_share_of_release_kinetic"].as_f64().unwrap() > 0.0);
                    } else {
                        assert!(launch["release_to_impact"].is_null());
                    }
                }
                // A strike in the first phase implies a struck first launch and vice versa.
                let struck = launches[0]["struck"] == true;
                let entries = take["repetition"]["phases"][0]["entries"]
                    .as_array()
                    .unwrap();
                assert_eq!(struck, !entries.is_empty());
            }
            let first = &takes[1]["repetition"]["phases"][0];
            let base = &cases[0]["rows"][index]["takes"][1]["repetition"]["phases"][0];
            assert_eq!(row["first_vs_control"]["passed"], attack(first, base));
        }
    }
    assert_eq!(takes_seen, 24);
}

#[test]
fn loaded_flight_budget_cli_preserves_outputs_and_rejects_extra_arguments() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-flight-budget"],
        vec!["loaded-flight-budget", "--output", "keep.json"],
        vec!["loaded-flight-budget", "--output", "bad.wav"],
        vec!["loaded-flight-budget", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists() && !scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_letoff_receipt_maps_the_pedestal_and_replays_prescribed_controls() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-letoff-validation.json");
    let prior = read("loaded-key-inertia-validation.json");
    assert_eq!(r["experiment"], "loaded-letoff-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    let attack = |first: &serde_json::Value, base: &serde_json::Value| -> bool {
        let a = first["entries"].as_array().unwrap();
        let b = base["entries"].as_array().unwrap();
        let impact_ok = ["impulse_n_s", "peak_force_n", "active_contact_seconds"]
            .iter()
            .all(|key| {
                let reference = base["impact"][key].as_f64().unwrap();
                reference > 0.0
                    && (first["impact"][key].as_f64().unwrap() / reference - 1.0).abs() < 0.05
            });
        a.len() == 1 && b.len() == 1 && impact_ok && {
            let va = a[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            let vb = b[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            (va / vb - 1.0).abs() < 0.05
                && (a[0]["seconds"].as_f64().unwrap() - b[0]["seconds"].as_f64().unwrap()).abs()
                    < 0.001
        }
    };
    let profiles = r["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 2);
    let mut takes_seen = 0;
    for profile in profiles {
        let old = prior["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == profile["name"])
            .unwrap();
        let drivers = profile["drivers"].as_array().unwrap();
        assert_eq!(drivers.len(), 4);
        for (shape, driver) in drivers.iter().enumerate() {
            let rows = driver["rows"].as_array().unwrap();
            assert_eq!(rows.len(), 2);
            for (index, row) in rows.iter().enumerate() {
                assert_eq!(row["measurement_qualified"], true);
                for key in [
                    "convergence",
                    "launch_convergence",
                    "key_convergence",
                    "letoff_convergence",
                ] {
                    assert_eq!(row[key]["passed"], true);
                }
                assert_eq!(row["letoff_convergence"]["prescribed"], shape == 0);
                let old_row = &old["drivers"][0]["rows"][index];
                assert_eq!(old_row["nominal_speed_m_s"], row["nominal_speed_m_s"]);
                let speed = row["nominal_speed_m_s"].as_f64().unwrap();
                let takes = row["takes"].as_array().unwrap();
                assert_eq!(takes.len(), 2);
                for (take, old_take) in takes.iter().zip(old_row["takes"].as_array().unwrap()) {
                    takes_seen += 1;
                    assert_eq!(take["passed"], true);
                    let d = &take["driver"];
                    assert_eq!(d["shape"], driver["name"]);
                    if shape == 0 {
                        // Same window as the key study: the control replays it exactly.
                        assert!(take["key"].is_null());
                        assert_eq!(take, old_take);
                        assert_eq!(row["convergence"], old_row["convergence"]);
                        assert_eq!(row["launch_convergence"], old_row["launch_convergence"]);
                        continue;
                    }
                    let top = if shape == 3 { -0.0025 } else { -0.0015 };
                    let band = if shape == 2 { 0.0006 } else { 0.0 };
                    let start = if shape == 2 {
                        top - 2.0 * band / 3.0
                    } else {
                        top
                    };
                    assert!((d["letoff_top_m"].as_f64().unwrap() - top).abs() < 1e-15);
                    assert_eq!(d["letoff_band_m"], band);
                    assert!((d["letoff_start_m"].as_f64().unwrap() - start).abs() < 1e-15);
                    assert!((d["key_bed_m"].as_f64().unwrap() + 0.0005).abs() < 1e-15);
                    let travel = start + 0.012;
                    assert!((d["letoff_travel_m"].as_f64().unwrap() - travel).abs() < 1e-15);
                    let k = &take["key"];
                    assert_eq!(k["passed"], true);
                    assert_eq!(k["mass_kg"], 0.05);
                    assert!(k["bed"].is_null());
                    let finger = 1.0 + 0.05 * speed * speed / (2.0 * travel);
                    assert!((k["finger_force_n"].as_f64().unwrap() - finger).abs() < 1e-12);
                    assert!(k["max_key_speed_m_s"].as_f64().unwrap() < 2.0);
                    assert!(k["max_tracking_error_m"].as_f64().unwrap() < 1e-9);
                    assert!(k["max_relative_energy_defect"].as_f64().unwrap() < 1e-8);
                    assert!(k["max_relative_pedestal_work_lag"].as_f64().unwrap() < 1e-3);
                    assert_eq!(k["hard_limit_engaged"], false);
                    assert!((k["travel_end_m"].as_f64().unwrap() + 0.0005).abs() < 1e-15);
                    assert!((k["letoff"]["top_m"].as_f64().unwrap() - top).abs() < 1e-15);
                    let gestures = k["gestures"].as_array().unwrap();
                    assert_eq!(gestures.len(), 2);
                    for (g, start_seconds) in gestures.iter().zip([0.03, 0.21]) {
                        assert_eq!(g["start_seconds"], start_seconds);
                        let l = &g["letoff"];
                        let begin = l["begin_seconds_after_key_down"].as_f64().unwrap();
                        let complete = l["complete_seconds_after_key_down"].as_f64().unwrap();
                        let arrival = g["arrival_seconds_after_key_down"].as_f64().unwrap();
                        // Let-off precedes the key bed; a sharp let-off completes at once.
                        assert!(begin > 0.0 && begin <= complete && complete < arrival);
                        if band == 0.0 {
                            assert_eq!(begin, complete);
                        }
                        let key_speed = l["key_speed_at_begin_m_s"].as_f64().unwrap();
                        assert!(key_speed > 0.0 && key_speed < speed * (1.0 + 1e-9));
                        let hammer = &l["hammer_at_begin"];
                        assert!(hammer["hammer_velocity_m_s"].as_f64().unwrap() > 0.0);
                        assert!(hammer["hammer_position_m"].as_f64().unwrap() <= top + 1e-4);
                        let after = l["finger_work_after_begin_j"].as_f64().unwrap();
                        assert!(after > 0.0);
                        let w = &g["window"];
                        assert!(w["relative_defect"].as_f64().unwrap() < 1e-8);
                        let t: Vec<f64> = w["terms_j"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|x| x.as_f64().unwrap())
                            .collect();
                        assert!(after < t[0]);
                        let sum: f64 = t[..7].iter().sum();
                        assert!(
                            (sum - t[7]).abs() <= 1e-8 * t.iter().map(|x| x.abs()).sum::<f64>()
                        );
                        assert!(t[0] > 0.0 && t[1] < 0.0 && t[2] < 0.0 && t[5] < 0.0);
                        assert!(t[3] == 0.0 && t[4] == 0.0);
                        assert!((w["end_position_m"].as_f64().unwrap() + 0.0005).abs() < 1e-15);
                    }
                    let recoveries = k["recoveries"].as_array().unwrap();
                    assert_eq!(recoveries.len(), 2);
                    let landing = recoveries[0]["landing_seconds_after_key_up"]
                        .as_f64()
                        .unwrap();
                    assert!(landing > 0.0 && landing < 0.06);
                    assert_eq!(recoveries[0]["window"]["end_position_m"], -0.012);
                    // The pedestal never exceeds the let-off top in the assembly.
                    for launch in take["launches"].as_array().unwrap() {
                        for event in launch["events"].as_array().unwrap() {
                            if event["port"] == 2 {
                                let position = event["hammer_position_m"][1].as_f64().unwrap();
                                assert!(position <= top + 0.0002);
                            }
                        }
                    }
                }
                let first = &takes[1]["repetition"]["phases"][0];
                let base = &drivers[0]["rows"][index]["takes"][1]["repetition"]["phases"][0];
                assert_eq!(row["first_vs_original"]["passed"], attack(first, base));
                if shape >= 2 {
                    let sharp = &drivers[1]["rows"][index]["takes"][1]["repetition"]["phases"][0];
                    assert_eq!(row["first_vs_sharp_letoff"]["passed"], attack(first, sharp));
                } else {
                    assert!(row["first_vs_sharp_letoff"].is_null());
                }
            }
        }
    }
    assert_eq!(takes_seen, 32);
}

#[test]
fn loaded_letoff_cli_preserves_outputs_and_rejects_extra_arguments() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-letoff"],
        vec!["loaded-letoff", "--output", "keep.json"],
        vec!["loaded-letoff", "--output", "bad.wav"],
        vec!["loaded-letoff", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists() && !scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_key_inertia_receipt_closes_key_energy_and_replays_prescribed_controls() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-key-inertia-validation.json");
    let prior = read("loaded-drive-onset-validation.json");
    assert_eq!(r["experiment"], "loaded-key-inertia-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    let terms = r["key_term_order"].as_array().unwrap();
    assert_eq!(terms.len(), 8);
    let attack = |first: &serde_json::Value, base: &serde_json::Value| -> bool {
        let a = first["entries"].as_array().unwrap();
        let b = base["entries"].as_array().unwrap();
        let impact_ok = ["impulse_n_s", "peak_force_n", "active_contact_seconds"]
            .iter()
            .all(|key| {
                let reference = base["impact"][key].as_f64().unwrap();
                reference > 0.0
                    && (first["impact"][key].as_f64().unwrap() / reference - 1.0).abs() < 0.05
            });
        a.len() == 1 && b.len() == 1 && impact_ok && {
            let va = a[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            let vb = b[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            (va / vb - 1.0).abs() < 0.05
                && (a[0]["seconds"].as_f64().unwrap() - b[0]["seconds"].as_f64().unwrap()).abs()
                    < 0.001
        }
    };
    let profiles = r["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 2);
    let mut takes_seen = 0;
    for profile in profiles {
        let old = prior["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == profile["name"])
            .unwrap();
        let drivers = profile["drivers"].as_array().unwrap();
        assert_eq!(drivers.len(), 4);
        for (shape, driver) in drivers.iter().enumerate() {
            let rows = driver["rows"].as_array().unwrap();
            assert_eq!(rows.len(), 2);
            for (index, row) in rows.iter().enumerate() {
                assert_eq!(row["measurement_qualified"], true);
                for key in ["convergence", "launch_convergence", "key_convergence"] {
                    assert_eq!(row[key]["passed"], true);
                }
                assert_eq!(row["key_convergence"]["prescribed"], shape == 0);
                let old_row = &old["drivers"][0]["rows"][index];
                assert_eq!(old_row["nominal_speed_m_s"], row["nominal_speed_m_s"]);
                let speed = row["nominal_speed_m_s"].as_f64().unwrap();
                let takes = row["takes"].as_array().unwrap();
                assert_eq!(takes.len(), 2);
                for (take, old_take) in takes.iter().zip(old_row["takes"].as_array().unwrap()) {
                    takes_seen += 1;
                    assert_eq!(take["passed"], true);
                    assert_eq!(take["driver"]["shape"], driver["name"]);
                    assert_eq!(
                        take["driver"]["nominal_speed_m_s"],
                        row["nominal_speed_m_s"]
                    );
                    for launch in take["launches"].as_array().unwrap() {
                        let start = launch["start_seconds"].as_f64().unwrap();
                        assert!(
                            (launch["end_seconds"].as_f64().unwrap() - start - 0.03).abs() < 1e-9
                        );
                    }
                    if shape == 0 {
                        assert!(take["key"].is_null());
                        // Everything except the longer launch window replays the prior receipt.
                        let mut replay = take.clone();
                        let object = replay.as_object_mut().unwrap();
                        object.remove("driver");
                        object.remove("key");
                        let launches = object.remove("launches").unwrap();
                        let mut old_replay = old_take.clone();
                        let old_object = old_replay.as_object_mut().unwrap();
                        old_object.remove("driver");
                        old_object.remove("drive_segments");
                        let old_launches = old_object.remove("launches").unwrap();
                        assert_eq!(replay, old_replay);
                        assert_eq!(row["convergence"], old_row["convergence"]);
                        for (new, old) in launches
                            .as_array()
                            .unwrap()
                            .iter()
                            .zip(old_launches.as_array().unwrap())
                        {
                            assert_eq!(new["before_contact"], old["before_contact"]);
                            assert_eq!(new["initial"], old["initial"]);
                            let cutoff = old["end_seconds"].as_f64().unwrap();
                            let prefix = |l: &serde_json::Value| {
                                l["events"]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .filter(|e| e["seconds"].as_f64().unwrap() <= cutoff)
                                    .cloned()
                                    .collect::<Vec<_>>()
                            };
                            assert_eq!(prefix(new), prefix(old));
                        }
                        continue;
                    }
                    let k = &take["key"];
                    assert_eq!(k["passed"], true);
                    let mass = k["mass_kg"].as_f64().unwrap();
                    assert_eq!(mass, if shape == 3 { 0.1 } else { 0.05 });
                    assert_eq!(k["return_force_n"], 1.0);
                    let length = 0.0105;
                    let finger = 1.0 + mass * speed * speed / (2.0 * length);
                    assert!((k["finger_force_n"].as_f64().unwrap() - finger).abs() < 1e-12);
                    assert_eq!(k["nominal_free_arrival_speed_m_s"].as_f64().unwrap(), speed);
                    assert!(k["max_key_speed_m_s"].as_f64().unwrap() < 2.0);
                    assert!(k["max_tracking_error_m"].as_f64().unwrap() < 1e-9);
                    assert!(k["max_relative_energy_defect"].as_f64().unwrap() < 1e-8);
                    assert!(k["max_relative_pedestal_work_lag"].as_f64().unwrap() < 1e-3);
                    assert_eq!(k["hard_limit_engaged"], false);
                    if shape == 2 {
                        let bed = &k["bed"];
                        assert_eq!(bed["depth_m"], 0.00025);
                        assert_eq!(bed["heat_monotone"], true);
                        let penetration = bed["max_penetration_m"].as_f64().unwrap();
                        assert!(penetration > 0.0 && penetration < 0.00025);
                    } else {
                        assert!(k["bed"].is_null());
                    }
                    let gestures = k["gestures"].as_array().unwrap();
                    assert_eq!(gestures.len(), 2);
                    for (g, start) in gestures.iter().zip([0.03, 0.21]) {
                        assert_eq!(g["start_seconds"], start);
                        let arrival = g["arrival_seconds_after_key_down"].as_f64().unwrap();
                        assert!(arrival > 0.0 && arrival < 0.12);
                        let arrival_speed = g["arrival_speed_m_s"].as_f64().unwrap();
                        // The hammer reaction can only slow the key below its free speed.
                        assert!(arrival_speed > 0.0 && arrival_speed < speed * (1.0 + 1e-9));
                        // Peak speed tracks end-of-tick velocities; an inelastic arrival speed is
                        // evaluated inside the final partial tick and can exceed it slightly.
                        assert!(g["peak_key_speed_m_s"].as_f64().unwrap() >= arrival_speed - 1e-4);
                        let w = &g["window"];
                        assert!((w["start_seconds"].as_f64().unwrap() - start).abs() < 1e-12);
                        assert!((w["end_seconds"].as_f64().unwrap() - start - 0.12).abs() < 1e-9);
                        assert!(w["relative_defect"].as_f64().unwrap() < 1e-8);
                        let t: Vec<f64> = w["terms_j"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|x| x.as_f64().unwrap())
                            .collect();
                        let sum: f64 = t[..7].iter().sum();
                        assert!(
                            (sum - t[7]).abs() <= 1e-8 * t.iter().map(|x| x.abs()).sum::<f64>()
                        );
                        assert!(t[0] > 0.0 && t[1] < 0.0 && t[2] < 0.0);
                        if shape == 2 {
                            assert!(t[4] < 0.0 && t[5] == 0.0);
                        } else {
                            assert!(t[3] == 0.0 && t[4] == 0.0 && t[5] < 0.0);
                        }
                        let held = w["end_position_m"].as_f64().unwrap();
                        if shape == 2 {
                            assert!(held > -0.00175 && held < -0.0015);
                        } else {
                            assert_eq!(held, -0.0015);
                        }
                    }
                    let recoveries = k["recoveries"].as_array().unwrap();
                    assert_eq!(recoveries.len(), 2);
                    let landing = recoveries[0]["landing_seconds_after_key_up"]
                        .as_f64()
                        .unwrap();
                    assert!(landing > 0.0 && landing < 0.06);
                    assert!(recoveries[0]["landing_speed_m_s"].as_f64().unwrap() > 0.0);
                    assert!(recoveries[0]["window"]["terms_j"][6].as_f64().unwrap() < 0.0);
                    assert_eq!(recoveries[0]["window"]["end_position_m"], -0.012);
                    assert_eq!(recoveries[0]["window"]["end_velocity_m_s"], 0.0);
                }
                let first = &takes[1]["repetition"]["phases"][0];
                let base = &drivers[0]["rows"][index]["takes"][1]["repetition"]["phases"][0];
                assert_eq!(row["first_vs_original"]["passed"], attack(first, base));
                if shape == 3 {
                    let light = &drivers[1]["rows"][index]["takes"][1]["repetition"]["phases"][0];
                    assert_eq!(
                        row["first_vs_light_hard_stop"]["passed"],
                        attack(first, light)
                    );
                } else {
                    assert!(row["first_vs_light_hard_stop"].is_null());
                }
            }
        }
    }
    assert_eq!(takes_seen, 32);
}

#[test]
fn loaded_key_inertia_cli_preserves_outputs_and_rejects_extra_arguments() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-key-inertia"],
        vec!["loaded-key-inertia", "--output", "keep.json"],
        vec!["loaded-key-inertia", "--output", "bad.wav"],
        vec!["loaded-key-inertia", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists() && !scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_drive_onset_receipt_splits_pedestal_work_and_replays_original_controls() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-drive-onset-validation.json");
    let prior = read("loaded-drive-release-validation.json");
    assert_eq!(r["experiment"], "loaded-drive-onset-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    assert_eq!(
        r["segment_order"],
        serde_json::json!(["onset", "cruise", "stop", "hold"])
    );
    let attack = |first: &serde_json::Value, base: &serde_json::Value| -> bool {
        let a = first["entries"].as_array().unwrap();
        let b = base["entries"].as_array().unwrap();
        let impact_ok = ["impulse_n_s", "peak_force_n", "active_contact_seconds"]
            .iter()
            .all(|key| {
                let reference = base["impact"][key].as_f64().unwrap();
                reference > 0.0
                    && (first["impact"][key].as_f64().unwrap() / reference - 1.0).abs() < 0.05
            });
        a.len() == 1 && b.len() == 1 && impact_ok && {
            let va = a[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            let vb = b[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
            (va / vb - 1.0).abs() < 0.05
                && (a[0]["seconds"].as_f64().unwrap() - b[0]["seconds"].as_f64().unwrap()).abs()
                    < 0.001
        }
    };
    let profiles = r["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 2);
    let mut striking = 0;
    let mut silent = 0;
    for profile in profiles {
        let old = prior["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == profile["name"])
            .unwrap();
        let drivers = profile["drivers"].as_array().unwrap();
        assert_eq!(drivers.len(), 4);
        for (shape, driver) in drivers.iter().enumerate() {
            let rows = driver["rows"].as_array().unwrap();
            assert_eq!(rows.len(), 2);
            for (index, row) in rows.iter().enumerate() {
                assert_eq!(row["measurement_qualified"], true);
                for key in ["convergence", "launch_convergence", "segment_convergence"] {
                    assert_eq!(row[key]["passed"], true);
                }
                let old_row = &old["drivers"][0]["rows"][index];
                assert_eq!(old_row["nominal_speed_m_s"], row["nominal_speed_m_s"]);
                let takes = row["takes"].as_array().unwrap();
                assert_eq!(takes.len(), 2);
                for (take, old_take) in takes.iter().zip(old_row["takes"].as_array().unwrap()) {
                    let d = &take["driver"];
                    assert_eq!(take["passed"], true);
                    assert_eq!(d["passed"], true);
                    assert_eq!(d["shape"], driver["name"]);
                    let speed = d["nominal_speed_m_s"].as_f64().unwrap();
                    let length = d["travel_m"].as_f64().unwrap();
                    let factor = [1.0, 1.2, 1.2, 1.4][shape];
                    let expected = factor * length / speed;
                    let ramp = 0.4 * length / speed;
                    assert!(
                        (d["nominal_arrival_seconds_after_key_down"]
                            .as_f64()
                            .unwrap()
                            - expected)
                            .abs()
                            < 1e-15
                    );
                    for time in d["measured_arrivals_seconds_after_key_down"]
                        .as_array()
                        .unwrap()
                    {
                        assert!((time.as_f64().unwrap() - expected).abs() < 1e-5);
                    }
                    assert!(d["max_tracking_error_m"].as_f64().unwrap() < 1e-8);
                    assert!(d["max_key_down_speed_m_s"].as_f64().unwrap() <= speed * (1.0 + 1e-8));
                    assert!(d["min_key_down_speed_m_s"].as_f64().unwrap() >= -speed * 1e-8);
                    let bounds = d["boundaries_seconds_after_key_down"].as_array().unwrap();
                    let onset_end = if shape == 0 { 0.0 } else { ramp };
                    assert!((bounds[0].as_f64().unwrap() - onset_end).abs() < 1e-15);
                    assert!((bounds[2].as_f64().unwrap() - expected).abs() < 1e-15);
                    let stop = if shape == 3 { ramp } else { 0.0 };
                    assert!(
                        (bounds[2].as_f64().unwrap() - bounds[1].as_f64().unwrap() - stop).abs()
                            < 1e-15
                    );
                    let peak = 0.75 * speed * speed / (0.2 * length);
                    match shape {
                        0 => assert!(d["onset_peak_acceleration_m_s2"].is_null()),
                        2 => assert!(
                            (d["onset_peak_acceleration_m_s2"].as_f64().unwrap() - peak / 1.5)
                                .abs()
                                < 1e-8
                        ),
                        _ => assert!(
                            (d["onset_peak_acceleration_m_s2"].as_f64().unwrap() - peak).abs()
                                < 1e-8
                        ),
                    }
                    assert_eq!(d["smooth_stop"], shape == 3);
                    assert_eq!(d["terminal_peak_deceleration_m_s2"].is_null(), shape != 3);
                    let segments = &take["drive_segments"];
                    assert_eq!(segments["passed"], true);
                    let keys = segments["key_downs"].as_array().unwrap();
                    assert_eq!(keys.len(), 2);
                    for (k, key) in keys.iter().enumerate() {
                        assert_eq!(key["passed"], true);
                        assert_eq!(key["start_seconds"], [0.03, 0.21][k]);
                        assert!(key["relative_pedestal_port_defect"].as_f64().unwrap() < 1e-8);
                        assert!((key["observed_seconds"].as_f64().unwrap() - 0.12).abs() < 1e-6);
                        let parts = key["segments"].as_array().unwrap();
                        assert_eq!(parts.len(), 4);
                        let sum = |name: &str| -> f64 {
                            parts.iter().map(|s| s[name].as_f64().unwrap()).sum()
                        };
                        let actuator = key["actuator_pedestal_work_j"].as_f64().unwrap();
                        assert!(
                            (sum("actuator_pedestal_work_j") - actuator).abs()
                                <= 1e-12 * actuator.abs().max(1e-12)
                        );
                        assert!(
                            (sum("pedestal_force_hammer_displacement_work_j")
                                - key["pedestal_force_hammer_displacement_work_j"]
                                    .as_f64()
                                    .unwrap())
                            .abs()
                                < 1e-15
                        );
                        assert!((sum("observed_seconds") - 0.12).abs() < 1e-6);
                        // The independent pedestal port identity reconstructs from retained terms.
                        let identity = actuator
                            - key["pedestal_force_hammer_displacement_work_j"]
                                .as_f64()
                                .unwrap()
                            - key["pedestal_stored_change_j"].as_f64().unwrap()
                            - key["pedestal_heat_change_j"].as_f64().unwrap();
                        assert!(identity.abs() < 1e-8 * actuator.abs().max(1e-6));
                        for (j, part) in parts.iter().enumerate() {
                            let empty = (j == 0 && shape == 0) || (j == 2 && shape != 3);
                            assert_eq!(part["observed_seconds"] == 0.0, empty);
                            assert_eq!(part["end"].is_null(), empty);
                            if empty {
                                assert_eq!(part["actuator_pedestal_work_j"], 0.0);
                            } else {
                                assert!(
                                    (part["end"]["seconds_after_key_down"].as_f64().unwrap()
                                        - part["end_seconds_after_key_down"].as_f64().unwrap())
                                    .abs()
                                        < 2.0 / (48000.0 * 128.0)
                                );
                            }
                        }
                        for ramp_row in key["ramp_acceleration"].as_array().unwrap() {
                            assert_eq!(ramp_row["passed"], true);
                            if ramp_row["analytical_peak_m_s2"].is_null() {
                                assert!(ramp_row["measured_peak_m_s2"].is_null());
                            } else {
                                assert!(ramp_row["relative_error"].as_f64().unwrap() < 1e-3);
                            }
                        }
                    }
                    if take["contact_entries"][0] == 0 {
                        silent += 1;
                        assert_eq!(take["repetition"]["two_clean_repeatable_strikes"], false);
                        assert!(
                            take["repetition"]["attack_repeatability"]["relative_impact_errors"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .all(|v| v.is_null())
                        );
                        for launch in take["launches"].as_array().unwrap() {
                            assert!(launch["before_contact"].is_null());
                        }
                    } else {
                        striking += 1;
                    }
                    if shape == 0 {
                        let mut replay = take.clone();
                        let object = replay.as_object_mut().unwrap();
                        object.remove("driver");
                        object.remove("drive_segments");
                        let mut old_replay = old_take.clone();
                        old_replay.as_object_mut().unwrap().remove("driver");
                        assert_eq!(replay, old_replay);
                        assert_eq!(row["convergence"], old_row["convergence"]);
                        assert_eq!(row["launch_convergence"], old_row["launch_convergence"]);
                    }
                    if shape == 1 || shape == 2 {
                        // Both onset ramps share the retained terminal-ease arrival time.
                        let shared = old["drivers"][1]["rows"][index]["takes"][1]["driver"]
                            ["nominal_arrival_seconds_after_key_down"]
                            .as_f64()
                            .unwrap();
                        assert!(
                            (d["nominal_arrival_seconds_after_key_down"]
                                .as_f64()
                                .unwrap()
                                - shared)
                                .abs()
                                < 1e-15
                        );
                    }
                }
                let first = &takes[1]["repetition"]["phases"][0];
                let base = &drivers[0]["rows"][index]["takes"][1]["repetition"]["phases"][0];
                assert_eq!(row["first_vs_original"]["passed"], attack(first, base));
                if shape == 2 {
                    let ease = &drivers[1]["rows"][index]["takes"][1]["repetition"]["phases"][0];
                    assert_eq!(
                        row["first_vs_onset_ease_same_arrival"]["passed"],
                        attack(first, ease)
                    );
                } else {
                    assert!(row["first_vs_onset_ease_same_arrival"].is_null());
                }
            }
        }
    }
    assert_eq!(striking + silent, 32);
    assert!(striking >= 8);
}

#[test]
fn loaded_drive_onset_cli_preserves_outputs_and_rejects_extra_arguments() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-drive-onset"],
        vec!["loaded-drive-onset", "--output", "keep.json"],
        vec!["loaded-drive-onset", "--output", "bad.wav"],
        vec!["loaded-drive-onset", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists() && !scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_drive_release_receipt_tracks_motion_and_preserves_original_controls() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-drive-release-validation.json");
    let prior = read("loaded-launch-validation.json");
    assert_eq!(r["experiment"], "loaded-drive-release-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    let profiles = r["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 2);
    for profile in profiles {
        let old = prior["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == profile["name"])
            .unwrap();
        let drivers = profile["drivers"].as_array().unwrap();
        assert_eq!(drivers.len(), 3);
        for (shape, driver) in drivers.iter().enumerate() {
            let rows = driver["rows"].as_array().unwrap();
            assert_eq!(rows.len(), 2);
            for (index, row) in rows.iter().enumerate() {
                assert_eq!(row["measurement_qualified"], true);
                assert_eq!(row["convergence"]["passed"], true);
                assert_eq!(row["launch_convergence"]["passed"], true);
                let old_row = old["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| {
                        r["speed_m_s"] == row["nominal_speed_m_s"]
                            && r["release_to_repeat_seconds"] == 0.06
                    })
                    .unwrap();
                let takes = row["takes"].as_array().unwrap();
                assert_eq!(takes.len(), 2);
                for (take, old_take) in takes.iter().zip(old_row["takes"].as_array().unwrap()) {
                    let d = &take["driver"];
                    assert_eq!(take["passed"], true);
                    assert_eq!(d["passed"], true);
                    if take["contact_entries"][0] == 0 {
                        assert_eq!(take["repetition"]["two_clean_repeatable_strikes"], false);
                        assert!(
                            take["repetition"]["attack_repeatability"]["relative_impact_errors"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .all(|v| v.is_null())
                        );
                        for launch in take["launches"].as_array().unwrap() {
                            assert!(launch["before_contact"].is_null());
                        }
                        for phase in take["repetition"]["phases"].as_array().unwrap() {
                            assert!(phase["entries"].as_array().unwrap().is_empty());
                            for key in ["impulse_n_s", "peak_force_n", "active_contact_seconds"] {
                                assert_eq!(phase["impact"][key], 0.0);
                            }
                        }
                    }
                    assert_eq!(d["shape"], driver["name"]);
                    let speed = d["nominal_speed_m_s"].as_f64().unwrap();
                    let length = d["travel_m"].as_f64().unwrap();
                    let expected = length / speed * if shape == 0 { 1.0 } else { 1.2 };
                    assert!(
                        (d["nominal_arrival_seconds_after_key_down"]
                            .as_f64()
                            .unwrap()
                            - expected)
                            .abs()
                            < 1e-15
                    );
                    for time in d["measured_arrivals_seconds_after_key_down"]
                        .as_array()
                        .unwrap()
                    {
                        assert!((time.as_f64().unwrap() - expected).abs() < 1e-5);
                    }
                    assert!(d["max_tracking_error_m"].as_f64().unwrap() < 1e-8);
                    assert!(d["max_key_down_speed_m_s"].as_f64().unwrap() <= speed * (1.0 + 1e-8));
                    assert!(d["min_key_down_speed_m_s"].as_f64().unwrap() >= -speed * 1e-8);
                    if shape == 1 {
                        assert!(
                            (d["terminal_deceleration_duration_seconds"]
                                .as_f64()
                                .unwrap()
                                - 0.4 * length / speed)
                                .abs()
                                < 1e-15
                        );
                        assert!(
                            (d["terminal_peak_deceleration_m_s2"].as_f64().unwrap()
                                - 0.75 * speed * speed / (0.2 * length))
                                .abs()
                                < 1e-8
                        );
                        // Contacts before terminal deceleration preserve the original launch prefix.
                        let original = &drivers[0]["rows"][index]["takes"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|a| a["steps_per_frame"] == take["steps_per_frame"])
                            .unwrap()["launches"][0];
                        let cutoff = 0.03 + 0.8 * length / speed;
                        let prefix = |l: &serde_json::Value| {
                            l["events"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .filter(|e| e["seconds"].as_f64().unwrap() < cutoff)
                                .cloned()
                                .collect::<Vec<_>>()
                        };
                        assert_eq!(prefix(&take["launches"][0]), prefix(original));
                    } else {
                        assert!(d["terminal_peak_deceleration_m_s2"].is_null());
                    }
                    if shape == 0 {
                        let mut replay = take.clone();
                        replay.as_object_mut().unwrap().remove("driver");
                        assert_eq!(replay, *old_take);
                        assert_eq!(row["convergence"], old_row["convergence"]);
                        assert_eq!(row["launch_convergence"], old_row["launch_convergence"]);
                    }
                }
                let first = &takes[1]["repetition"]["phases"][0];
                let base = &drivers[0]["rows"][index]["takes"][1]["repetition"]["phases"][0];
                let a = first["entries"].as_array().unwrap();
                let b = base["entries"].as_array().unwrap();
                let impact_ok = ["impulse_n_s", "peak_force_n", "active_contact_seconds"]
                    .iter()
                    .all(|key| {
                        let reference = base["impact"][key].as_f64().unwrap();
                        reference > 0.0
                            && (first["impact"][key].as_f64().unwrap() / reference - 1.0).abs()
                                < 0.05
                    });
                let agrees = a.len() == 1 && b.len() == 1 && impact_ok && {
                    let va = a[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
                    let vb = b[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
                    (va / vb - 1.0).abs() < 0.05
                        && (a[0]["seconds"].as_f64().unwrap() - b[0]["seconds"].as_f64().unwrap())
                            .abs()
                            < 0.001
                };
                assert_eq!(row["first_vs_original"]["passed"], agrees);
            }
        }
        for (eased, linear) in drivers[1]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .zip(drivers[2]["rows"].as_array().unwrap())
        {
            assert_eq!(
                eased["takes"][1]["driver"]["nominal_arrival_seconds_after_key_down"],
                linear["takes"][1]["driver"]["nominal_arrival_seconds_after_key_down"]
            );
        }
    }
}

#[test]
fn loaded_drive_release_cli_preserves_outputs_and_rejects_extra_arguments() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-drive-release"],
        vec!["loaded-drive-release", "--output", "keep.json"],
        vec!["loaded-drive-release", "--output", "bad.wav"],
        vec!["loaded-drive-release", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists() && !scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_launch_receipt_replays_repetition_and_closes_momentum_and_work() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-launch-validation.json");
    let prior = read("loaded-repetition-validation.json");
    assert_eq!(r["experiment"], "loaded-launch-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    for (case, old_case) in cases.iter().zip(prior["cases"].as_array().unwrap()) {
        assert_eq!(case["name"], old_case["name"]);
        let mass = case["settings"]["hammer_mass_kg"].as_f64().unwrap();
        let rows = case["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 4);
        for (row, old) in rows.iter().zip(old_case["rows"].as_array().unwrap()) {
            assert_eq!(row["speed_m_s"], old["speed_m_s"]);
            assert_eq!(
                row["release_to_repeat_seconds"],
                old["release_to_repeat_seconds"]
            );
            assert_eq!(row["measurement_qualified"], true);
            assert_eq!(row["convergence"], old["convergence"]);
            assert_eq!(row["launch_convergence"]["passed"], true);
            for (take, old_take) in row["takes"]
                .as_array()
                .unwrap()
                .iter()
                .zip(old["takes"].as_array().unwrap())
            {
                let mut replay = take.clone();
                replay.as_object_mut().unwrap().remove("launches");
                assert_eq!(replay, *old_take);
                let launches = take["launches"].as_array().unwrap();
                assert_eq!(launches.len(), 2);
                for (i, launch) in launches.iter().enumerate() {
                    assert_eq!(launch["passed"], true);
                    assert_eq!(launch["overflow"], false);
                    assert!(launch["max_relative_momentum_defect"].as_f64().unwrap() < 1e-8);
                    assert!(
                        launch["work_defects"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .all(|d| d.as_f64().unwrap() < 1e-8)
                    );
                    assert!(
                        (launch["end_seconds"].as_f64().unwrap()
                            - launch["start_seconds"].as_f64().unwrap()
                            - 0.02)
                            .abs()
                            < 1e-15
                    );
                    let initial = &launch["initial"];
                    let h0 = &initial["hammer"];
                    for key in ["initial", "before_contact", "end"] {
                        let snapshot = &launch[key];
                        let h = &snapshot["hammer"];
                        let terms = snapshot["kinetic_work_terms_j"].as_array().unwrap();
                        assert_eq!(terms.len(), 6);
                        let impulses = snapshot["impulse_components_n_s"].as_array().unwrap();
                        assert_eq!(impulses.len(), 5);
                        let sum: f64 = terms.iter().map(|x| x.as_f64().unwrap()).sum();
                        assert!((sum - h["hammer_kinetic_j"].as_f64().unwrap()).abs() < 1e-10);
                        let momentum = snapshot["initial_momentum_n_s"].as_f64().unwrap()
                            + impulses.iter().map(|x| x.as_f64().unwrap()).sum::<f64>();
                        assert!(
                            (momentum - mass * h["hammer_velocity_m_s"].as_f64().unwrap()).abs()
                                < 1e-10
                        );
                        assert!(
                            (snapshot["momentum_n_s"].as_f64().unwrap()
                                - mass * h["hammer_velocity_m_s"].as_f64().unwrap())
                            .abs()
                                < 1e-15
                        );
                        assert_eq!(terms[0], h0["hammer_kinetic_j"]);
                        let expected = [
                            h["pedestal_to_hammer_work_j"].as_f64().unwrap(),
                            -h["hammer_to_bridle_work_j"].as_f64().unwrap(),
                            -h["hammer_to_contact_work_j"].as_f64().unwrap(),
                            -h["hammer_return_heat_j"].as_f64().unwrap(),
                            h0["hammer_return_potential_j"].as_f64().unwrap()
                                - h["hammer_return_potential_j"].as_f64().unwrap(),
                        ];
                        for (term, expected) in terms[1..].iter().zip(expected) {
                            assert!((term.as_f64().unwrap() - expected).abs() < 1e-15);
                        }
                        // Pedestal storage and heat are measured from this launch's incoming state.
                        let pedestal = h["actuator_pedestal_work_j"].as_f64().unwrap()
                            - h["pedestal_to_hammer_work_j"].as_f64().unwrap()
                            - h["pedestal_heat_j"].as_f64().unwrap()
                            - h["pedestal_potential_j"].as_f64().unwrap()
                            + h0["pedestal_potential_j"].as_f64().unwrap();
                        assert!(pedestal.abs() < 1e-10);
                    }
                    let entry = &take["repetition"]["phases"][i * 2]["entries"][0];
                    let before = &launch["before_contact"];
                    assert_eq!(
                        before["hammer"]["hammer_velocity_m_s"],
                        entry["before"]["velocity_m_s"][18]
                    );
                    assert_eq!(
                        before["hammer"]["hammer_position_m"],
                        entry["before"]["position_m"][18]
                    );
                    let dt =
                        entry["seconds"].as_f64().unwrap() - before["seconds"].as_f64().unwrap();
                    assert!(
                        (dt - 1.0 / (48000.0 * take["steps_per_frame"].as_f64().unwrap())).abs()
                            < 1e-15
                    );
                    assert!(
                        before["peak_preimpact_velocity_m_s"].as_f64().unwrap()
                            >= before["hammer"]["hammer_velocity_m_s"].as_f64().unwrap()
                    );
                    let events = launch["events"].as_array().unwrap();
                    assert!(events.len() <= 64 && !events.is_empty());
                    for event in events {
                        let time = event["seconds"].as_f64().unwrap();
                        assert!(
                            time >= launch["start_seconds"].as_f64().unwrap()
                                && time <= launch["end_seconds"].as_f64().unwrap()
                        );
                        assert_eq!(
                            event["kind"] == "entry",
                            event["force_n"].as_f64().unwrap() > 0.0
                        );
                    }
                }
            }
            let d = &row["preimpact_difference"];
            assert_eq!(d["available"], true);
            let kinetic = d["second_minus_first_kinetic_work_terms_j"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .sum::<f64>();
            assert!(
                (kinetic - d["preimpact_kinetic_difference_j"].as_f64().unwrap()).abs() < 1e-10
            );
            let velocity = d["incoming_velocity_difference_m_s"].as_f64().unwrap()
                + d["second_minus_first_impulse_components_n_s"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_f64().unwrap() / mass)
                    .sum::<f64>();
            assert!(
                (velocity - d["preimpact_velocity_difference_m_s"].as_f64().unwrap()).abs() < 1e-8
            );
        }
    }
}

#[test]
fn loaded_launch_cli_rejects_ambiguous_arguments_and_preserves_outputs() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-launch"],
        vec!["loaded-launch", "--output", "keep.json"],
        vec!["loaded-launch", "--output", "bad.wav"],
        vec!["loaded-launch", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists() && !scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_repetition_receipt_preserves_prefix_and_separates_second_strikes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-repetition-validation.json");
    let prior = read("loaded-hammer-return-validation.json");
    assert_eq!(r["experiment"], "loaded-repetition-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    for (case, old_case) in cases.iter().zip(prior["cases"].as_array().unwrap()) {
        assert_eq!(case["name"], old_case["name"]);
        assert_eq!(
            case["settings"]["return_damping_n_s_m"],
            old_case["settings"]["return_damping_n_s_m"]
        );
        assert_eq!(
            case["settings"]["pedestal_rate_loss_s_m"],
            old_case["settings"]["pedestal_rate_loss_s_m"]
        );
        let rows = case["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 4);
        for row in rows {
            assert_eq!(row["measurement_qualified"], true);
            assert_eq!(row["convergence"]["passed"], true);
            assert_eq!(row["convergence"]["velocity"].as_array().unwrap().len(), 16);
            assert_eq!(row["convergence"]["phases"].as_array().unwrap().len(), 4);
            let old = old_case["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|old| old["speed_m_s"] == row["speed_m_s"])
                .unwrap();
            let takes = row["takes"].as_array().unwrap();
            assert_eq!(takes.len(), 2);
            for (take, old) in takes.iter().zip(old["takes"].as_array().unwrap()) {
                assert_eq!(take["passed"], true);
                assert_eq!(take["steps_per_frame"], old["steps_per_frame"]);
                assert_eq!(take["first_contact"], old["first_contact"]);
                let repeat = &take["repetition"];
                let phases = repeat["phases"].as_array().unwrap();
                assert_eq!(phases.len(), 4);
                let second_start = phases[2]["start_seconds"].as_f64().unwrap();
                assert!(
                    (second_start - 0.15 - row["release_to_repeat_seconds"].as_f64().unwrap())
                        .abs()
                        < 1e-15
                );
                for snapshot in take["snapshots"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|s| s["seconds"].as_f64().unwrap() < second_start)
                {
                    let old_snapshot = old["snapshots"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|s| s["seconds"] == snapshot["seconds"])
                        .unwrap();
                    assert_eq!(snapshot, old_snapshot);
                }
                assert_eq!(phases[0]["impact"], old["impact"]);
                let entries: usize = phases
                    .iter()
                    .map(|s| s["entries"].as_array().unwrap().len())
                    .sum();
                assert_eq!(entries as u64, take["contact_entries"][0].as_u64().unwrap());
                let impulse: f64 = phases
                    .iter()
                    .map(|s| s["impact"]["impulse_n_s"].as_f64().unwrap())
                    .sum();
                assert!(
                    (impulse
                        - take["whole_gesture_impact"]["impulse_n_s"]
                            .as_f64()
                            .unwrap())
                    .abs()
                        < 1e-12
                );
                for phase in phases {
                    assert_eq!(phase["overflow"], false);
                    assert!(phase["raw_voltage_rms_v"].as_f64().unwrap().is_finite());
                    for entry in phase["entries"].as_array().unwrap() {
                        let t = entry["seconds"].as_f64().unwrap();
                        assert!(
                            t >= phase["start_seconds"].as_f64().unwrap()
                                && t <= phase["end_seconds"].as_f64().unwrap() + 1e-15
                        );
                    }
                }
                let before = &repeat["before_second_drive"];
                assert_eq!(before["position_m"].as_array().unwrap().len(), 20);
                assert_eq!(before["velocity_m_s"].as_array().unwrap().len(), 20);
                assert!(before["mechanical_energy_j"].as_f64().unwrap() > 0.0);
                assert_eq!(
                    before["contact_entries"][0].as_u64().unwrap(),
                    (phases[0]["entries"].as_array().unwrap().len()
                        + phases[1]["entries"].as_array().unwrap().len())
                        as u64
                );
                let ready = &repeat["readiness"];
                let ready_pass = ready["max_position_error_m"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|x| x.as_f64().unwrap() < 0.0001)
                    && ready["max_velocity_m_s"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|x| x.as_f64().unwrap() < 0.01)
                    && ready["felt_contact_fraction"].as_f64().unwrap() >= 0.9;
                assert_eq!(ready["passed"], ready_pass);
                let lifted = repeat["minimum_held_felt_clearance_m"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|x| x.as_f64().unwrap() >= 0.0001)
                    && repeat["held_felt_contact_ticks"] == serde_json::json!([0, 0]);
                assert_eq!(repeat["both_lifts_passed"], lifted);
                let separated = phases
                    .iter()
                    .all(|s| s["carry_in"] == false && s["carry_out"] == false)
                    && [1, 3].iter().all(|i| {
                        phases[*i]["entries"].as_array().unwrap().is_empty()
                            && phases[*i]["impact"]["active_contact_seconds"] == 0.0
                    });
                assert_eq!(repeat["separated_strikes"], separated);
                let impact_ok = ["impulse_n_s", "peak_force_n", "active_contact_seconds"]
                    .iter()
                    .all(|key| {
                        let first = phases[0]["impact"][key].as_f64().unwrap();
                        first > 0.0
                            && (phases[2]["impact"][key].as_f64().unwrap() / first - 1.0).abs()
                                < 0.05
                    });
                let a = phases[0]["entries"].as_array().unwrap();
                let b = phases[2]["entries"].as_array().unwrap();
                let matching = a.len() == 1 && b.len() == 1 && impact_ok && {
                    let va = a[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
                    let vb = b[0]["before"]["velocity_m_s"][18].as_f64().unwrap();
                    (vb / va - 1.0).abs() < 0.05
                        && ((b[0]["seconds"].as_f64().unwrap() - second_start)
                            - (a[0]["seconds"].as_f64().unwrap() - 0.03))
                            .abs()
                            < 0.001
                };
                assert_eq!(repeat["attack_repeatability"]["passed"], matching);
                assert_eq!(
                    repeat["two_clean_repeatable_strikes"],
                    matching && separated && lifted
                );
            }
        }
    }
}

#[test]
fn loaded_repetition_cli_preserves_outputs_and_rejects_ambiguous_arguments() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-repetition"],
        vec!["loaded-repetition", "--output", "keep.json"],
        vec!["loaded-repetition", "--output", "bad.wav"],
        vec!["loaded-repetition", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists() && !scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_hammer_return_receipt_checks_free_motion_and_replays_bridle_prefix() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-hammer-return-validation.json");
    let prior = read("loaded-bridle-validation.json");
    assert_eq!(r["experiment"], "loaded-hammer-return-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    for case in cases {
        let m = case["settings"]["hammer_mass_kg"].as_f64().unwrap();
        let k = case["settings"]["return_stiffness_n_m"].as_f64().unwrap();
        let rows = case["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        for row in rows {
            assert_eq!(row["measurement_qualified"], true);
            assert_eq!(row["convergence"]["passed"], true);
            assert_eq!(row["event_convergence"]["passed"], true);
            assert_eq!(row["convergence"]["velocity"].as_array().unwrap().len(), 16);
            let takes = row["takes"].as_array().unwrap();
            assert_eq!(takes.len(), 2);
            for (take, steps) in takes.iter().zip([128, 256]) {
                assert_eq!(take["steps_per_frame"], steps);
                assert_eq!(take["duration_seconds"], 0.8);
                assert_eq!(take["passed"], true);
                let trace = &take["return_trace"];
                assert_eq!(trace["qualified"], true);
                assert_eq!(trace["overflow"], false);
                assert!(trace["max_free_relative_state_error"].as_f64().unwrap() < 1e-6);
                let flights = trace["free_intervals"].as_array().unwrap();
                assert!(!flights.is_empty());
                for flight in flights {
                    let n = |key: &str| flight[key].as_f64().unwrap();
                    assert!(n("end_seconds") - n("start_seconds") >= 0.001);
                    let energy = |x: f64, v: f64| 0.5 * k * x * x + 0.5 * m * v * v;
                    let initial = energy(n("initial_offset_m"), n("initial_velocity_m_s"));
                    let final_energy = energy(n("actual_offset_m"), n("actual_velocity_m_s"));
                    assert!((initial - n("initial_energy_j")).abs() < 1e-14);
                    assert!((final_energy - n("final_energy_j")).abs() < 1e-14);
                    assert!((final_energy - initial + n("return_heat_j")).abs() < 1e-12);
                    let error = ((k / m
                        * (n("actual_offset_m") - n("predicted_offset_m")).powi(2)
                        + (n("actual_velocity_m_s") - n("predicted_velocity_m_s")).powi(2))
                        / (2.0 * initial / m).max(1e-16))
                    .sqrt();
                    assert!((error - n("relative_state_error")).abs() < 1e-12 && error < 1e-6);
                }
                let mut previous_time = 0.15;
                for event in trace["pedestal_events"].as_array().unwrap() {
                    let time = event["seconds"].as_f64().unwrap();
                    assert!(time > previous_time && time <= 0.8);
                    previous_time = time;
                    let before = event["before"]["force_n"][2].as_f64().unwrap() > 0.0;
                    let after = event["after"]["force_n"][2].as_f64().unwrap() > 0.0;
                    assert_ne!(before, after);
                    assert_eq!(event["kind"], if after { "entry" } else { "exit" });
                }
                let f = &take["function"];
                let returned = f["return_max_position_error_m"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|v| v.as_f64().unwrap() < 0.0001)
                    && f["return_max_velocity_m_s"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|v| v.as_f64().unwrap() < 0.01)
                    && f["return_felt_contact_fraction"].as_f64().unwrap() >= 0.9;
                assert_eq!(f["return_passed"], returned);
                assert_eq!(
                    f["single_strike_lift_return_passed"],
                    returned && f["held_lift_passed"] == true && take["contact_entries"][0] == 1
                );
            }
            assert_eq!(
                row["attack_lift_return_passed"],
                row["attack_vs_baseline"]["passed"] == true
                    && takes[1]["function"]["single_strike_lift_return_passed"] == true
            );
        }
    }
    for row in cases[0]["rows"].as_array().unwrap() {
        let old = prior["cases"][0]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["speed_m_s"] == row["speed_m_s"])
            .unwrap();
        for (take, old) in row["takes"]
            .as_array()
            .unwrap()
            .iter()
            .zip(old["takes"].as_array().unwrap())
        {
            assert_eq!(take["first_contact"], old["first_contact"]);
            for (a, b) in take["snapshots"]
                .as_array()
                .unwrap()
                .iter()
                .zip(old["snapshots"].as_array().unwrap())
            {
                assert_eq!(a, b);
            }
        }
    }
    for case in cases {
        for (index, row) in case["rows"].as_array().unwrap().iter().enumerate() {
            let take = &row["takes"][1];
            let base = &cases[0]["rows"][index]["takes"][1];
            let impact_errors: Vec<f64> = ["impulse_n_s", "peak_force_n", "active_contact_seconds"]
                .iter()
                .map(|key| {
                    (take["impact"][key].as_f64().unwrap() / base["impact"][key].as_f64().unwrap()
                        - 1.0)
                        .abs()
                })
                .collect();
            for (actual, recorded) in impact_errors.iter().zip(
                row["attack_vs_baseline"]["relative_impact_errors"]
                    .as_array()
                    .unwrap(),
            ) {
                assert!((actual - recorded.as_f64().unwrap()).abs() < 1e-12);
            }
            let velocity = take["first_contact"]["before_hammer"]["hammer_velocity_m_s"].as_f64();
            let base_velocity = base["first_contact"]["before_hammer"]["hammer_velocity_m_s"]
                .as_f64()
                .unwrap();
            let speed_ok = velocity.is_some_and(|v| (v / base_velocity - 1.0).abs() < 0.05);
            let time_ok = take["first_contact"]["seconds"].as_f64().is_some_and(|t| {
                (t - base["first_contact"]["seconds"].as_f64().unwrap()).abs() < 0.001
            });
            assert_eq!(
                row["attack_vs_baseline"]["passed"],
                take["contact_entries"][0] == 1
                    && impact_errors.iter().all(|e| *e < 0.05)
                    && speed_ok
                    && time_ok
            );
            for key in [
                "return_passed",
                "held_lift_passed",
                "single_strike_lift_return_passed",
            ] {
                assert_eq!(row["takes"][0]["function"][key], take["function"][key]);
            }
        }
    }
}

#[test]
fn loaded_hammer_return_cli_preserves_existing_outputs() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-hammer-return"],
        vec!["loaded-hammer-return", "--output", "keep.json"],
        vec!["loaded-hammer-return", "--output", "bad.wav"],
        vec!["loaded-hammer-return", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists());
    assert!(!scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_bridle_receipt_closes_coupling_and_preserves_strike_prefix() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-bridle-validation.json");
    let prior = read("loaded-strike-threshold-validation.json");
    assert_eq!(r["experiment"], "loaded-bridle-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    assert_eq!(r["physical_calibration_claimed"], false);
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    for case in cases {
        let rows = case["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        for row in rows {
            assert_eq!(row["measurement_qualified"], true);
            assert_eq!(row["takes"].as_array().unwrap().len(), 2);
            for take in row["takes"].as_array().unwrap() {
                let initial = &take["initial_coupling"];
                assert!(initial["arm_energy_j"].as_f64().unwrap() > 0.0);
                for snapshot in take["snapshots"].as_array().unwrap() {
                    let c = &snapshot["coupling"];
                    let number = |key: &str| c[key].as_f64().unwrap();
                    let delta = |key: &str| number(key) - initial[key].as_f64().unwrap();
                    let bridle = number("hammer_to_bridle_work_j")
                        - number("bridle_to_arm_work_j")
                        - delta("bridle_potential_j")
                        - delta("bridle_heat_j");
                    let arm = delta("arm_energy_j") - number("bridle_to_arm_work_j")
                        + number("arm_to_felt_work_j")
                        + number("arm_heat_j")
                        - number("pedal_work_j");
                    let felt = number("arm_to_felt_work_j")
                        - number("felt_to_structure_work_j")
                        - delta("felt_potential_j")
                        - delta("felt_heat_j");
                    assert!(bridle.abs() < 1e-10 && arm.abs() < 1e-10 && felt.abs() < 1e-10);
                    assert_eq!(
                        c["hammer_to_bridle_work_j"],
                        snapshot["hammer"]["hammer_to_bridle_work_j"]
                    );
                    assert_eq!(number("pedal_work_j"), 0.0);
                }
                let f = &take["function"];
                let lifted = f["minimum_held_felt_clearance_m"].as_f64().unwrap() >= 0.0001
                    && f["held_felt_contact_ticks"] == 0;
                let returned = f["return_max_position_error_m"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|v| v.as_f64().unwrap() < 0.0001)
                    && f["return_max_velocity_m_s"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|v| v.as_f64().unwrap() < 0.01)
                    && f["return_felt_contact_fraction"].as_f64().unwrap() >= 0.9;
                assert_eq!(f["held_lift_passed"], lifted);
                assert_eq!(f["return_passed"], returned);
                assert!(lifted && !returned);
                // The arm and felt return; the hammer alone exceeds both settling limits.
                assert!(f["return_max_position_error_m"][0].as_f64().unwrap() > 0.0001);
                assert!(f["return_max_velocity_m_s"][0].as_f64().unwrap() > 0.01);
                assert!(f["return_max_position_error_m"][1].as_f64().unwrap() < 0.0001);
                assert!(f["return_max_velocity_m_s"][1].as_f64().unwrap() < 0.01);
                assert_eq!(f["return_felt_contact_fraction"], 1.0);
                assert_eq!(
                    f["single_strike_lift_return_passed"],
                    take["contact_entries"][0] == 1 && lifted && returned
                );
            }
        }
    }
    let slack_rows = cases[1]["rows"].as_array().unwrap();
    for (row, count) in slack_rows.iter().zip([1, 0, 1]) {
        for take in row["takes"].as_array().unwrap() {
            assert_eq!(take["contact_entries"][0], count);
        }
    }
    for row in cases[0]["rows"].as_array().unwrap() {
        let old = prior["cases"][0]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["speed_m_s"] == row["speed_m_s"])
            .unwrap();
        for (take, old) in row["takes"]
            .as_array()
            .unwrap()
            .iter()
            .zip(old["takes"].as_array().unwrap())
        {
            assert_eq!(take["snapshots"][0]["hammer"], old["end"]);
            if !old["before_first_contact"].is_null() {
                assert_eq!(
                    take["first_contact"]["before_hammer"],
                    old["before_first_contact"]
                );
            }
        }
    }
}

#[test]
fn loaded_bridle_cli_preserves_outputs_and_rejects_ambiguous_arguments() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-bridle"],
        vec!["loaded-bridle", "--output", "keep.json"],
        vec!["loaded-bridle", "--output", "bad.wav"],
        vec!["loaded-bridle", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists());
    assert!(!scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_threshold_receipt_closes_hammer_work_and_preserves_known_impacts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-strike-threshold-validation.json");
    let prior = read("loaded-dynamics-validation.json");
    assert_eq!(r["experiment"], "loaded-strike-threshold-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["reference_match_claimed"], false);
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    for case in cases {
        let rows = case["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 6);
        for row in rows {
            assert_eq!(row["measurement_qualified"], true);
            assert_eq!(row["convergence"]["passed"], true);
            assert_eq!(row["takes"].as_array().unwrap().len(), 2);
            for take in row["takes"].as_array().unwrap() {
                assert_eq!(take["event_overflow"], false);
                assert_eq!(take["duration_seconds"], 0.12);
                let end = &take["end"];
                // This fixed grid starts with zero hammer/hammer-contact/pedestal energy.
                let number = |key: &str| end[key].as_f64().unwrap();
                let hammer = number("hammer_energy_j") - number("pedestal_to_hammer_work_j")
                    + number("hammer_to_contact_work_j")
                    + number("hammer_to_bridle_work_j")
                    + number("hammer_return_heat_j");
                let pedestal = number("actuator_pedestal_work_j")
                    - number("pedestal_to_hammer_work_j")
                    - number("pedestal_potential_j")
                    - number("pedestal_heat_j");
                let contact = number("hammer_to_contact_work_j")
                    - number("contact_to_structure_work_j")
                    - number("hammer_contact_potential_j")
                    - number("hammer_contact_heat_j");
                assert!(hammer.abs() < 1e-10 && pedestal.abs() < 1e-10 && contact.abs() < 1e-10);
                if take["contact_entries"][0] == 0 {
                    assert_eq!(take["classification"], "no_contact_in_window");
                    assert!(take["before_first_contact"].is_null());
                    assert_eq!(take["impact"]["impulse_n_s"], 0.0);
                    assert_eq!(number("contact_to_structure_work_j"), 0.0);
                }
            }
        }
    }
    // Contact stiffness cannot change the trajectory before first hammer contact.
    for (a, b) in cases[0]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .zip(cases[4]["rows"].as_array().unwrap())
    {
        for (a, b) in a["takes"]
            .as_array()
            .unwrap()
            .iter()
            .zip(b["takes"].as_array().unwrap())
        {
            assert_eq!(a["before_first_contact"], b["before_first_contact"]);
        }
    }
    assert_eq!(cases[2]["rows"][2]["takes"][1]["contact_entries"][0], 2);
    assert!(
        cases[3]["transition_intervals"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    for ((a, b), c) in cases[3]["rows"][1]["takes"]
        .as_array()
        .unwrap()
        .iter()
        .zip(cases[3]["rows"][2]["takes"].as_array().unwrap())
        .zip(cases[3]["rows"][3]["takes"].as_array().unwrap())
    {
        let speed = |take: &serde_json::Value| {
            take["before_first_contact"]["hammer_velocity_m_s"]
                .as_f64()
                .unwrap()
        };
        assert!(speed(a) > speed(b) && speed(b) > speed(c));
    }
    for speed in [1.125, 1.3125, 1.5] {
        let row = cases[0]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["speed_m_s"] == speed)
            .unwrap();
        let old = prior["training_cases"]
            .as_array()
            .unwrap()
            .iter()
            .chain(prior["validation_cases"].as_array().unwrap())
            .find(|r| r["voicing"] == "baseline" && r["speed_m_s"] == speed)
            .unwrap();
        for i in 0..2 {
            let take = &row["takes"][i];
            assert_eq!(
                take["first_contact_seconds"],
                old["takes"][i]["first_contact_seconds"]
            );
            assert_eq!(
                take["before_first_contact"]["hammer_velocity_m_s"],
                old["takes"][i]["impact"]["pre_contact_hammer_speed_m_s"]
            );
            for key in ["impulse_n_s", "peak_force_n", "active_contact_seconds"] {
                assert_eq!(take["impact"][key], old["takes"][i]["impact"][key]);
            }
        }
    }
}

#[test]
fn loaded_threshold_cli_preserves_outputs_and_rejects_ambiguous_arguments() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-strike-threshold"],
        vec!["loaded-strike-threshold", "--output", "keep.json"],
        vec!["loaded-strike-threshold", "--output", "bad.wav"],
        vec![
            "loaded-strike-threshold",
            "--output",
            "bad.json",
            "--unknown",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists());
    assert!(!scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_dynamics_receipt_selects_shared_training_minimum_and_freezes_reserved_speeds() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-dynamics-validation.json");
    let prior = read("loaded-voicing-validation.json");
    let calibration = read("loaded-loss-calibration-qualified-validation.json");
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["experiment"], "loaded-dynamics-v1");
    assert_eq!(r["measurement_qualified"], true);
    assert_eq!(r["sources"], prior["sources"]);
    assert_eq!(r["manifest"], prior["manifest"]);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    assert_eq!(r["reference_match_claimed"], false);
    assert_eq!(r["physical_calibration_claimed"], false);
    let training = r["training_cases"].as_array().unwrap();
    assert_eq!(training.len(), 6);
    assert_eq!(r["validation_cases"].as_array().unwrap().len(), 2);
    assert_eq!(r["all_comparisons"].as_array().unwrap().len(), 40);
    assert_eq!(r["mapped_comparisons"].as_array().unwrap().len(), 5);
    for case in training
        .iter()
        .chain(r["validation_cases"].as_array().unwrap())
    {
        assert_eq!(case["measurement_qualified"], true);
        assert_eq!(case["voltage_convergence"]["passed"], true);
        assert_eq!(case["impact_convergence"]["passed"], true);
        for take in case["takes"].as_array().unwrap() {
            assert_eq!(take["passed"], true);
            assert_eq!(take["contact_entries"][0], 1);
            assert!(take["max_relative_energy_defect"].as_f64().unwrap() < 1e-8);
            assert!(take["pre_key_peak_fs"].as_f64().unwrap() < 1e-10);
        }
    }
    // Reusing analysis helpers must preserve historical renders exactly.
    for (i, previous_case) in [(1, 0), (4, 6)] {
        assert_eq!(training[i]["takes"], prior["cases"][previous_case]["takes"]);
    }
    for (i, g) in [1, 0, 2].into_iter().enumerate() {
        for resolution in 0..2 {
            let mut take = training[i]["takes"][resolution].clone();
            take.as_object_mut().unwrap().remove("impact");
            assert_eq!(take, calibration["gestures"][g]["takes"][resolution]);
        }
    }
    let hypotheses = r["hypotheses"].as_array().unwrap();
    assert_eq!(hypotheses.len(), 4);
    let mut best = (usize::MAX, f64::INFINITY);
    for (h, hypothesis) in hypotheses.iter().enumerate() {
        let reverse = hypothesis["reverse"].as_bool().unwrap();
        let mut sum = Some(0.0);
        for layer in [0, 2, 4] {
            let speed = if reverse { 4 - layer } else { layer };
            let take = training
                .iter()
                .find(|t| t["voicing"] == hypothesis["voicing"] && t["speed_index"] == speed)
                .unwrap();
            if take["measurement_qualified"] != true {
                sum = None;
            }
            for band in 0..3 {
                sum = sum.zip(r["sources"][layer]["profile"]["windows"][0]["band_db_relative_to_fundamental_band"][band].as_f64())
                    .zip(take["takes"][1]["timbre"]["windows"][0]["band_db_relative_to_fundamental_band"][band].as_f64())
                    .map(|((total,a),b)|total+(a-b).powi(2));
            }
        }
        if let Some(sum) = sum {
            let score = (sum / 9.0).sqrt();
            assert!((score - hypothesis["training_attack_rms_db"].as_f64().unwrap()).abs() < 1e-12);
            if score < best.1 {
                best = (h, score);
            }
        } else {
            assert!(hypothesis["training_attack_rms_db"].is_null());
        }
    }
    assert_eq!(r["selected"]["hypothesis_index"], best.0);
    assert_eq!(r["selected"]["voicing"], hypotheses[best.0]["voicing"]);
    let reverse = r["selected"]["reverse"].as_bool().unwrap();
    let mut reserved_square = 0.0;
    for (layer, pair) in r["mapped_comparisons"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let speed = if reverse { 4 - layer } else { layer };
        assert_eq!(pair["speed_m_s"], r["speed_grid_m_s"][speed]);
        assert_eq!(pair["source_id"], r["sources"][layer]["id"]);
        if layer % 2 == 1 {
            let left = r["mapped_comparisons"][layer - 1]["speed_m_s"]
                .as_f64()
                .unwrap();
            let right = r["mapped_comparisons"][layer + 1]["speed_m_s"]
                .as_f64()
                .unwrap();
            assert_eq!(pair["speed_m_s"].as_f64().unwrap(), 0.5 * (left + right));
            reserved_square += pair["comparison"]["attack_band_rms_db"]
                .as_f64()
                .unwrap()
                .powi(2);
        }
    }
    assert!(
        ((reserved_square / 2.0).sqrt()
            - r["selected"]["validation_attack_rms_db"].as_f64().unwrap())
        .abs()
            < 1e-12
    );
}

#[test]
fn loaded_dynamics_cli_preserves_outputs_and_requires_verified_inputs() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-dynamics"],
        vec!["loaded-dynamics", "missing.json", "--output", "keep.json"],
        vec!["loaded-dynamics", "missing.json", "--output", "bad.wav"],
        vec!["loaded-dynamics", "missing.json", "--output", "bad.json"],
        vec![
            "loaded-dynamics",
            "missing.json",
            "--output",
            "bad.json",
            "--unknown",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.wav").exists());
    assert!(!scratch.0.join("bad.json").exists());
}

#[test]
fn loaded_voicing_receipt_preserves_baseline_and_separates_impact_from_spectrum() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-voicing-validation.json");
    let prior = read("loaded-loss-calibration-qualified-validation.json");
    assert_eq!(r["experiment"], "loaded-voicing-v1");
    assert!(r["failure_reason"].is_null());
    assert_eq!(r["sources"], prior["sources"]);
    assert_eq!(r["manifest"], prior["manifest"]);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    assert_eq!(r["reference_match_claimed"], false);
    assert_eq!(r["physical_calibration_claimed"], false);
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    assert_eq!(r["comparisons"].as_array().unwrap().len(), 35);
    assert_eq!(cases[0]["measurement_qualified"], true);
    assert_eq!(r["measurement_qualified"], false);
    assert_eq!(
        cases
            .iter()
            .filter(|c| c["measurement_qualified"] == true)
            .count(),
        6
    );
    // Preserve the converged recontact as an ineligible single-strike control.
    assert_eq!(cases[2]["name"], "strike_30_percent");
    assert_eq!(cases[2]["measurement_qualified"], false);
    assert_eq!(cases[2]["voltage_convergence"]["passed"], true);
    assert_eq!(cases[2]["impact_convergence"]["passed"], true);
    for take in cases[2]["takes"].as_array().unwrap() {
        assert_eq!(take["contact_entries"][0], 3);
        assert!(take["max_relative_energy_defect"].as_f64().unwrap() < 1e-8);
    }
    for i in 0..2 {
        let mut take = cases[0]["takes"][i].clone();
        assert!(take.as_object_mut().unwrap().remove("impact").is_some());
        assert_eq!(take, prior["gestures"][0]["takes"][i]);
    }
    for c in cases {
        let a = &c["takes"][0];
        let b = &c["takes"][1];
        assert_eq!(a["steps_per_frame"], 128);
        assert_eq!(b["steps_per_frame"], 256);
        let mut impact_passed = true;
        for q in c["impact_convergence"]["quantities"].as_array().unwrap() {
            let key = q["quantity"].as_str().unwrap();
            let fine = b["impact"][key].as_f64().unwrap();
            assert!(fine > 0.0);
            let error = (a["impact"][key].as_f64().unwrap() - fine).abs() / fine;
            assert!((q["relative_error"].as_f64().unwrap() - error).abs() < 1e-12);
            assert_eq!(q["passed"], error < 0.01);
            impact_passed &= error < 0.01;
        }
        assert_eq!(c["impact_convergence"]["passed"], impact_passed);
        let qualified = a["passed"] == true
            && b["passed"] == true
            && c["voltage_convergence"]["passed"] == true
            && impact_passed;
        assert_eq!(c["measurement_qualified"], qualified);
        // Independently pool all nine training attack dimensions; absent bands withhold it.
        let mut squared = Some(0.0);
        for s in r["sources"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["role"] == "training")
        {
            for i in 0..3 {
                squared = squared
                    .zip(
                        s["profile"]["windows"][0]["band_db_relative_to_fundamental_band"][i]
                            .as_f64(),
                    )
                    .zip(
                        b["timbre"]["windows"][0]["band_db_relative_to_fundamental_band"][i]
                            .as_f64(),
                    )
                    .map(|((sum, source), candidate)| sum + (candidate - source).powi(2));
            }
        }
        match squared {
            Some(sum) => assert!(
                ((sum / 9.0).sqrt() - c["training_attack_rms_db"].as_f64().unwrap()).abs() < 1e-12
            ),
            None => assert!(c["training_attack_rms_db"].is_null()),
        }
    }
    assert_eq!(
        r["measurement_qualified"],
        cases.iter().all(|c| c["measurement_qualified"] == true)
    );
    for pair in r["comparisons"].as_array().unwrap() {
        let p = &pair["comparison"];
        if p["available"] == true && p["maximum_band_difference_db"].as_f64().unwrap() > 6.0 {
            assert_eq!(p["spectral_agreement"], false);
        }
    }
}

#[test]
fn loaded_voicing_cli_requires_verified_inputs_and_preserves_outputs() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-voicing"],
        vec!["loaded-voicing", "missing.json", "--output", "keep.json"],
        vec!["loaded-voicing", "missing.json", "--output", "bad.wav"],
        vec!["loaded-voicing", "missing.json", "--output", "bad.json"],
        vec![
            "loaded-voicing",
            "missing.json",
            "--output",
            "bad.json",
            "--unknown",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.wav").exists());
    assert!(!scratch.0.join("bad.json").exists());
}

#[test]
fn loaded_calibration_receipt_selects_training_minimum_and_preserves_validation() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let failed = read("loaded-loss-calibration-validation.json");
    assert_eq!(failed["measurement_qualified"], false);
    assert_eq!(failed["reason"], "loss study pitch unavailable");
    let r = read("loaded-loss-calibration-qualified-validation.json");
    assert_eq!(r["qualified_grid_followup"], true);
    let prior = read("loaded-source-rest-validation.json");
    assert_eq!(r["measurement_qualified"], true);
    assert_eq!(r["physical_calibration_claimed"], false);
    assert_eq!(r["reference_match_claimed"], false);
    assert_eq!(r["sources"], prior["sources"]);
    assert_eq!(r["manifest"], prior["manifest"]);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    let grid = r["grid"].as_array().unwrap();
    assert_eq!(grid.len(), 9);
    let best = grid
        .iter()
        .filter_map(|x| x["training_rms_db"].as_f64())
        .fold(f64::INFINITY, f64::min);
    assert_eq!(
        best,
        r["selected"]["training_coarse_rms_db"].as_f64().unwrap()
    );
    let selected = grid
        .iter()
        .find(|x| {
            x["tine_loss_scale"] == r["selected"]["tine_loss_scale"]
                && x["support_loss_scale"] == r["selected"]["support_loss_scale"]
        })
        .unwrap();
    assert_eq!(selected["take"]["passed"], true);
    for row in grid {
        if row["take"]["passed"] != true {
            assert!(row["training_rms_db"].is_null());
        }
    }
    assert_eq!(
        selected["training_rms_db"],
        r["selected"]["training_coarse_rms_db"]
    );
    assert_eq!(r["gestures"].as_array().unwrap().len(), 3);
    assert_eq!(r["comparisons"].as_array().unwrap().len(), 15);
    for g in r["gestures"].as_array().unwrap() {
        assert_eq!(g["qualified"], true);
        assert_eq!(g["convergence"]["passed"], true);
        for take in g["takes"].as_array().unwrap() {
            assert_eq!(take["passed"], true);
            assert_eq!(take["initial_position"], r["baseline"]["initial_position"]);
        }
    }
    // Recompute role-specific scores from retained observations, without the fitter.
    let fine = &r["gestures"][0]["takes"][1]["timbre"];
    for (role, key) in [
        ("training", "training_fine_rms_db"),
        ("validation", "validation_fine_rms_db"),
    ] {
        let mut sum = 0.0;
        let mut n = 0;
        for s in r["sources"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["role"] == role)
        {
            for i in [3, 4] {
                sum += (s["profile"]["windows"][i]["level_relative_to_body_db"]
                    .as_f64()
                    .unwrap()
                    - fine["windows"][i]["level_relative_to_body_db"]
                        .as_f64()
                        .unwrap())
                .powi(2);
                n += 1;
            }
        }
        assert!(((sum / n as f64).sqrt() - r["selected"][key].as_f64().unwrap()).abs() < 1e-12);
    }
}

#[test]
fn loaded_calibration_cli_requires_verified_inputs_and_new_outputs() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"keep").unwrap();
    fs::write(scratch.0.join("keep.wav"), b"keep").unwrap();
    for args in [
        vec!["calibrate-loaded-loss"],
        vec![
            "calibrate-loaded-loss",
            "missing.json",
            "--output",
            "keep.json",
        ],
        vec![
            "calibrate-loaded-loss",
            "missing.json",
            "--output",
            "bad.json",
            "--preview",
            "keep.wav",
        ],
        vec![
            "calibrate-loaded-loss",
            "missing.json",
            "--output",
            "bad.json",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"keep");
    assert_eq!(fs::read(scratch.0.join("keep.wav")).unwrap(), b"keep");
    assert!(!scratch.0.join("bad.json").exists());
}

#[test]
fn loaded_loss_receipt_closes_independent_channels_and_preserves_baseline() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-loss-budget-validation.json");
    let prior = read("loaded-source-rest-validation.json");
    assert_eq!(r["passed"], true);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    assert_eq!(r["channels"].as_array().unwrap().len(), 12);
    let cases = r["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    assert_eq!(
        cases[0]["takes"][1]["timbre"],
        prior["candidates"][1]["profile"]
    );
    for case in cases {
        assert_eq!(case["passed"], true);
        assert_eq!(case["convergence"]["passed"], true);
        for take in case["takes"].as_array().unwrap() {
            assert_eq!(take["passed"], true);
            assert_eq!(take["heat_monotone"], true);
            assert_eq!(
                take["initial_position"],
                cases[0]["takes"][0]["initial_position"]
            );
            assert!(
                take["max_relative_structural_split_defect"]
                    .as_f64()
                    .unwrap()
                    < 1e-10
            );
            assert!(take["max_relative_energy_defect"].as_f64().unwrap() < 1e-8);
            assert!(take["pre_key_peak_fs"].as_f64().unwrap() < 1e-10);
            for window in take["windows"].as_array().unwrap() {
                let heat = window["heat_j"].as_array().unwrap();
                assert_eq!(heat.len(), 12);
                assert!(heat.iter().all(|v| v.as_f64().unwrap() >= 0.0));
                assert_eq!(window["passed"], true);
                if let Some(fractions) = window["heat_fraction"].as_array() {
                    assert!(
                        (fractions.iter().map(|x| x.as_f64().unwrap()).sum::<f64>() - 1.0).abs()
                            < 1e-12
                    );
                }
            }
        }
    }
}

#[test]
fn loaded_loss_cli_preserves_outputs_and_rejects_ambiguous_arguments() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["loaded-loss-budget"],
        vec!["loaded-loss-budget", "--output", "keep.json"],
        vec!["loaded-loss-budget", "--output", "bad.wav"],
        vec!["loaded-loss-budget", "--output", "bad.json", "--unknown"],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.wav").exists());
    assert!(!scratch.0.join("bad.json").exists());
}

#[test]
fn stationary_rest_receipt_keeps_positive_control_and_physical_silence() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let r: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("stationary-rest-validation.json")).unwrap())
            .unwrap();
    assert_eq!(r["passed"], true);
    assert_eq!(r["cases"].as_array().unwrap().len(), 4);
    for c in r["cases"].as_array().unwrap() {
        assert_eq!(c["passed"], true);
        assert!(c["cold"]["peak_filtered_voltage_v"].as_f64().unwrap() > 1e-4);
        let a = &c["rest"];
        assert_eq!(a["time_step_independent"], true);
        assert!(a["peak_raw_voltage_v"].as_f64().unwrap() < 1e-9);
        assert!(a["peak_filtered_voltage_v"].as_f64().unwrap() < 1e-9);
        assert!(a["maximum_pickup_drift_m"].as_f64().unwrap() < 1e-12);
        assert!(a["max_relative_total_balance_defect"].as_f64().unwrap() < 1e-8);
        assert!(a["initial_energy_j"].as_f64().unwrap() > 0.0);
        assert_eq!(a["absolute_drive_work_j"], 0.0);
        assert_eq!(a["contact_entries"], serde_json::json!([0, 0, 0, 0]));
    }
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    for args in [
        vec!["stationary-rest"],
        vec!["stationary-rest", "--output", "keep.json"],
        vec!["stationary-rest", "--output", "bad.wav"],
        vec!["stationary-rest", "--output", "bad.json", "--unknown"],
        vec![
            "compare-loaded-bank",
            "missing.json",
            "--output",
            "bad.json",
            "--at-rest",
            "--at-rest",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    assert!(!scratch.0.join("bad.json").exists());
    assert!(!scratch.0.join("bad.wav").exists());
}

#[test]
fn loaded_bank_receipt_keeps_failed_control_and_separates_measurement_from_disagreement() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let failed = read("loaded-source-timbre-baseline-validation.json");
    assert_eq!(failed["measurement_qualified"], false);
    assert_eq!(failed["reason"], "gesture produced no hammer contact");
    let r = read("loaded-source-timbre-striking-validation.json");
    assert_eq!(r["measurement_qualified"], true);
    assert_eq!(r["reference_match_claimed"], false);
    assert_eq!(r["sources"].as_array().unwrap().len(), 5);
    assert_eq!(r["candidates"].as_array().unwrap().len(), 3);
    assert_eq!(r["comparisons"].as_array().unwrap().len(), 15);
    let prior = read("loaded-polarized-spring-tuning-validation.json");
    assert_eq!(r["candidates"][1]["takes"], prior["cases"][1]["takes"]);
    assert_eq!(r["structural_fit"], prior["structural_fit"]);
    for candidate in r["candidates"].as_array().unwrap() {
        assert_eq!(candidate["measurement_qualified"], true);
        assert!(candidate["output_error_cents"].as_f64().unwrap().abs() < 5.0);
        assert!(candidate["first_hammer_contact_seconds"].as_f64().unwrap() > 0.03);
        for take in candidate["takes"].as_array().unwrap() {
            assert_eq!(take["passed"], true);
            assert!(take["max_relative_total_balance_defect"].as_f64().unwrap() < 1e-8);
            assert!(take["max_relative_exchange_defect"].as_f64().unwrap() < 1e-10);
            assert!(take["hammer_contact_entries"].as_u64().unwrap() >= 2);
        }
        for window in candidate["convergence"]["windows"].as_array().unwrap() {
            assert!(window["voltage_relative_rmse"].as_f64().unwrap() < 0.01);
        }
    }
    for pair in r["comparisons"].as_array().unwrap() {
        assert_eq!(pair["comparison"]["within_descriptive_tolerances"], false);
        assert_eq!(pair["comparison"]["windows"].as_array().unwrap().len(), 5);
        assert!(
            pair["comparison"]["max_absolute_relative_level_difference_db"]
                .as_f64()
                .unwrap()
                > 3.0
        );
    }
    for row in r["sources"].as_array().unwrap() {
        assert_eq!(row["profile"]["qualified"], true);
        for window in row["profile"]["windows"].as_array().unwrap() {
            let sum: f64 = window["band_power_fractions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .sum();
            assert!((sum - 1.0).abs() < 1e-12);
        }
    }
}

#[test]
fn loaded_rest_receipt_preserves_sources_and_qualifies_quiet_gestures() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let r = read("loaded-source-rest-validation.json");
    let cold = read("loaded-source-timbre-striking-validation.json");
    assert_eq!(r["at_rest"], true);
    assert_eq!(r["measurement_qualified"], true);
    assert_eq!(r["reference_match_claimed"], false);
    for key in [
        "manifest",
        "sources",
        "structural_fit",
        "pedestal_speeds_m_s",
        "training_pitch_target_hz",
    ] {
        assert_eq!(r[key], cold[key], "changed frozen input: {key}");
    }
    assert_eq!(r["candidates"].as_array().unwrap().len(), 3);
    assert_eq!(r["comparisons"].as_array().unwrap().len(), 15);
    for c in r["candidates"].as_array().unwrap() {
        assert_eq!(c["measurement_qualified"], true);
        assert!(c["output_error_cents"].as_f64().unwrap().abs() < 5.0);
        assert_eq!(c["convergence"]["passed"], true);
        for take in c["takes"].as_array().unwrap() {
            assert_eq!(take["passed"], true);
            assert!(take["max_relative_total_balance_defect"].as_f64().unwrap() < 1e-8);
            assert!(take["max_relative_exchange_defect"].as_f64().unwrap() < 1e-10);
            assert!(take["hammer_contact_entries"].as_u64().unwrap() >= 2);
            let rest = &take["rest_preparation"];
            assert_eq!(rest["quiet_idle_passed"], true);
            assert!(rest["pre_key_peak_fs"].as_f64().unwrap() < 1e-10);
            assert!(rest["max_relative_force_defect"].as_f64().unwrap() <= 1e-10);
            assert!(rest["max_relative_contact_defect"].as_f64().unwrap() <= 1e-12);
            assert!(rest["initial_energy_j"].as_f64().unwrap() > 0.0);
        }
    }
    let preflight = r["strike_feasibility"]["cases"].as_array().unwrap();
    assert!(preflight[0]["first_contact_seconds"].is_null());
    assert!(preflight[1]["first_contact_seconds"].is_null());
    assert!(preflight[2]["first_contact_seconds"].as_f64().unwrap() > 0.03);
}

#[test]
fn loaded_bank_cli_preserves_outputs_and_rejects_unverified_sources_before_rendering() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"receipt").unwrap();
    fs::write(scratch.0.join("keep.wav"), b"audio").unwrap();
    fs::write(scratch.0.join("bad-manifest.json"), b"{}").unwrap();
    for args in [
        vec!["compare-loaded-bank"],
        vec![
            "compare-loaded-bank",
            "missing.json",
            "--output",
            "keep.json",
        ],
        vec![
            "compare-loaded-bank",
            "missing.json",
            "--output",
            "bad.json",
            "--preview",
            "keep.wav",
        ],
        vec![
            "compare-loaded-bank",
            "bad-manifest.json",
            "--output",
            "bad.json",
        ],
        vec![
            "compare-loaded-bank",
            "bad-manifest.json",
            "--output",
            "bad.wav",
        ],
        vec![
            "compare-loaded-bank",
            "bad-manifest.json",
            "--output",
            "bad.json",
            "--unknown",
            "new.wav",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../references/g3-pitch-reference.manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
    // All bytes are present, but the first blob identity is deliberately wrong.
    for (i, take) in manifest["takes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        let file = format!("source-{i}.wav");
        fs::write(scratch.0.join(&file), b"corrupt source").unwrap();
        take["file"] = serde_json::json!(file);
    }
    fs::write(
        scratch.0.join("corrupt.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let out = scratch.run(&[
        "compare-loaded-bank",
        "corrupt.json",
        "--output",
        "bad.json",
        "--preview",
        "new.wav",
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("blob mismatch"));
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"receipt");
    assert_eq!(fs::read(scratch.0.join("keep.wav")).unwrap(), b"audio");
    for name in ["bad.json", "bad.wav", "new.wav"] {
        assert!(!scratch.0.join(name).exists());
    }
}

#[test]
fn loaded_tuning_receipt_requires_audio_pitch_long_gesture_convergence_and_changed_ratios() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../references/loaded-polarized-spring-tuning-validation.json");
    let r: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(r["passed"], true);
    assert_eq!(r["cases"].as_array().unwrap().len(), 2);
    assert!(
        r["structural_fit"]["undamped_error_cents"]
            .as_f64()
            .unwrap()
            .abs()
            < 0.0001
    );
    assert!(
        r["tuned_structure"]["spring_center_from_root_mm"]
            .as_f64()
            .unwrap()
            < 59.5
    );
    assert_ne!(
        r["before_structure"]["fixed_root_ratios"],
        r["tuned_structure"]["fixed_root_ratios"]
    );
    for case in r["cases"].as_array().unwrap() {
        assert_eq!(case["passed"], true);
        assert_eq!(case["pitch_anchor"]["qualified"], true);
        assert_eq!(case["takes"].as_array().unwrap().len(), 2);
        if case["case"] == "tuned" {
            assert!(case["output_error_cents"].as_f64().unwrap().abs() < 5.0);
        }
        for take in case["takes"].as_array().unwrap() {
            assert_eq!(take["passed"], true);
            assert_eq!(take["heat_monotone"], true);
            assert!(take["max_relative_total_balance_defect"].as_f64().unwrap() < 1e-8);
            assert!(take["max_relative_exchange_defect"].as_f64().unwrap() < 1e-10);
            assert!(take["max_stationary_drive_energy_growth"].as_f64().unwrap() < 1e-10);
            assert!(take["hammer_contact_entries"].as_u64().unwrap() >= 2);
            assert!((0.0..1.0).contains(&take["peak"].as_f64().unwrap()));
        }
        let windows = case["voltage_convergence"]["windows"].as_array().unwrap();
        assert_eq!(windows.len(), 5);
        for window in windows {
            assert!(window["voltage_relative_rmse"].as_f64().unwrap() < 0.01);
        }
    }
    assert_eq!(r["tone_comparison"]["windows"].as_array().unwrap().len(), 3);
}

#[test]
fn loaded_tuning_cli_validates_reference_and_preserves_each_output() {
    let scratch = Scratch::new();
    for name in ["a.wav", "b-before.wav", "c.json"] {
        fs::write(scratch.0.join(name), b"preserve").unwrap();
    }
    for output in ["a.wav", "b.wav", "c.wav"] {
        assert!(
            !scratch
                .run(&["tune-electromechanical", "missing.json", "--output", output])
                .status
                .success()
        );
    }
    for name in ["a.wav", "b-before.wav", "c.json"] {
        assert_eq!(fs::read(scratch.0.join(name)).unwrap(), b"preserve");
    }
    fs::write(scratch.0.join("bad-reference.json"), b"{}").unwrap();
    for args in [
        vec!["tune-electromechanical"],
        vec![
            "tune-electromechanical",
            "bad-reference.json",
            "--output",
            "bad.wav",
        ],
        vec![
            "tune-electromechanical",
            "bad-reference.json",
            "--output",
            "bad.json",
        ],
        vec![
            "tune-electromechanical",
            "bad-reference.json",
            "--bad",
            "bad.wav",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    for name in [
        "a-before.wav",
        "a.json",
        "b.wav",
        "b.json",
        "c.wav",
        "c-before.wav",
        "bad.wav",
        "bad-before.wav",
        "bad.json",
    ] {
        assert!(!scratch.0.join(name).exists());
    }
}

#[test]
fn electromechanical_receipt_qualifies_reciprocity_load_controls_and_output_convergence() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../references/electromechanical-validation.json");
    let p: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(p["passed"], true);
    assert_eq!(p["steps_per_frame"], serde_json::json!([64, 128, 256]));
    assert_eq!(p["cases"].as_array().unwrap().len(), 8);
    for case in p["cases"].as_array().unwrap() {
        assert_eq!(case["passed"], true);
        let zero = case["case"] == "zero_flux";
        let open = case["case"] == "open_capacitive";
        assert_eq!(case["takes"].as_array().unwrap().len(), 3);
        if !zero {
            assert!(
                case["feedback_velocity_rms_difference_m_s"]
                    .as_f64()
                    .unwrap()
                    > 1e-12
            );
        }
        for take in case["takes"].as_array().unwrap() {
            assert_eq!(take["passed"], true);
            assert_eq!(take["heat_monotone"], true);
            for (field, limit) in [
                ("max_relative_total_balance_defect", 1e-8),
                ("max_relative_exchange_defect", 1e-10),
                ("max_relative_circuit_balance_defect", 1e-8),
                ("max_stationary_drive_relative_energy_growth", 1e-10),
            ] {
                assert!(take[field].as_f64().unwrap() < limit);
            }
            assert!(take["mechanical_contact_entries"][0].as_u64().unwrap() >= 2);
            assert!(take["maximum_coupling_iterations"].as_u64().unwrap() <= 16);
            if zero {
                for field in [
                    "peak_filtered_voltage_v",
                    "peak_reaction_force_n",
                    "coil_heat_j",
                ] {
                    assert_eq!(take[field], 0.0);
                }
            } else {
                assert!(take["peak_filtered_voltage_v"].as_f64().unwrap() > 1e-4);
                assert!(take["peak_reaction_force_n"].as_f64().unwrap() > 1e-8);
                assert!(take["coil_heat_j"].as_f64().unwrap() > 0.0);
                assert!(take["mechanical_pickup_work_j"].as_f64().unwrap() < 0.0);
            }
            if zero || open {
                assert_eq!(take["load_heat_j"], 0.0);
            } else {
                assert!(take["load_heat_j"].as_f64().unwrap() > 0.0);
            }
        }
        for comparison in ["coarse_vs_fine", "medium_vs_fine"] {
            let windows = case[comparison]["windows"].as_array().unwrap();
            assert_eq!(windows.len(), 4);
            for window in windows {
                for error in window["relative_rmse_voltage_current_vertical_horizontal"]
                    .as_array()
                    .unwrap()
                {
                    assert!(error.as_f64().unwrap() < 0.01);
                }
            }
        }
    }
}

#[test]
fn electromechanical_cli_rejects_bad_options_and_preserves_both_render_outputs() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"receipt").unwrap();
    fs::write(scratch.0.join("audio.wav"), b"audio").unwrap();
    for args in [
        vec!["electromechanical", "--output", "keep.json"],
        vec!["electromechanical-render", "--output", "keep.wav"],
        vec!["electromechanical-render", "--output", "audio.wav"],
        vec!["electromechanical"],
        vec!["electromechanical", "--output", "bad.wav"],
        vec!["electromechanical", "--output", "bad.json", "--unknown"],
        vec!["electromechanical", "--bad", "bad.json"],
        vec!["electromechanical-render"],
        vec!["electromechanical-render", "--output", "bad.json"],
        vec![
            "electromechanical-render",
            "--output",
            "bad.wav",
            "--refined",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"receipt");
    assert_eq!(fs::read(scratch.0.join("audio.wav")).unwrap(), b"audio");
    for gain in ["NaN", "inf", "0", "-0.1", "2", "invalid"] {
        assert!(
            !scratch
                .run(&[
                    "electromechanical-render",
                    "--output",
                    "bad.wav",
                    "--gain",
                    gain
                ])
                .status
                .success()
        );
    }
    assert!(
        !scratch
            .run(&["electromechanical-render", "--output", "bad.wav", "--gain"])
            .status
            .success()
    );
    for absent in ["keep.wav", "audio.json", "bad.wav", "bad.json"] {
        assert!(!scratch.0.join(absent).exists());
    }
}

#[test]
fn polarized_action_receipt_closes_each_plane_and_preserves_symmetry_controls() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../references/polarized-action-validation.json");
    let p: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(p["passed"], true);
    assert_eq!(p["steps_per_frame"], serde_json::json!([64, 128, 256]));
    assert_eq!(p["cases"].as_array().unwrap().len(), 8);
    for case in p["cases"].as_array().unwrap() {
        assert_eq!(case["passed"], true);
        for take in case["takes"].as_array().unwrap() {
            assert_eq!(take["passed"], true);
            assert!(take["max_relative_balance_defect"].as_f64().unwrap() < 1e-8);
            for defect in take["max_relative_plane_work_defect"].as_array().unwrap() {
                assert!(defect.as_f64().unwrap() < 1e-8);
            }
            if case["case"] == "isotropic" || case["case"] == "aligned_anisotropy" {
                assert_eq!(take["peak_displacement_xy_m"][1], 0.0);
                assert_eq!(take["plane_coupling_work_j"][1], 0.0);
            } else {
                assert!(take["peak_displacement_xy_m"][1].as_f64().unwrap() > 1e-9);
                assert!(take["orbit_covariance_rank"].as_f64().unwrap() > 1e-3);
            }
            if case["case"] == "rotated_boundary" {
                assert_eq!(take["plane_contact_work_j"][1], 0.0);
                let coupling = take["plane_coupling_work_j"][1].as_f64().unwrap();
                let stored = take["final_plane_diagonal_energy_j"][1].as_f64().unwrap();
                let heat = take["plane_diagonal_heat_j"][1].as_f64().unwrap();
                assert!(coupling > 0.0);
                assert!((coupling - stored - heat).abs() < coupling * 1e-7);
            }
        }
        for comparison in ["coarse_vs_fine", "medium_vs_fine"] {
            for window in case[comparison]["windows"].as_array().unwrap() {
                for error in window["velocity_xy_relative_rmse"].as_array().unwrap() {
                    assert!(error.as_f64().unwrap() < 0.01);
                }
                for error in window["hammer_arm_rmse_m"].as_array().unwrap() {
                    assert!(error.as_f64().unwrap() < 1e-5);
                }
            }
        }
    }
}

#[test]
fn polarized_action_cli_rejects_invalid_options_and_existing_reports() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    let out = scratch.run(&["polarized-action", "--output", "keep.json"]);
    assert!(!out.status.success());
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    for args in [
        vec!["polarized-action"],
        vec!["polarized-action", "--output", "bad.wav"],
        vec!["polarized-action", "--output", "bad.json", "--unknown"],
        vec!["polarized-action", "--bad", "bad.json"],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
    }
}

#[test]
fn action_cycle_cli_rejects_invalid_options_and_preserves_existing_reports() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    let out = scratch.run(&[
        "action-cycle",
        "--output",
        "keep.json",
        "--fast-drive",
        "--reference",
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("new .json file"));
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    for args in [
        vec!["action-cycle"],
        vec!["action-cycle", "--output", "bad.wav"],
        vec!["action-cycle", "--output", "bad.json", "--unknown"],
        vec![
            "action-cycle",
            "--output",
            "bad.json",
            "--reference",
            "--refined",
        ],
        vec![
            "action-cycle",
            "--output",
            "bad.json",
            "--fast-drive",
            "--fast-drive",
        ],
        vec!["action-cycle", "--bad", "bad.json"],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
    }
}

#[test]
fn action_cycle_receipts_preserve_failed_studies_and_reproduce_overlapping_resolution() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let read = |name: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(name)).unwrap()).unwrap()
    };
    let slow = read("persistent-action-cycle-validation.json");
    let fast = read("persistent-action-cycle-fast-refined-validation.json");
    let reference = read("persistent-action-cycle-reference-validation.json");
    assert_eq!(slow["passed"], false);
    assert_eq!(fast["passed"], false);
    assert_eq!(reference["passed"], true);
    assert_eq!(
        reference["steps_per_frame"],
        serde_json::json!([512, 1024, 2048])
    );
    assert_eq!(reference["drive_speed_m_s"], 1.5);
    assert_eq!(reference["profile"], fast["profile"]);
    assert_eq!(reference["cases"].as_array().unwrap().len(), 16);
    for (a, b) in fast["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(reference["cases"].as_array().unwrap())
    {
        for field in ["length_m", "sample_rate", "gesture"] {
            assert_eq!(a[field], b[field]);
        }
        assert_eq!(a["takes"][2], b["takes"][0]);
        assert_eq!(b["passed"], true);
        for take in b["takes"].as_array().unwrap() {
            assert_eq!(take["passed"], true);
            assert_eq!(take["behavior_passed"], true);
            assert_eq!(take["heat_monotone"], true);
            assert!(take["max_relative_balance_defect"].as_f64().unwrap() < 1e-8);
            assert!(take["max_relative_contact_work_defect"].as_f64().unwrap() < 1e-9);
            assert!(take["maximum_solver_sweeps"].as_u64().unwrap() <= 64);
            if b["gesture"] == "slack_bridle" {
                assert!(take["simultaneous_hammer_felt_steps"].as_u64().unwrap() > 0);
            }
        }
        for comparison in ["coarse_vs_fine", "medium_vs_fine"] {
            for window in b[comparison]["windows"].as_array().unwrap() {
                assert!(window["pickup_velocity_relative_rmse"].as_f64().unwrap() < 0.01);
                assert!(window["hammer_position_rmse_m"].as_f64().unwrap() < 1e-5);
                assert!(window["arm_position_rmse_m"].as_f64().unwrap() < 1e-5);
            }
        }
    }
}

#[test]
fn felt_damper_preserves_failed_coarse_evidence_and_qualifies_refinement() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let coarse: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("moving-felt-damper-validation.json")).unwrap())
            .unwrap();
    let fine: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("moving-felt-damper-refined-validation.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(coarse["passed"], false);
    assert_eq!(fine["passed"], true);
    assert_eq!(fine["steps_per_frame"], serde_json::json!([32, 64, 128]));
    assert!(
        fine["protocol"]
            .as_str()
            .unwrap()
            .contains("original 16/32/64 matrix failed")
    );
    assert_eq!(fine["cases"].as_array().unwrap().len(), 24);
    let mut failed = 0;
    for (a, b) in coarse["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fine["cases"].as_array().unwrap())
    {
        for key in ["length_m", "sample_rate", "gesture"] {
            assert_eq!(a[key], b[key]);
        }
        if a["passed"] == false {
            failed += 1;
        }
        assert_eq!(b["passed"], true);
        // Independent runs at overlapping resolutions must preserve the complete summary.
        assert_eq!(a["takes"][1], b["takes"][0]);
        assert_eq!(a["takes"][2], b["takes"][1]);
        for take in b["takes"].as_array().unwrap() {
            assert_eq!(take["passed"], true);
            assert_eq!(take["heat_monotone"], true);
            assert!(take["minimum_force_n"].as_f64().unwrap() >= 0.0);
            assert!(take["max_relative_balance_defect"].as_f64().unwrap() < 1e-8);
            if b["gesture"] == "held" {
                assert_eq!(take["contact_entries"], 0);
                assert_eq!(take["felt_heat_j"], 0.0);
            } else {
                assert!(take["contact_entries"].as_u64().unwrap() > 0);
                assert!(take["felt_heat_j"].as_f64().unwrap() > 0.0);
            }
        }
        for key in ["coarse_vs_fine", "medium_vs_fine"] {
            assert_eq!(b[key]["passed"], true);
            assert_eq!(b[key]["windows"].as_array().unwrap().len(), 3);
        }
    }
    assert_eq!(failed, 1);
}

#[test]
fn felt_damper_cli_rejects_invalid_options_and_existing_outputs() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    let out = scratch.run(&["felt-damper", "--output", "keep.json", "--refined"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("new .json file"));
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    for args in [
        vec!["felt-damper"],
        vec!["felt-damper", "--output", "bad.wav"],
        vec!["felt-damper", "--output", "bad.json", "--unknown"],
        vec!["felt-damper", "--bad", "bad.json"],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
    }
}

#[test]
fn magnetic_weighted_loss_resolution_replays_and_qualifies_every_start() {
    let scratch = Scratch::new();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../references/nonlinear-magnetic-loss-weighting-validation.json");
    let bytes = fs::read(source).unwrap();
    fs::write(scratch.0.join("source.json"), &bytes).unwrap();
    let args = [
        "magnetic-weighted-loss-resolution",
        "--input",
        "source.json",
        "--output",
        "resolution.json",
    ];
    scratch.success(&args);
    let report = scratch.json("resolution.json");
    let source: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        report["experiment"],
        "nonlinear-magnetic-weighted-loss-resolution-v1"
    );
    assert_eq!(report["weighting"], "constant_voltage");
    assert_eq!(
        report["source_git_blob_sha1"],
        "72ef1d0838548765e3955eaf673da4ed82ad0494"
    );
    assert_eq!(report["controls_passed"], true);
    assert_eq!(report["cases"].as_array().unwrap().len(), 6);
    let mut zero_noise = 0;
    let mut source_limited = 0;
    for (ci, case) in report["cases"].as_array().unwrap().iter().enumerate() {
        assert_eq!(case["sample_rate"], source["cases"][ci]["sample_rate"]);
        assert_eq!(case["observations"].as_array().unwrap().len(), 5);
        for (ri, row) in case["observations"].as_array().unwrap().iter().enumerate() {
            let d = &row["diagnosis"];
            let old = &source["cases"][ci]["observations"][ri];
            assert_eq!(d["center_replayed"], true);
            assert!(
                d["source_training_rmse_absolute_difference"]
                    .as_f64()
                    .unwrap()
                    < 1e-8
            );
            assert!(d["relative_window_profile_singular_values"].is_null());
            let norm = d["training_voltage_l2_norm"].as_f64().unwrap();
            for i in 0..2 {
                let normalized = d["pooled_relative_profile_singular_values"][i]
                    .as_f64()
                    .unwrap();
                let raw = d["raw_voltage_profile_singular_values"][i]
                    .as_f64()
                    .unwrap();
                assert!((normalized * norm / raw - 1.0).abs() < 1e-10);
                let a = d["center_scales"][i].as_f64().unwrap();
                let b = old["fit"]["estimated_scales"][i].as_f64().unwrap();
                assert!((a / b - 1.0).abs() < 1e-14);
            }
            let qualification = &d["optimizer_qualification"];
            assert_eq!(qualification["global_optimum_certified"], false);
            assert_eq!(
                qualification["selected_source_outer_start_index"],
                old["fit"]["selected_loss_start_index"]
            );
            let incomplete = old["fit"]["attempts"].as_array().unwrap().iter().any(|a| {
                !matches!(
                    a["status"].as_str(),
                    Some("residual_converged" | "no_descent_step")
                )
            });
            assert_eq!(qualification["source_all_starts_complete"], !incomplete);
            let sigma = row["noise_standard_deviation"].as_f64().unwrap();
            if sigma == 0.0 {
                zero_noise += 1;
                assert_eq!(d["local_radius_status"], "withheld_no_noise_scale");
                assert!(d["oracle_noise_scaled_singular_values"].is_null());
                assert!(d["linearized_log_radius_for_one_noise_unit"].is_null());
            } else if incomplete {
                source_limited += 1;
                assert_eq!(d["local_radius_status"], "withheld_incomplete_optimizer");
                assert!(d["linearized_log_radius_for_one_noise_unit"].is_null());
            } else if d["local_radius_status"] == "descriptive_local_radius" {
                assert_eq!(qualification["local_all_starts_complete"], true);
                assert_eq!(qualification["all_alternatives_non_improving"], true);
                let radius = d["linearized_log_radius_for_one_noise_unit"]
                    .as_f64()
                    .unwrap();
                let small = d["raw_voltage_profile_singular_values"][1]
                    .as_f64()
                    .unwrap();
                assert!((radius * small / sigma - 1.0).abs() < 1e-12);
            } else {
                assert!(d["linearized_log_radius_for_one_noise_unit"].is_null());
            }
            let evaluations = row["profile_evaluations"].as_array().unwrap();
            assert_eq!(evaluations.len(), 11);
            for e in evaluations {
                let starts = e["state_starts"].as_array().unwrap();
                assert_eq!(starts.len(), 3);
                let selected = e["selected_state_start_index"].as_u64().unwrap() as usize;
                let best = starts[selected]["training_relative_rmse"].as_f64().unwrap();
                assert!(
                    starts
                        .iter()
                        .all(|s| best <= s["training_relative_rmse"].as_f64().unwrap())
                );
            }
            let alternatives = d["alternatives"].as_array().unwrap();
            assert_eq!(alternatives.len(), 10);
            for a in alternatives {
                let i = a["evaluation_index"].as_u64().unwrap() as usize;
                assert_eq!(a["scales"], evaluations[i]["scales"]);
                assert_eq!(a["training_objective"], evaluations[i]["objective"]);
                if sigma > 0.0 {
                    let distance = a["prediction_change_voltage_l2"].as_f64().unwrap() / sigma;
                    assert!(
                        (a["oracle_noise_scaled_prediction_distance"]
                            .as_f64()
                            .unwrap()
                            / distance
                            - 1.0)
                            .abs()
                            < 1e-12
                    );
                    assert_eq!(a["within_one_noise_unit"], distance < 1.0);
                } else {
                    assert!(a["oracle_noise_scaled_prediction_distance"].is_null());
                    assert!(a["within_one_noise_unit"].is_null());
                }
            }
        }
    }
    assert_eq!(zero_noise, 6);
    assert_eq!(source_limited, 1);
    let before = fs::read(scratch.0.join("resolution.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(before, fs::read(scratch.0.join("resolution.json")).unwrap());
    let mut modified = bytes.clone();
    modified.push(b' ');
    fs::write(scratch.0.join("modified.json"), modified).unwrap();
    let out = scratch.run(&[
        "magnetic-weighted-loss-resolution",
        "--input",
        "modified.json",
        "--output",
        "bad.json",
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("pinned evidence"));
    assert!(!scratch.0.join("bad.json").exists());
    assert_eq!(fs::read(scratch.0.join("source.json")).unwrap(), bytes);
    for args in [
        vec!["magnetic-weighted-loss-resolution"],
        vec![
            "magnetic-weighted-loss-resolution",
            "--input",
            "source.json",
            "--output",
            "bad.wav",
        ],
        vec![
            "magnetic-weighted-loss-resolution",
            "--bad",
            "source.json",
            "--output",
            "bad.json",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn magnetic_loss_weighting_preflights_output_and_pinned_input() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("keep.json"), b"preserve").unwrap();
    let result = scratch.run(&[
        "magnetic-loss-weighting",
        "--input",
        "missing.json",
        "--output",
        "keep.json",
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("new .json file"));
    assert_eq!(fs::read(scratch.0.join("keep.json")).unwrap(), b"preserve");
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../references/nonlinear-magnetic-loss-noise-validation.json");
    let mut bytes = fs::read(source).unwrap();
    bytes.push(b' ');
    fs::write(scratch.0.join("modified.json"), &bytes).unwrap();
    let result = scratch.run(&[
        "magnetic-loss-weighting",
        "--input",
        "modified.json",
        "--output",
        "bad.json",
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("pinned evidence"));
    assert!(!scratch.0.join("bad.json").exists());
    assert_eq!(fs::read(scratch.0.join("modified.json")).unwrap(), bytes);
    for args in [
        vec!["magnetic-loss-weighting"],
        vec![
            "magnetic-loss-weighting",
            "--input",
            "modified.json",
            "--output",
            "bad.wav",
        ],
        vec![
            "magnetic-loss-weighting",
            "--unknown",
            "modified.json",
            "--output",
            "bad.json",
        ],
        vec![
            "magnetic-loss-weighting",
            "--input",
            "modified.json",
            "--output",
            "bad.json",
            "extra",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
    let help = scratch.run(&["--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("magnetic-loss-weighting --input"));
}

#[test]
fn magnetic_loss_weighting_receipt_preserves_pairs_and_training_selection() {
    fn same_evidence(a: &serde_json::Value, b: &serde_json::Value) {
        use serde_json::Value;
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => {
                // Baseline scores undergo an extra JSON parse/serialize cycle.
                // Retain a relative machine-precision allowance, not an error gate.
                let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
                assert!((a - b).abs() <= 8.0 * f64::EPSILON * a.abs().max(b.abs()));
            }
            (Value::Array(a), Value::Array(b)) => {
                assert_eq!(a.len(), b.len());
                for (a, b) in a.iter().zip(b) {
                    same_evidence(a, b);
                }
            }
            (Value::Object(a), Value::Object(b)) => {
                assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
                for (key, a) in a {
                    same_evidence(a, &b[key]);
                }
            }
            _ => assert_eq!(a, b),
        }
    }
    // The expensive full command is run once to produce the tracked receipt.
    // Recheck its evidence without repeating the complete outer-search matrix.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../references");
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("nonlinear-magnetic-loss-weighting-validation.json")).unwrap(),
    )
    .unwrap();
    let old: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("nonlinear-magnetic-loss-noise-validation.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(report["experiment"], "nonlinear-magnetic-loss-weighting-v1");
    assert_eq!(report["weighting"], "constant_voltage");
    assert_eq!(
        report["source_git_blob_sha1"],
        "5e32ac1255599fd8aae280eb6656d14921c0f174"
    );
    assert_eq!(report["controls_passed"], true);
    assert_eq!(report["cases"].as_array().unwrap().len(), 6);
    let mut controls = 0;
    for (case, prior) in report["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(old["cases"].as_array().unwrap())
    {
        assert_eq!(case["sample_rate"], prior["sample_rate"]);
        assert_eq!(
            case["reference_scales_for_scoring_only"],
            prior["reference_scales_for_scoring_only"]
        );
        assert_eq!(case["observations"].as_array().unwrap().len(), 5);
        for (row, previous) in case["observations"]
            .as_array()
            .unwrap()
            .iter()
            .zip(prior["observations"].as_array().unwrap())
        {
            for key in ["snr_db", "seed", "noise_standard_deviation"] {
                assert_eq!(row[key], previous[key]);
            }
            assert_eq!(row["weighting"], "constant_voltage");
            same_evidence(
                &row["relative_window_baseline"]["validation"],
                &previous["fit"]["validation"],
            );
            same_evidence(
                &row["relative_window_baseline"]["known_loss_state_control"],
                &previous["known_loss_state_control"],
            );
            assert_eq!(row["paired_weighting_comparison"]["status"], "compared");
            if row["required_control"] == true {
                controls += 1;
                assert_eq!(row["required_control_passed"], true);
            } else {
                assert!(row["required_control_passed"].is_null());
            }
            let fit = &row["fit"];
            let attempts = fit["attempts"].as_array().unwrap();
            assert_eq!(attempts.len(), 2);
            let selected = fit["selected_loss_start_index"].as_u64().unwrap() as usize;
            let objective = attempts[selected]["objective"].as_f64().unwrap();
            assert!(
                attempts
                    .iter()
                    .all(|a| objective <= a["objective"].as_f64().unwrap())
            );
            let evaluations = row["profile_evaluations"].as_array().unwrap();
            for a in attempts {
                let index = a["evaluation_index"].as_u64().unwrap() as usize;
                assert_eq!(a["scales"], evaluations[index]["scales"]);
                assert_eq!(a["objective"], evaluations[index]["objective"]);
            }
            for e in evaluations {
                let starts = e["state_starts"].as_array().unwrap();
                assert_eq!(starts.len(), 3);
                let selected = e["selected_state_start_index"].as_u64().unwrap() as usize;
                let best = starts[selected]["training_relative_rmse"].as_f64().unwrap();
                assert!(
                    starts
                        .iter()
                        .all(|s| best <= s["training_relative_rmse"].as_f64().unwrap())
                );
            }
            for i in 0..2 {
                let expected = fit["validation"]["relative_loss_errors"][i]
                    .as_f64()
                    .unwrap()
                    - previous["fit"]["validation"]["relative_loss_errors"][i]
                        .as_f64()
                        .unwrap();
                let actual = row["paired_weighting_comparison"]["relative_loss_error_change"][i]
                    .as_f64()
                    .unwrap();
                assert!((actual - expected).abs() < 1e-14);
                for field in [
                    "clean_voltage_relative_rmse",
                    "measured_voltage_relative_rmse",
                    "state_energy_norm_relative_rmse",
                ] {
                    let expected = fit["validation"]["windows"][i][field].as_f64().unwrap()
                        - previous["fit"]["validation"]["windows"][i][field]
                            .as_f64()
                            .unwrap();
                    let actual = row["paired_weighting_comparison"]["windows"][i]
                        [format!("{field}_change")]
                    .as_f64()
                    .unwrap();
                    assert!((actual - expected).abs() < 1e-14);
                }
            }
        }
    }
    assert_eq!(controls, 6);
}

#[test]
fn magnetic_loss_resolution_pins_evidence_replays_centers_and_withholds_zero_noise() {
    let scratch = Scratch::new();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../references/nonlinear-magnetic-loss-noise-validation.json");
    let bytes = fs::read(source).unwrap();
    fs::write(scratch.0.join("source.json"), &bytes).unwrap();
    let args = [
        "magnetic-loss-resolution",
        "--input",
        "source.json",
        "--output",
        "resolution.json",
    ];
    scratch.success(&args);
    let report = scratch.json("resolution.json");
    assert_eq!(
        report["experiment"],
        "nonlinear-magnetic-loss-resolution-v1"
    );
    assert_eq!(report["controls_passed"], true);
    assert_eq!(
        report["source_git_blob_sha1"],
        "5e32ac1255599fd8aae280eb6656d14921c0f174"
    );
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let mut noiseless = 0;
    for case in cases {
        let rows = case["observations"].as_array().unwrap();
        assert_eq!(rows.len(), 5);
        for row in rows {
            let d = &row["diagnosis"];
            assert_eq!(d["center_replayed"], true);
            let alternatives = d["alternatives"].as_array().unwrap();
            assert_eq!(alternatives.len(), 10);
            let evaluations = row["profile_evaluations"].as_array().unwrap();
            assert_eq!(evaluations.len(), 11);
            for eval in evaluations {
                let starts = eval["state_starts"].as_array().unwrap();
                assert_eq!(starts.len(), 3);
                let best=starts[eval["selected_state_start_index"].as_u64().unwrap() as usize]["training_relative_rmse"].as_f64().unwrap();
                for start in starts {
                    if let Some(other) = start["training_relative_rmse"].as_f64() {
                        assert!(best <= other);
                    } else {
                        assert!(start["error"].is_string());
                    }
                }
            }
            let sigma = row["noise_standard_deviation"].as_f64().unwrap();
            if sigma == 0.0 {
                noiseless += 1;
                assert_eq!(d["noise_resolution_status"], "withheld_no_noise_scale");
                assert!(d["oracle_noise_scaled_singular_values"].is_null());
                assert!(d["linearized_log_radius_for_one_noise_unit"].is_null());
            }
            let singular = d["raw_voltage_profile_singular_values"].as_array().unwrap();
            assert!(singular[0].as_f64().unwrap() >= singular[1].as_f64().unwrap());
            let center = d["center_scales"].as_array().unwrap();
            for alt in alternatives {
                assert!(alt["evaluation_index"].as_u64().unwrap() < 11);
                for j in 0..2 {
                    let expected = center[j].as_f64().unwrap()
                        * alt["log_scale_offset"][j].as_f64().unwrap().exp();
                    assert!((alt["scales"][j].as_f64().unwrap() - expected).abs() < 1e-14);
                }
                if sigma > 0.0 {
                    let distance = alt["prediction_change_voltage_l2"].as_f64().unwrap() / sigma;
                    let stored = alt["oracle_noise_scaled_prediction_distance"]
                        .as_f64()
                        .unwrap();
                    assert!((distance - stored).abs() < 1e-12 * distance.max(1.0));
                    assert_eq!(alt["within_one_noise_unit"], stored < 1.0);
                } else {
                    assert!(alt["within_one_noise_unit"].is_null());
                }
            }
        }
    }
    assert_eq!(noiseless, 6);
    let saved = fs::read(scratch.0.join("resolution.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("resolution.json")).unwrap());
    let mut modified = bytes.clone();
    modified.push(b' ');
    fs::write(scratch.0.join("modified.json"), modified).unwrap();
    assert!(
        !scratch
            .run(&[
                "magnetic-loss-resolution",
                "--input",
                "modified.json",
                "--output",
                "rejected.json"
            ])
            .status
            .success()
    );
    assert!(!scratch.0.join("rejected.json").exists());
    assert_eq!(bytes, fs::read(scratch.0.join("source.json")).unwrap());
}

#[test]
fn noisy_magnetic_losses_keep_truth_separate_and_pair_identical_noise() {
    let scratch = Scratch::new();
    let args = ["magnetic-loss-noise", "--output", "noise.json"];
    scratch.success(&args);
    let report = scratch.json("noise.json");
    assert_eq!(report["experiment"], "nonlinear-magnetic-loss-noise-v1");
    assert_eq!(report["controls_passed"], true);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let mut controls = 0;
    let mut noisy = 0;
    for case in cases {
        let rows = case["observations"].as_array().unwrap();
        assert_eq!(rows.len(), 5);
        for row in rows {
            if row["required_control"] == true {
                controls += 1;
                assert_eq!(row["required_control_passed"], true);
                assert!(row["snr_db"].is_null());
                assert_eq!(row["noise_standard_deviation"], 0.0);
            } else {
                noisy += 1;
                assert!(row["required_control_passed"].is_null());
                assert!(row["noise_standard_deviation"].as_f64().unwrap() > 0.0);
            }
            let fit = &row["fit"];
            let known = &row["known_loss_state_control"];
            if let Some(selected) = fit["selected_loss_start_index"].as_u64() {
                let attempts = fit["attempts"].as_array().unwrap();
                assert_eq!(attempts.len(), 2);
                let best = attempts[selected as usize]["objective"].as_f64().unwrap();
                for attempt in attempts {
                    if let Some(cost) = attempt["objective"].as_f64() {
                        assert!(best <= cost);
                        assert_eq!(
                            attempt["relative_loss_errors_for_scoring_only"]
                                .as_array()
                                .unwrap()
                                .len(),
                            2
                        );
                    } else {
                        assert!(attempt["error"].is_string());
                    }
                }
                let v = &fit["validation"];
                let errors = v["relative_loss_errors"].as_array().unwrap();
                assert_eq!(
                    v["both_losses_within_one_percent"],
                    errors.iter().all(|e| e.as_f64().unwrap() < 0.01)
                );
                assert_eq!(
                    v["prediction_consistent_loss_error"],
                    v["prediction_consistent"] == true
                        && v["both_losses_within_one_percent"] == false
                );
                assert_eq!(
                    fit["agreement_hides_loss_error"],
                    fit["loss_start_agreement"]["within_one_percent"] == true
                        && v["prediction_consistent_loss_error"] == true
                );
                if row["paired_state_comparison"]["status"] == "compared" {
                    for i in 0..2 {
                        assert_eq!(
                            v["windows"][i]["injected_noise_relative_rmse"],
                            known["windows"][i]["injected_noise_relative_rmse"]
                        );
                    }
                    assert_eq!(
                        row["paired_state_comparison"]["new_hidden_state_error_when_freeing_losses"],
                        known["state_within_one_percent"] == true
                            && v["prediction_consistent"] == true
                            && v["state_within_one_percent"] == false
                    );
                } else {
                    assert!(known["error"].is_string());
                }
            } else {
                assert!(fit["error"].is_string());
            }
            assert!(!row["profile_evaluations"].as_array().unwrap().is_empty());
        }
    }
    assert_eq!((controls, noisy), (6, 24));
    let saved = fs::read(scratch.0.join("noise.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("noise.json")).unwrap());
    for args in [
        vec!["magnetic-loss-noise"],
        vec!["magnetic-loss-noise", "--output", "bad.wav"],
        vec![
            "magnetic-loss-noise",
            "--output",
            "bad.json",
            "--truth",
            "1",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert!(!scratch.0.join("bad.json").exists());
}

#[test]
fn nonlinear_loss_profile_recovers_unknown_scales_and_retains_bounded_failure() {
    let scratch = Scratch::new();
    let args = ["magnetic-loss-profile", "--output", "profile.json"];
    scratch.success(&args);
    let report = scratch.json("profile.json");
    assert_eq!(report["experiment"], "nonlinear-magnetic-loss-profile-v1");
    assert_eq!(report["controls_passed"], true);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    let mut positives = 0;
    let mut negatives = 0;
    for case in cases {
        assert_eq!(case["control_passed"], true);
        let fit = &case["fit"];
        if case["negative_out_of_range_control"] == true {
            negatives += 1;
            assert_eq!(fit["prediction_consistent"], false);
            assert_eq!(fit["both_losses_within_one_percent"], false);
        } else {
            positives += 1;
            assert_eq!(fit["validation"]["known_state_recovery"], true);
            assert_eq!(fit["both_losses_within_one_percent"], true);
            assert_eq!(fit["boundary_limited"], false);
        }
        let evaluations = case["profile_evaluations"].as_array().unwrap();
        for evaluation in evaluations {
            for x in evaluation["scales"].as_array().unwrap() {
                assert!((0.25..=2.0).contains(&x.as_f64().unwrap()));
            }
            if let Some(selected) = evaluation["selected_state_start_index"].as_u64() {
                let starts = evaluation["state_starts"].as_array().unwrap();
                assert_eq!(starts.len(), 3);
                let best = starts[selected as usize]["training_relative_rmse"]
                    .as_f64()
                    .unwrap();
                for start in starts {
                    if let Some(cost) = start["training_relative_rmse"].as_f64() {
                        assert!(best <= cost);
                    } else {
                        assert!(start["error"].is_string());
                    }
                }
            } else {
                assert!(evaluation["error"].is_string());
            }
        }
        let attempts = fit["attempts"].as_array().unwrap();
        assert_eq!(attempts.len(), 2);
        let selected = fit["selected_loss_start_index"].as_u64().unwrap() as usize;
        let best = attempts[selected]["objective"].as_f64().unwrap();
        for attempt in attempts {
            if let Some(cost) = attempt["objective"].as_f64() {
                assert!(best <= cost);
                for step in attempt["history"].as_array().unwrap() {
                    if let Some(index) = step["from_evaluation_index"].as_u64() {
                        let next = step["proposal_evaluation_index"].as_u64().unwrap();
                        assert_eq!(
                            step["accepted"],
                            evaluations[next as usize]["objective"].as_f64().unwrap()
                                < evaluations[index as usize]["objective"].as_f64().unwrap()
                        );
                        for difference in step["derivative_evaluations"].as_array().unwrap() {
                            assert!(difference["log_span"].as_f64().unwrap() > 0.0);
                            assert!(
                                difference["plus_evaluation_index"].as_u64().unwrap()
                                    < evaluations.len() as u64
                            );
                            assert!(
                                difference["minus_evaluation_index"].as_u64().unwrap()
                                    < evaluations.len() as u64
                            );
                        }
                    }
                }
            } else {
                assert!(attempt["error"].is_string());
            }
        }
    }
    assert_eq!((positives, negatives), (6, 1));
    let saved = fs::read(scratch.0.join("profile.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("profile.json")).unwrap());
    for args in [
        vec!["magnetic-loss-profile"],
        vec!["magnetic-loss-profile", "--output", "bad.wav"],
        vec![
            "magnetic-loss-profile",
            "--output",
            "bad.json",
            "--truth",
            "1",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert!(!scratch.0.join("bad.json").exists());
}

#[test]
fn combined_magnetic_state_keeps_paired_controls_and_withheld_geometry() {
    let scratch = Scratch::new();
    let args = ["magnetic-state-combined", "--output", "combined.json"];
    scratch.success(&args);
    let report = scratch.json("combined.json");
    assert_eq!(report["experiment"], "nonlinear-magnetic-state-combined-v1");
    assert_eq!(report["controls_passed"], true);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let mut controls = 0;
    let mut withheld = 0;
    let mut incomplete = 0;
    for case in cases {
        let rows = case["observations"].as_array().unwrap();
        assert_eq!(rows.len(), 120);
        for row in rows {
            if row["required_control"] == true {
                controls += 1;
                assert_eq!(row["fit"]["strict_matched_recovery"], true);
            }
            if row["fit"]["status"] == "withheld_invalid_sensor" {
                withheld += 1;
                assert!(row["requested_gap_m"].as_f64().unwrap() < 0.0005);
            }
            if let Some(selected) = row["fit"]["selected_start_index"].as_u64() {
                let attempts = row["fit"]["attempts"].as_array().unwrap();
                assert_eq!(attempts.len(), 3);
                let cost = attempts[selected as usize]["training_relative_rmse"]
                    .as_f64()
                    .unwrap();
                for attempt in attempts {
                    if let Some(other) = attempt["training_relative_rmse"].as_f64() {
                        assert!(cost <= other);
                    } else {
                        assert!(attempt["error"].is_string());
                    }
                }
            } else {
                assert!(row["fit"]["error"].is_string());
            }
        }
        let pairs = case["paired_comparisons"].as_array().unwrap();
        assert_eq!(pairs.len(), 80);
        for pair in pairs {
            let combined = &rows[pair["combined_row_index"].as_u64().unwrap() as usize];
            let noise = &rows[pair["noise_only_row_index"].as_u64().unwrap() as usize];
            let sensor = &rows[pair["sensor_only_row_index"].as_u64().unwrap() as usize];
            assert_eq!(combined["true_sensor"], noise["true_sensor"]);
            assert_eq!(combined["true_sensor"], sensor["true_sensor"]);
            assert_eq!(combined["condition"]["seed"], noise["condition"]["seed"]);
            assert_eq!(
                combined["condition"]["snr_db"],
                noise["condition"]["snr_db"]
            );
            assert_eq!(
                combined["noise_standard_deviation"],
                noise["noise_standard_deviation"]
            );
            assert!(sensor["condition"]["snr_db"].is_null());
            for field in ["gap_scale", "offset_scale", "swap_law"] {
                assert_eq!(combined["condition"][field], sensor["condition"][field]);
            }
            let comparison = &pair["comparison"];
            if comparison["status"] == "withheld_incomplete_pair" {
                incomplete += 1;
                assert!(
                    [combined, noise, sensor]
                        .iter()
                        .any(|r| r["fit"]["error"].is_string())
                );
            } else {
                assert_eq!(comparison["status"], "compared");
                let accepted = combined["fit"]["oracle_prediction_consistent"] == true;
                let wrong = combined["fit"]["state_within_one_percent"] == false;
                assert_eq!(
                    comparison["mismatch_masked_by_noise"],
                    sensor["fit"]["oracle_prediction_consistent"] == false && accepted
                );
                assert_eq!(
                    comparison["prediction_consistent_state_error"],
                    accepted && wrong
                );
                assert_eq!(
                    comparison["new_hidden_state_error_vs_noise_only"],
                    accepted
                        && wrong
                        && noise["fit"]["oracle_prediction_consistent"] == true
                        && noise["fit"]["state_within_one_percent"] == true
                );
            }
        }
    }
    assert_eq!((controls, withheld, incomplete), (24, 60, 48));
    let saved = fs::read(scratch.0.join("combined.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("combined.json")).unwrap());
    for args in [
        vec!["magnetic-state-combined"],
        vec!["magnetic-state-combined", "--output", "bad.wav"],
        vec![
            "magnetic-state-combined",
            "--output",
            "bad.json",
            "--truth",
            "1",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert!(!scratch.0.join("bad.json").exists());
    assert!(!scratch.0.join("bad.wav").exists());
}

#[test]
fn magnetic_state_robustness_retains_noise_mismatch_and_training_selection() {
    let scratch = Scratch::new();
    let args = ["magnetic-state-robustness", "--output", "robustness.json"];
    scratch.success(&args);
    let report = scratch.json("robustness.json");
    assert_eq!(
        report["experiment"],
        "nonlinear-magnetic-state-robustness-v1"
    );
    assert_eq!(report["controls_passed"], true);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let mut controls = 0;
    let mut noisy = 0;
    let mut mismatch = 0;
    let mut withheld = 0;
    for case in cases {
        let rows = case["observations"].as_array().unwrap();
        assert_eq!(rows.len(), 48);
        for row in rows {
            if row["required_control"] == true {
                controls += 1;
                assert_eq!(row["fit"]["strict_matched_recovery"], true);
            } else if row["condition"]["snr_db"].is_number() {
                noisy += 1;
                assert!(row["noise_standard_deviation"].as_f64().unwrap() > 0.0);
                assert_eq!(row["true_sensor"], row["assumed_sensor"]);
            } else {
                mismatch += 1;
                assert_eq!(row["noise_standard_deviation"], 0.0);
                assert_ne!(row["true_sensor"], row["assumed_sensor"]);
            }
            let fit = &row["fit"];
            if fit["status"] == "withheld_invalid_sensor" {
                withheld += 1;
                assert_eq!(row["condition"]["name"], "assumed_gap");
                assert_eq!(row["true_sensor"]["geometry"], "close");
                assert!(row["requested_gap_m"].as_f64().unwrap() < 0.0005);
            }
            if let Some(index) = fit["selected_start_index"].as_u64() {
                let attempts = fit["attempts"].as_array().unwrap();
                assert_eq!(attempts.len(), 3);
                let selected = attempts[index as usize]["training_relative_rmse"]
                    .as_f64()
                    .unwrap();
                for attempt in attempts {
                    if let Some(other) = attempt["training_relative_rmse"].as_f64() {
                        assert!(selected <= other);
                    } else {
                        assert!(attempt["error"].is_string());
                    }
                }
                let windows = fit["windows"].as_array().unwrap();
                assert_eq!(windows.len(), 2);
                assert_eq!(
                    fit["state_within_one_percent"],
                    windows
                        .iter()
                        .all(|w| w["state_energy_norm_relative_rmse"].as_f64().unwrap() < 0.01)
                );
                for w in windows {
                    let threshold =
                        (1.25 * w["injected_noise_relative_rmse"].as_f64().unwrap()).max(1e-6);
                    // JSON round trips can shift the recomputed product by an ULP.
                    let stored = w["oracle_prediction_threshold"].as_f64().unwrap();
                    assert!((stored - threshold).abs() <= 8.0 * f64::EPSILON * threshold);
                }
            } else {
                assert!(fit["error"].is_string());
            }
        }
    }
    assert_eq!((controls, noisy, mismatch), (24, 144, 120));
    assert_eq!(withheld, 12);
    let saved = fs::read(scratch.0.join("robustness.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("robustness.json")).unwrap());
    assert!(
        !scratch
            .run(&["magnetic-state-robustness", "--output", "bad.wav"])
            .status
            .success()
    );
    assert!(!scratch.0.join("bad.wav").exists());
}

#[test]
fn nonlinear_magnetic_state_uses_training_selection_and_retains_centered_ambiguity() {
    let scratch = Scratch::new();
    let args = ["magnetic-state", "--output", "state.json"];
    scratch.success(&args);
    let report = scratch.json("state.json");
    assert_eq!(report["experiment"], "nonlinear-magnetic-state-v1");
    assert_eq!(report["controls_passed"], true);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let mut centered = 0;
    let mut baseline = 0;
    for case in cases {
        assert_eq!(case["controls_passed"], true);
        let rows = case["observations"].as_array().unwrap();
        assert_eq!(rows.len(), 6);
        for row in rows {
            if row["required_control"] == true {
                assert_eq!(row["required_control_passed"], true);
            }
            let fit = &row["fit"];
            if row["sensor"]["geometry"] == "centered" {
                centered += 1;
                assert_eq!(fit["status"], "withheld_sign_ambiguity");
                assert!(fit["voltage_energy"].as_f64().unwrap() > 0.0);
                assert!(fit["sign_symmetry_relative_rmse"].as_f64().unwrap() < 1e-12);
                continue;
            }
            if row["sensor"]["geometry"] == "baseline" {
                baseline += 1;
                assert_eq!(fit["validation"]["known_state_recovery"], true);
            }
            let attempts = fit["attempts"].as_array().unwrap();
            assert_eq!(attempts.len(), 3);
            if fit["error"].is_null() {
                let selected = fit["selected_start_index"].as_u64().unwrap() as usize;
                let objective = attempts[selected]["optimization"]["training_relative_rmse"]
                    .as_f64()
                    .unwrap();
                for attempt in attempts {
                    if let Some(other) = attempt["optimization"]["training_relative_rmse"].as_f64()
                    {
                        assert!(objective <= other);
                    }
                }
            }
            for attempt in attempts {
                if let Some(history) = attempt["optimization"]["history"].as_array() {
                    let mut previous = history[0]["objective"].as_f64().unwrap();
                    for step in &history[1..] {
                        if step["accepted"] == true {
                            let next = step["candidate_objective"].as_f64().unwrap();
                            assert!(next < previous);
                            previous = next;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(centered, 12);
    assert_eq!(baseline, 12);
    let saved = fs::read(scratch.0.join("state.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("state.json")).unwrap());
    for args in [
        vec!["magnetic-state"],
        vec!["magnetic-state", "--output", "bad.wav"],
        vec!["magnetic-state", "--output", "bad.json", "--truth", "1"],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn magnetic_loss_keeps_linear_controls_nonlinear_failures_and_centered_withholding() {
    let scratch = Scratch::new();
    let args = ["magnetic-pickup-loss", "--output", "magnetic.json"];
    scratch.success(&args);
    let report = scratch.json("magnetic.json");
    assert_eq!(report["experiment"], "magnetic-observation-loss-v1");
    assert_eq!(report["controls_passed"], true);
    assert_eq!(report["summary"]["observations"], 66);
    assert_eq!(report["summary"]["positive_controls"], 30);
    assert_eq!(report["summary"]["centered_withheld"], 12);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let rows = case["observations"].as_array().unwrap();
        assert_eq!(rows.len(), 11);
        for row in rows {
            if row["required_control"] == true {
                assert_eq!(row["required_control_passed"], true);
            }
            if row["status"] == "withheld_zero_rest_sensitivity" {
                assert_eq!(row["sensor"]["geometry"], "centered");
                assert_eq!(row["rest_sensitivity_per_velocity"], 0.0);
                assert!(row["reason"].as_str().unwrap().contains("rest sensitivity"));
                assert!(row["fit"].is_null());
                for stats in row["nonlinear_forward_diagnostics"].as_array().unwrap() {
                    assert!(stats["voltage_proxy_rms"].as_f64().unwrap() > 0.0);
                    assert_eq!(stats["rest_linearization_relative_rmse"], 1.0);
                }
            } else if row["status"] == "fitted" {
                assert_eq!(row["fit"]["fitted_initial_state_count"], 1);
                for window in row["fit"]["windows"].as_array().unwrap() {
                    assert!(
                        window["held_out_clean_observation_relative_rmse"]
                            .as_f64()
                            .is_some()
                    );
                    assert!(window["held_out_clean_pickup_relative_rmse"].is_null());
                    if row["required_control"] == true {
                        assert!(
                            window["held_out_clean_observation_relative_rmse"]
                                .as_f64()
                                .unwrap()
                                < 1e-5
                        );
                    }
                }
            }
            if row["observation"] != "mechanical_velocity" {
                assert_eq!(row["observation_units"], "uncalibrated_voltage_proxy");
                assert_eq!(
                    row["nonlinear_forward_diagnostics"]
                        .as_array()
                        .unwrap()
                        .len(),
                    2
                );
            }
        }
    }
    let saved = fs::read(scratch.0.join("magnetic.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("magnetic.json")).unwrap());
    for args in [
        vec!["magnetic-pickup-loss"],
        vec!["magnetic-pickup-loss", "--output", "bad.wav"],
        vec![
            "magnetic-pickup-loss",
            "--output",
            "bad.json",
            "--gain",
            "1",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn continuous_loss_carries_one_state_and_pairs_every_position_case() {
    let scratch = Scratch::new();
    let args = ["pickup-loss-continuity", "--output", "continuous.json"];
    scratch.success(&args);
    let report = scratch.json("continuous.json");
    assert_eq!(report["experiment"], "continuous-pickup-loss-v1");
    assert_eq!(report["controls_passed"], true);
    assert_eq!(report["summary"]["paired_observations"], 102);
    // The unchanged independent path must reproduce the established grid.
    assert_eq!(report["summary"]["independent_prediction_consistent"], 100);
    assert_eq!(report["summary"]["independent_consistent_but_biased"], 46);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let mut required = 0;
    let mut biased = 0;
    for case in cases {
        assert_eq!(case["propagation_gap_contact_free"], true);
        assert_eq!(case["controls_passed"], true);
        let rows = case["observations"].as_array().unwrap();
        assert_eq!(rows.len(), 17);
        for row in rows {
            assert_eq!(row["operator_invariants_passed"], true);
            if row["required_control"] == true {
                required += 1;
                assert_eq!(row["required_control_passed"], true);
            }
            if row["classification"]["prediction_consistent_but_biased"] == true {
                biased += 1;
            }
            let fit = &row["fit"];
            if fit["error"].is_null() {
                assert_eq!(fit["fitted_initial_state_count"], 1);
                assert_eq!(
                    fit["estimated_structural_scale"],
                    row["independent_window_reference"]["estimated_structural_scale"]
                );
                assert_eq!(fit["windows"].as_array().unwrap().len(), 2);
                assert!(fit["windows"][1]["minimum_normalized_qr_pivot"].is_null());
                assert!(
                    fit["windows"][1]["source_off_state_fit_minimum_normalized_qr_pivot"]
                        .as_f64()
                        .unwrap()
                        > 1e-8
                );
                for name in ["structural_profile", "conditional_damper_profile"] {
                    assert_eq!(fit[name]["evaluation_count"], 51);
                    assert_eq!(
                        fit[name]["coarse_evaluations"].as_array().unwrap().len(),
                        17
                    );
                }
            }
        }
    }
    assert_eq!(required, 6);
    assert_eq!(
        report["summary"]["continuous_consistent_but_biased"],
        biased
    );
    let saved = fs::read(scratch.0.join("continuous.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("continuous.json")).unwrap());
    for args in [
        vec!["pickup-loss-continuity"],
        vec!["pickup-loss-continuity", "--output", "bad.wav"],
        vec![
            "pickup-loss-continuity",
            "--output",
            "bad.json",
            "--reset",
            "1",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn small_position_errors_retain_every_fit_and_keep_prediction_separate_from_truth() {
    let scratch = Scratch::new();
    let args = ["pickup-loss-geometry", "--output", "geometry.json"];
    scratch.success(&args);
    let report = scratch.json("geometry.json");
    assert_eq!(report["experiment"], "pickup-loss-position-errors-v1");
    assert_eq!(report["controls_passed"], true);
    assert_eq!(report["summary"]["observations"], 102);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let mut biased = 0;
    for case in cases {
        assert_eq!(case["controls_passed"], true);
        let rows = case["observations"].as_array().unwrap();
        assert_eq!(rows.len(), 17);
        for (family, count) in [
            ("matched", 1),
            ("damper_only", 6),
            ("pickup_only", 6),
            ("combined", 4),
        ] {
            assert_eq!(
                rows.iter()
                    .filter(|r| r["perturbation"]["family"] == family)
                    .count(),
                count
            );
        }
        for row in rows {
            assert_eq!(row["operator_invariants_passed"], true);
            if row["required_control"] == true {
                assert_eq!(row["required_control_passed"], true);
            } else {
                assert!(row["required_control_passed"].is_null());
            }
            let c = &row["classification"];
            assert_eq!(
                c["prediction_consistent_but_biased"],
                row["fit"]["prediction_consistent"] == true
                    && c["known_scale_recovery_within_one_percent"] == false
                    && c["known_scale_relative_errors"].is_array()
            );
            if c["prediction_consistent_but_biased"] == true {
                biased += 1;
            }
            if row["fit"]["error"].is_null() {
                for name in ["structural_profile", "conditional_damper_profile"] {
                    let p = &row["fit"][name];
                    assert_eq!(p["evaluation_count"], 51);
                    assert_eq!(p["coarse_evaluations"].as_array().unwrap().len(), 17);
                    assert!(p["evaluations"].is_null());
                }
            }
        }
    }
    assert_eq!(
        report["summary"]["prediction_consistent_but_biased"],
        biased
    );
    let saved = fs::read(scratch.0.join("geometry.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("geometry.json")).unwrap());
    for args in [
        vec!["pickup-loss-geometry"],
        vec!["pickup-loss-geometry", "--output", "bad.wav"],
        vec![
            "pickup-loss-geometry",
            "--output",
            "bad.json",
            "--offset",
            "0.1",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn pickup_loss_recovers_off_grid_scales_without_truth_in_search_and_keeps_controls() {
    let scratch = Scratch::new();
    let args = ["pickup-loss", "--output", "loss.json"];
    scratch.success(&args);
    let report = scratch.json("loss.json");
    assert_eq!(report["experiment"], "profiled-pickup-loss-v1");
    assert_eq!(report["controls_passed"], true);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        assert_eq!(case["controls_passed"], true);
        let observations = case["observations"].as_array().unwrap();
        assert_eq!(observations.len(), 3);
        for (observation, label) in observations.iter().zip([
            "matched_noiseless",
            "matched_noise_1pct",
            "wrong_damper_position",
        ]) {
            assert_eq!(observation["observation"], label);
            if observation["required_control"] == true {
                assert_eq!(observation["required_control_passed"], true);
                assert_eq!(
                    observation["fit"]["known_scale_recovery_within_one_percent"],
                    true
                );
                for error in observation["fit"]["known_scale_relative_errors"]
                    .as_array()
                    .unwrap()
                {
                    assert!(error.as_f64().unwrap() < 0.001);
                }
            } else {
                assert!(observation["required_control_passed"].is_null());
            }
            let fit = &observation["fit"];
            if fit["error"].is_null() {
                assert_eq!(fit["windows"].as_array().unwrap().len(), 2);
                for name in ["structural_profile", "conditional_damper_profile"] {
                    let evaluations = fit[name]["evaluations"].as_array().unwrap();
                    assert_eq!(evaluations.len(), 51);
                    for e in evaluations {
                        assert!((0.25..=2.0).contains(&e[0].as_f64().unwrap()));
                        assert!(e[1].as_f64().unwrap() >= 0.0);
                    }
                }
                assert!(
                    fit["local_sensitivity"]["minimum_to_maximum_singular_ratio"]
                        .as_f64()
                        .is_some_and(|r| (0.0..=1.0).contains(&r))
                );
                assert!(fit["prediction_consistent"].is_boolean());
            }
        }
    }
    let saved = fs::read(scratch.0.join("loss.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("loss.json")).unwrap());
    for args in [
        vec!["pickup-loss"],
        vec!["pickup-loss", "--output", "bad.wav"],
        vec!["pickup-loss", "--output", "bad.json", "--truth", "1"],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn pickup_history_infers_states_with_held_out_prediction_and_retains_reductions() {
    let scratch = Scratch::new();
    let args = ["pickup-state", "--output", "state.json"];
    scratch.success(&args);
    let report = scratch.json("state.json");
    assert_eq!(report["experiment"], "dynamic-pickup-state-v1");
    assert_eq!(report["controls_passed"], true);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    let mut required = 0;
    for case in cases {
        assert_eq!(case["controls_passed"], true);
        let observations = case["observations"].as_array().unwrap();
        assert_eq!(observations.len(), 18);
        for damped in [false, true] {
            for count in [3, 6, 9] {
                for noise in [0.0, 0.001, 0.01] {
                    assert_eq!(
                        observations
                            .iter()
                            .filter(|o| o["damper_on"] == damped
                                && o["retained_modes"] == count
                                && o["training_noise_relative_rms"] == noise)
                            .count(),
                        1
                    );
                }
            }
        }
        for observation in observations {
            assert_eq!(observation["contact_free"], true);
            assert_eq!(
                observation["training_samples"],
                observation["held_out_samples"]
            );
            if observation["required_control"] == true {
                required += 1;
                assert_eq!(observation["required_control_passed"], true);
                assert!(
                    observation["fit"]["held_out_clean_pickup_relative_rmse"]
                        .as_f64()
                        .unwrap()
                        < 1e-6
                );
                assert!(
                    observation["fit"]["held_out_full_state_energy_norm_relative_rmse"]
                        .as_f64()
                        .unwrap()
                        < 1e-5
                );
            } else {
                assert!(observation["required_control_passed"].is_null());
            }
            if observation["fit"]["error"].is_null() {
                assert_eq!(
                    observation["fit"]["held_out_per_mode_energy_norm_relative_rmse"]
                        .as_array()
                        .unwrap()
                        .len(),
                    9
                );
                assert_eq!(
                    observation["fit"]["inferred_initial_energy_coordinates"]
                        .as_array()
                        .unwrap()
                        .len(),
                    observation["retained_modes"].as_u64().unwrap() as usize * 2
                );
            }
        }
    }
    assert_eq!(required, 12);
    let saved = fs::read(scratch.0.join("state.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("state.json")).unwrap());
    for args in [
        vec!["pickup-state"],
        vec!["pickup-state", "--output", "bad.wav"],
        vec!["pickup-state", "--output", "bad.json", "--modes", "3"],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn reduced_mechanical_loss_keeps_shared_trajectory_controls_and_all_observations() {
    let scratch = Scratch::new();
    let args = ["reduced-mechanical-loss", "--output", "reduced.json"];
    scratch.success(&args);
    let report = scratch.json("reduced.json");
    assert_eq!(report["controls_passed"], true);
    assert_eq!(report["modes"].as_array().unwrap().len(), 9);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        assert_eq!(case["full_state_rows_match_original"], true);
        let observations = case["observations"].as_array().unwrap();
        assert_eq!(observations.len(), 6);
        for (index, label) in [
            "full_state",
            "lowest_1_modes",
            "lowest_3_modes",
            "lowest_6_modes",
            "lowest_9_modes",
            "instantaneous_pickup_lift",
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(observations[index]["observation"], label);
            assert_eq!(observations[index]["rows"].as_array().unwrap().len(), 6);
            assert!(observations[index]["fit"]["internally_consistent"].is_boolean());
            assert!(observations[index]["fit"]["known_scale_recovery"].is_boolean());
        }
        for index in [0, 4] {
            assert_eq!(observations[index]["fit"]["internally_consistent"], true);
            assert_eq!(observations[index]["fit"]["known_scale_recovery"], true);
        }
    }
    let original = fs::read(scratch.0.join("reduced.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(original, fs::read(scratch.0.join("reduced.json")).unwrap());
    for args in [
        vec!["reduced-mechanical-loss"],
        vec!["reduced-mechanical-loss", "--output", "bad.wav"],
        vec![
            "reduced-mechanical-loss",
            "--output",
            "bad.json",
            "--modes",
            "3",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn mechanical_loss_recovers_known_scales_with_held_out_rows_and_output_protection() {
    let scratch = Scratch::new();
    let args = ["mechanical-loss", "--output", "loss.json"];
    scratch.success(&args);
    let report = scratch.json("loss.json");
    assert_eq!(report["all_cases_qualified"], true);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        assert_eq!(case["qualified"], true);
        assert_eq!(case["rows"].as_array().unwrap().len(), 6);
        assert_eq!(case["fit_row_indices"], serde_json::json!([0, 1, 3]));
        assert_eq!(case["held_out_row_indices"], serde_json::json!([2, 4, 5]));
        for row in case["rows"].as_array().unwrap() {
            assert_eq!(row["contact_free"], true);
        }
        assert!(
            case["omitted_damper_control"]["relative_energy_rmse"]
                .as_f64()
                .unwrap()
                > 0.01
        );
        assert!(case["held_out_relative_energy_rmse"].as_f64().unwrap() < 0.005);
        for name in ["structural", "damper"] {
            let actual = case[format!("estimated_{name}_scale")].as_f64().unwrap();
            let expected = case[format!("known_{name}_scale")].as_f64().unwrap();
            assert!((actual / expected - 1.0).abs() < 0.01);
        }
    }
    let saved = fs::read(scratch.0.join("loss.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("loss.json")).unwrap());
    for args in [
        vec!["mechanical-loss"],
        vec!["mechanical-loss", "--output", "bad.wav"],
        vec!["mechanical-loss", "--output", "bad.json", "--hold", "0.1"],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn band_event_grid_retains_all_outcomes_without_identifying_natural_sustain() {
    let scratch = Scratch::new();
    let args = ["study-band-events", "--output", "events.json"];
    scratch.success(&args);
    let report = scratch.json("events.json");
    assert_eq!(report["controls_passed"], true);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 162);
    for rate in [44100, 48000, 96000] {
        for event in ["onset", "release", "loss_increase", "loss_decrease"] {
            let matching: Vec<_> = cases
                .iter()
                .filter(|c| c["sample_rate_hz"] == rate && c["event"] == event)
                .collect();
            assert_eq!(matching.len(), 13);
            assert_eq!(matching.first().unwrap()["requested_event_seconds"], 0.0);
            assert_eq!(matching.last().unwrap()["requested_event_seconds"], 0.204);
        }
    }
    for case in cases {
        assert_eq!(case["measurements"].as_array().unwrap().len(), 2);
        assert_eq!(
            case["natural_sustain_status"],
            "not_identified_by_measurement"
        );
        assert_eq!(
            case["accepted_event_in_measurement_interval"],
            case["paired_qualified"] == true
                && case["ground_truth_region"] == "measurement_interval"
        );
        if case["paired_qualified"] == true {
            assert!(case["paired_rejections"].as_array().unwrap().is_empty());
            for m in case["measurements"].as_array().unwrap() {
                assert_eq!(m["qualified"], true);
            }
        } else {
            assert!(case["conditional_amplitude_decay_per_second"].is_null());
        }
    }
    let saved = fs::read(scratch.0.join("events.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("events.json")).unwrap());
    for args in [
        vec!["study-band-events"],
        vec!["study-band-events", "--output", "bad.wav"],
        vec!["study-band-events", "--output", "bad.json", "--time", "0.1"],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn band_envelope_study_retains_onset_false_acceptance_and_requires_both_windows() {
    let scratch = Scratch::new();
    let args = ["validate-band-envelope", "--output", "study.json"];
    let out = scratch.run(&args);
    // The original per-window onset expectation fails: this is retained evidence,
    // not a reason to relax a gate or silently mark the study as fully validated.
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("retained failed expectations"));
    let report = scratch.json("study.json");
    assert_eq!(report["all_expectations_passed"], false);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 66);
    let failures: Vec<_> = cases
        .iter()
        .filter(|c| c["expectation_passed"] != true)
        .collect();
    assert_eq!(failures.len(), 3);
    for failure in failures {
        assert_eq!(failure["probe"], "onset_inside_interval");
        assert_eq!(
            failure["filtered"]["measurement"]["options"]["window_seconds"],
            0.064
        );
        assert_eq!(failure["filtered"]["measurement"]["qualified"], true);
    }
    // Apply the already-declared source pilot agreement criterion descriptively.
    // This does not erase the failed individual-window expectations above.
    for [a, b] in cases.as_chunks::<2>().0 {
        assert_eq!(a["probe"], b["probe"]);
        assert_eq!(a["sample_rate_hz"], b["sample_rate_hz"]);
        let ma = &a["filtered"]["measurement"];
        let mb = &b["filtered"]["measurement"];
        let rates = ma["provisional_fit"]["amplitude_decay_per_second"]
            .as_f64()
            .zip(mb["provisional_fit"]["amplitude_decay_per_second"].as_f64());
        let pair_qualified = ma["qualified"] == true
            && mb["qualified"] == true
            && rates
                .is_some_and(|(x, y)| (x - y).abs() <= 0.5_f64.max(0.15 * x.abs().max(y.abs())));
        assert_eq!(
            pair_qualified,
            a["expected_rejection"].is_null(),
            "{}",
            a["probe"]
        );
    }
    let original = fs::read(scratch.0.join("study.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(original, fs::read(scratch.0.join("study.json")).unwrap());
    for args in [
        vec!["validate-band-envelope"],
        vec![
            "validate-band-envelope",
            "--output",
            "bad.json",
            "--unknown",
        ],
        vec!["validate-band-envelope", "--output", "bad.wav"],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn short_envelope_study_and_wav_observations_preserve_outputs_and_sources() {
    let scratch = Scratch::new();
    scratch.success(&["validate-short-envelope", "--output", "study.json"]);
    let report = scratch.json("study.json");
    assert_eq!(report["all_expectations_passed"], true);
    assert_eq!(report["cases"].as_array().unwrap().len(), 18);
    scratch.success(&[
        "render",
        "--output",
        "source.wav",
        "--seconds",
        "0.25",
        "--hold",
        "0.2",
    ]);
    let original = fs::read(scratch.0.join("source.wav")).unwrap();
    let args = [
        "short-envelope",
        "source.wav",
        "--output",
        "observation.json",
        "--frequencies-hz",
        "1620,1568",
        "--start",
        "0.02",
        "--end",
        "0.18",
    ];
    scratch.success(&args);
    let observation = scratch.json("observation.json");
    assert_eq!(
        observation["measurement"]["method"],
        "joint-quadratic-carrier-envelope-v1"
    );
    assert!(
        !observation["measurement"]["points"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let before = fs::read(scratch.0.join("observation.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(
        before,
        fs::read(scratch.0.join("observation.json")).unwrap()
    );
    for args in [
        vec![
            "validate-short-envelope",
            "--output",
            "bad.json",
            "--unknown",
            "1",
        ],
        vec![
            "validate-short-envelope",
            "--output",
            "bad.json",
            "--output",
            "other.json",
        ],
        vec!["short-envelope", "source.wav", "--output", "bad.json"],
        vec![
            "short-envelope",
            "source.wav",
            "--output",
            "bad.json",
            "--frequencies-hz",
            "1620,1620",
            "--start",
            "0.02",
            "--end",
            "0.18",
        ],
        vec![
            "short-envelope",
            "source.wav",
            "--output",
            "bad.json",
            "--frequencies-hz",
            "NaN",
            "--start",
            "0.02",
            "--end",
            "0.18",
        ],
        vec!["validate-short-envelope", "--output", "bad.wav"],
    ] {
        assert!(!scratch.run(&args).status.success());
        for name in ["bad.json", "other.json", "bad.wav"] {
            assert!(!scratch.0.join(name).exists());
        }
    }
    assert_eq!(original, fs::read(scratch.0.join("source.wav")).unwrap());
}

#[test]
fn source_envelopes_keep_missing_components_and_verify_receipt_and_audio_bytes() {
    check_pinned_source_envelopes(false);
}

#[test]
fn short_source_envelopes_keep_missing_components_and_verify_receipt_and_audio_bytes() {
    check_pinned_source_envelopes(true);
}

fn check_pinned_source_envelopes(short: bool) {
    let command = if short {
        "observe-short-source-envelopes"
    } else {
        "observe-source-envelopes"
    };
    let scratch = Scratch::new();
    let hash = |name: &str| {
        let out = Command::new("git")
            .current_dir(&scratch.0)
            .args(["hash-object", "--no-filters", "--", name])
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };
    let mut takes = Vec::new();
    let mut inputs = Vec::new();
    let mut original = Vec::new();
    for (index, velocity) in ["0.3", "0.6", "0.9"].into_iter().enumerate() {
        let file = format!("source-{index}.wav");
        scratch.success(&[
            "render",
            "--output",
            &file,
            "--note",
            "55",
            "--velocity",
            velocity,
            "--seconds",
            if short { "0.25" } else { "2" },
            "--hold",
            if short { "0.2" } else { "1.5" },
        ]);
        original.push(fs::read(scratch.0.join(&file)).unwrap());
        takes.push(serde_json::json!({"id":format!("take-{index}"),"file":file,"git_blob_sha1":hash(&file)}));
        inputs.push(serde_json::json!({"id":format!("take-{index}"),"pitch_anchor":{"qualified":true,"frequency_hz":196.0},
            "observation_windows":[{"label":"attack_128_ms","observed_samples":6144,"capacity_limited":false,
                "accepted_peaks":([196.1,393.0,589.0].map(|f|serde_json::json!({"frequency_hz":f,"ambiguous_neighbor":false,"capacity_limited":false})))}]}));
    }
    let receipt = serde_json::json!({"schema_version":1,"experiment":"cross-note-spectral-hypotheses-v1",
        "manifest":{"groups":[{"note":55,"takes":takes}]},"groups":[{"note":55,"inputs":inputs}]});
    fs::write(
        scratch.0.join("evidence.json"),
        serde_json::to_vec(&receipt).unwrap(),
    )
    .unwrap();
    let mut manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../references/source-envelope.manifest.json"
    ))
    .unwrap();
    manifest["evidence_file"] = serde_json::json!("evidence.json");
    manifest["evidence_git_blob_sha1"] = serde_json::json!(hash("evidence.json"));
    if short {
        manifest["start_seconds"] = serde_json::json!(0.02);
        manifest["end_seconds"] = serde_json::json!(0.18);
    }
    fs::write(
        scratch.0.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let args = [command, "manifest.json", "--output", "result.json"];
    scratch.success(&args);
    let result = scratch.json("result.json");
    assert_eq!(result["takes"].as_array().unwrap().len(), 3);
    for take in result["takes"].as_array().unwrap() {
        let obs = &take["observation"];
        if short {
            let fundamental = &obs["components"][0];
            assert_eq!(
                fundamental["declared_frequencies_hz"],
                serde_json::json!([196.0, 393.0, 589.0])
            );
            assert_eq!(fundamental["measurements"].as_array().unwrap().len(), 2);
            assert_eq!(
                fundamental["measurements"][0]["options"]["window_seconds"],
                0.032
            );
            assert_eq!(
                fundamental["measurements"][1]["options"]["window_seconds"],
                0.064
            );
        }
        assert_eq!(
            obs["conditional_weak_mixing_relation"]["amplitude_decay_sum_residual_per_second"],
            serde_json::Value::Null
        );
        for c in &obs["components"].as_array().unwrap()[1..] {
            assert_eq!(c["selected_frequency_hz"], serde_json::Value::Null);
            assert_eq!(c["selection_rejections"][0], "missing_prior_peak");
            assert!(c["measurements"].as_array().unwrap().is_empty());
        }
    }
    let saved = fs::read(scratch.0.join("result.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(saved, fs::read(scratch.0.join("result.json")).unwrap());
    for (i, bytes) in original.iter().enumerate() {
        assert_eq!(
            *bytes,
            fs::read(scratch.0.join(format!("source-{i}.wav"))).unwrap()
        );
    }
    fs::write(scratch.0.join("source-0.wav"), b"changed source").unwrap();
    let invalid = [command, "manifest.json", "--output", "bad.json"];
    let out = scratch.run(&invalid);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("blob mismatch"));
    assert!(!scratch.0.join("bad.json").exists());
    fs::write(scratch.0.join("source-0.wav"), &original[0]).unwrap();
    fs::write(scratch.0.join("evidence.json"), b"changed evidence").unwrap();
    let out = scratch.run(&invalid);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("evidence blob mismatch"));
    assert!(!scratch.0.join("bad.json").exists());
    manifest["unknown"] = serde_json::json!(true);
    fs::write(
        scratch.0.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(!scratch.run(&invalid).status.success());
    assert!(!scratch.0.join("bad.json").exists());
}

#[test]
fn component_envelope_roundtrip_validation_and_output_protection() {
    let scratch = Scratch::new();
    scratch.success(&["validate-envelope", "--output", "validation.json"]);
    let report = scratch.json("validation.json");
    assert_eq!(report["all_expectations_passed"], true);
    assert_eq!(report["cases"].as_array().unwrap().len(), 18);
    let path = scratch.0.join("source.wav");
    // Independent PCM16 fixture; the lab writer produces float WAVs.
    let mut pcm = Vec::from(&b"RIFF"[..]);
    pcm.extend_from_slice(&(36_u32 + 96000 * 2).to_le_bytes());
    pcm.extend_from_slice(b"WAVEfmt ");
    pcm.extend_from_slice(&16_u32.to_le_bytes());
    pcm.extend_from_slice(&1_u16.to_le_bytes());
    pcm.extend_from_slice(&1_u16.to_le_bytes());
    pcm.extend_from_slice(&48000_u32.to_le_bytes());
    pcm.extend_from_slice(&96000_u32.to_le_bytes());
    pcm.extend_from_slice(&2_u16.to_le_bytes());
    pcm.extend_from_slice(&16_u16.to_le_bytes());
    pcm.extend_from_slice(b"data");
    pcm.extend_from_slice(&(96000_u32 * 2).to_le_bytes());
    for i in 0..96000 {
        let t = i as f64 / 48000.0;
        let x = 0.2 * (-3.0 * t).exp() * (std::f64::consts::TAU * 1426.7578125 * t + 0.73).cos();
        pcm.extend_from_slice(&((x * 32767.0).round() as i16).to_le_bytes());
    }
    fs::write(&path, pcm).unwrap();
    let source = fs::read(&path).unwrap();
    let args = [
        "component-envelope",
        "source.wav",
        "--output",
        "measurement.json",
        "--frequency-hz",
        "1426.7578125",
        "--start",
        "0.1",
        "--end",
        "1.5",
    ];
    scratch.success(&args);
    let measurement = scratch.json("measurement.json");
    assert_eq!(measurement["measurement"]["qualified"], true);
    assert!(
        (measurement["measurement"]["provisional_fit"]["amplitude_decay_per_second"]
            .as_f64()
            .unwrap()
            - 3.0)
            .abs()
            < 0.01
    );
    let before = fs::read(scratch.0.join("measurement.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(
        before,
        fs::read(scratch.0.join("measurement.json")).unwrap()
    );
    for invalid in [
        vec![
            "validate-envelope",
            "--output",
            "bad.json",
            "--unknown",
            "1",
        ],
        vec![
            "validate-envelope",
            "--output",
            "bad.json",
            "--output",
            "other.json",
        ],
        vec!["component-envelope", "source.wav", "--output", "bad.json"],
        vec![
            "component-envelope",
            "source.wav",
            "--output",
            "bad.json",
            "--frequency-hz",
            "NaN",
            "--start",
            "0.1",
            "--end",
            "1.5",
        ],
        vec![
            "component-envelope",
            "source.wav",
            "--output",
            "bad.json",
            "--unknown",
            "1",
        ],
        vec!["validate-envelope", "--output", "bad.wav"],
    ] {
        assert!(!scratch.run(&invalid).status.success());
        for name in ["bad.json", "other.json", "bad.wav"] {
            assert!(!scratch.0.join(name).exists());
        }
    }
    assert_eq!(source, fs::read(&path).unwrap());
}

#[test]
fn register_evidence_preserves_outputs_and_rejects_changed_source_bytes() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("report.json"), b"preserve").unwrap();
    assert!(
        !scratch
            .run(&[
                "observe-register",
                "missing.json",
                "--output",
                "report.json"
            ])
            .status
            .success()
    );
    assert_eq!(
        fs::read(scratch.0.join("report.json")).unwrap(),
        b"preserve"
    );
    fs::write(scratch.0.join("bad.json"), b"{}").unwrap();
    assert!(
        !scratch
            .run(&["observe-register", "bad.json", "--output", "new.json"])
            .status
            .success()
    );
    assert!(!scratch.0.join("new.json").exists());
    let mut m: serde_json::Value = serde_json::from_str(include_str!(
        "../../../references/register-families.manifest.json"
    ))
    .unwrap();
    m["groups"][0]["takes"][0]["file"] = serde_json::json!("altered.wav");
    fs::write(scratch.0.join("altered.wav"), b"changed source bytes").unwrap();
    fs::write(
        scratch.0.join("manifest.json"),
        serde_json::to_vec(&m).unwrap(),
    )
    .unwrap();
    let out = scratch.run(&["observe-register", "manifest.json", "--output", "new.json"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("blob mismatch"));
    assert!(!scratch.0.join("new.json").exists());
}

#[test]
fn spectral_families_preserve_outputs_and_reject_invalid_or_mismatched_inputs() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("output.json"), b"preserve").unwrap();
    assert!(
        !scratch
            .run(&[
                "observe-families",
                "missing.json",
                "--output",
                "output.json"
            ])
            .status
            .success()
    );
    assert_eq!(
        fs::read(scratch.0.join("output.json")).unwrap(),
        b"preserve"
    );
    fs::write(scratch.0.join("manifest.json"), b"{}").unwrap();
    assert!(
        !scratch
            .run(&["observe-families", "manifest.json", "--output", "new.json"])
            .status
            .success()
    );
    assert!(!scratch.0.join("new.json").exists());
    let mut m: serde_json::Value = serde_json::from_str(include_str!(
        "../../../references/g3-spectral-families.manifest.json"
    ))
    .unwrap();
    m["takes"][0]["file"] = serde_json::json!("source.wav");
    fs::write(scratch.0.join("source.wav"), b"incorrect bytes").unwrap();
    fs::write(
        scratch.0.join("manifest.json"),
        serde_json::to_vec(&m).unwrap(),
    )
    .unwrap();
    let out = scratch.run(&["observe-families", "manifest.json", "--output", "new.json"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("blob mismatch"));
    assert!(!scratch.0.join("new.json").exists());
}

#[test]
fn geometry_study_preserves_outputs_and_rejects_unqualified_reference() {
    for command in [
        "sweep-tuned-geometry",
        "sweep-spring-span",
        "sweep-tine-taper",
        "sweep-tine-transition",
    ] {
        let scratch = Scratch::new();
        fs::write(scratch.0.join("study.json"), b"preserve").unwrap();
        let args = [command, "missing.json", "--output", "study.json"];
        let out = scratch.run(&args);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("new .json"));
        assert_eq!(fs::read(scratch.0.join("study.json")).unwrap(), b"preserve");
        fs::write(scratch.0.join("invalid.json"), b"{}").unwrap();
        let out = scratch.run(&[command, "invalid.json", "--output", "new.json"]);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("qualified G3"));
        assert!(!scratch.0.join("new.json").exists());
    }
}

#[test]
fn modal_observation_verifies_bytes_keeps_native_windows_and_preserves_output() {
    let scratch = Scratch::new();
    scratch.success(&[
        "render",
        "--output",
        "source.wav",
        "--note",
        "55",
        "--seconds",
        "1",
        "--hold",
        "0.9",
    ]);
    let hash = Command::new("git")
        .args(["hash-object", "--", "source.wav"])
        .current_dir(&scratch.0)
        .output()
        .unwrap();
    assert!(hash.status.success());
    let hash = String::from_utf8(hash.stdout).unwrap();
    let args = [
        "observe-modes",
        "source.wav",
        "--blob-sha1",
        hash.trim(),
        "--fundamental",
        "196.4",
        "--modes",
        "196.4,1361.9,3686.4",
        "--output",
        "observation.json",
    ];
    scratch.success(&args);
    let report = scratch.json("observation.json");
    assert_eq!(report["git_blob_sha1"], hash.trim());
    assert_eq!(report["observation"]["sample_rate"], 48000);
    assert_eq!(
        report["observation"]["windows"][2]["spectrum"]["observed_samples"],
        24576
    );
    let preserved = fs::read(scratch.0.join("observation.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(
        fs::read(scratch.0.join("observation.json")).unwrap(),
        preserved
    );
    fs::write(scratch.0.join("source.wav"), b"different bytes").unwrap();
    let mut mismatch = args;
    mismatch[9] = "mismatch.json";
    let result = scratch.run(&mismatch);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("blob mismatch"));
    assert!(!scratch.0.join("mismatch.json").exists());
    mismatch[2] = "--unknown";
    assert!(!scratch.run(&mismatch).status.success());
}
impl Scratch {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        // Windows clock resolution can give parallel tests the same timestamp.
        // Reserve each directory atomically; never adopt an existing directory.
        for _ in 0..64 {
            let id = SCRATCH_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("rf-tines-cli-{}-{unique}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("failed to reserve test directory: {error}"),
            }
        }
        panic!("could not reserve a unique test directory");
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_rf-tines-lab"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn success(&self, args: &[&str]) {
        let result = self.run(args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fn json(&self, name: &str) -> serde_json::Value {
        serde_json::from_slice(&fs::read(self.0.join(name)).unwrap()).unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn pitch_reference_roundtrip_verifies_content_and_preserves_outputs() {
    let scratch = Scratch::new();
    let mut takes = Vec::new();
    for (i, velocity) in ["0.2", "0.5", "0.8"].into_iter().enumerate() {
        let file = format!("take-{i}.wav");
        scratch.success(&[
            "render",
            "--output",
            &file,
            "--note",
            "55",
            "--velocity",
            velocity,
            "--seconds",
            "2",
            "--hold",
            "1.9",
        ]);
        let hash = Command::new("git")
            .args(["hash-object", "--", &file])
            .current_dir(&scratch.0)
            .output()
            .unwrap();
        assert!(hash.status.success());
        takes.push(serde_json::json!({"id":format!("take-{i}"),"file":file,
            "git_blob_sha1":String::from_utf8(hash.stdout).unwrap().trim(),
            "role":if i==2 {"validation"} else {"training"}}));
    }
    let mut manifest = serde_json::json!({"schema_version":1,"note":55,"source":"Synthetic test",
        "source_revision":"generated","instrument":"Production research engine","processing":"None",
        "capture_gain":"Fixed","prior_exposure":"Synthetic regression only","takes":takes});
    let manifest_path = scratch.0.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let args = [
        "prepare-pitch-reference",
        "manifest.json",
        "--output",
        "target.json",
    ];
    scratch.success(&args);
    let report = scratch.json("target.json");
    assert_eq!(report["reference_qualification_passed"], true);
    assert!((report["training_target_hz"].as_f64().unwrap() - 195.9977).abs() < 0.1);
    let saved = fs::read(scratch.0.join("target.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(fs::read(scratch.0.join("target.json")).unwrap(), saved);
    manifest["takes"][0]["git_blob_sha1"] =
        serde_json::json!("0000000000000000000000000000000000000000");
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(
        !scratch
            .run(&[
                "prepare-pitch-reference",
                "manifest.json",
                "--output",
                "bad.json"
            ])
            .status
            .success()
    );
    assert!(!scratch.0.join("bad.json").exists());
}

#[test]
fn hammer_comparison_preserves_every_destination_and_rejects_invalid_work() {
    for name in [
        "study.wav",
        "study-elastic.wav",
        "study-rate.wav",
        "study.json",
    ] {
        let scratch = Scratch::new();
        fs::write(scratch.0.join(name), b"preserve existing output").unwrap();
        assert!(
            !scratch
                .run(&["compare-modal-hammers", "--output", "study.wav"])
                .status
                .success()
        );
        assert_eq!(
            fs::read(scratch.0.join(name)).unwrap(),
            b"preserve existing output"
        );
        assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 1);
    }
    let scratch = Scratch::new();
    for tail in [["--seconds", "NaN"], ["--speed", "2"], ["--note", "57"]] {
        assert!(
            !scratch
                .run(&[
                    "compare-modal-hammers",
                    "--output",
                    "study.wav",
                    tail[0],
                    tail[1]
                ])
                .status
                .success()
        );
        assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 0);
    }
}

#[test]
fn modal_audio_rejects_invalid_options_and_preserves_both_output_paths() {
    let scratch = Scratch::new();
    for args in [
        vec![
            "render-memory-modal",
            "--output",
            "preview.wav",
            "--seconds",
            "NaN",
        ],
        vec![
            "render-memory-modal",
            "--output",
            "preview.wav",
            "--hold",
            "2",
        ],
        vec![
            "render-memory-modal",
            "--output",
            "preview.wav",
            "--note",
            "57",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
    }
    assert!(!scratch.0.join("preview.wav").exists());
    assert!(!scratch.0.join("preview.json").exists());
    for (name, other) in [("a.wav", "a.json"), ("b.json", "b.wav")] {
        fs::write(scratch.0.join(name), b"preserve existing output").unwrap();
        let wav = if name.ends_with("wav") { name } else { other };
        assert!(
            !scratch
                .run(&["render-memory-modal", "--output", wav])
                .status
                .success()
        );
        assert_eq!(
            fs::read(scratch.0.join(name)).unwrap(),
            b"preserve existing output"
        );
        assert!(!scratch.0.join(other).exists());
    }
}

#[test]
fn listening_wavs_match_global_rms_stay_below_ceiling_and_preserve_outputs() {
    let scratch = Scratch::new();
    let args = ["pickup-listening", "--output", "study"];
    scratch.success(&args);
    let report = scratch.json("study/report.json");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["faults"], 0);
    assert_eq!(
        report["diagnostics"]["isolated_keys"]
            .as_array()
            .unwrap()
            .len(),
        73
    );
    assert_eq!(report["frames"], 24 * 44100);
    let mut observed_rms = Vec::new();
    for name in [
        "current",
        "close-original",
        "close-point-pole",
        "close-aperture",
    ] {
        let path = format!("study/{name}.wav");
        scratch.success(&["inspect", &path]);
        let bytes = fs::read(scratch.0.join(path)).unwrap();
        let mut power = 0.0;
        for chunk in bytes[58..].as_chunks::<4>().0 {
            let value = f32::from_le_bytes(*chunk) as f64;
            assert!(value.is_finite() && value.abs() <= 10.0_f64.powf(-6.0 / 20.0));
            power += value * value;
        }
        observed_rms.push((power / (24.0 * 44100.0)).sqrt());
    }
    for rms in &observed_rms[1..] {
        assert!((rms / observed_rms[0] - 1.0).abs() < 1e-7);
    }
    let before = fs::read(scratch.0.join("study/report.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(
        before,
        fs::read(scratch.0.join("study/report.json")).unwrap()
    );
    for flags in [
        vec!["--ceiling-dbfs", "0"],
        vec!["--ceiling-dbfs", "NaN"],
        vec!["--gap-mm", "0"],
        vec!["--offset-mm", "inf"],
        vec!["--sample-rate", "44000"],
        vec!["--unknown", "1"],
        vec!["--measure-only", "--measure-only"],
        vec!["--gap-mm", "1", "--gap-mm", "2"],
        vec!["--gap-mm"],
    ] {
        let mut args = vec!["pickup-listening", "--output", "invalid"];
        args.extend(flags);
        assert!(!scratch.run(&args).status.success(), "{args:?}");
        assert!(!scratch.0.join("invalid").exists());
    }
}

#[test]
fn pickup_convergence_reports_finite_reference_and_frozen_path_without_overwrite() {
    let scratch = Scratch::new();
    let args = [
        "converge-pickup",
        "--output",
        "pickup-convergence.json",
        "--note",
        "55",
        "--seconds",
        "0.05",
        "--gap-mm",
        "0.5",
        "--offset-mm",
        "0.25",
    ];
    scratch.success(&args);
    let report = scratch.json("pickup-convergence.json");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["reference"]["internal_steps"], 128);
    assert_eq!(report["filter_delay_output_samples"], 15.75);
    assert_eq!(report["faults"], 0);
    let rows = report["comparisons"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    assert_eq!(rows[4]["internal_steps"], 64);
    assert_eq!(rows[5]["path"], "frozen_128x_trajectory_sampled_at_4x");
    assert!(rows[5]["mechanics"].is_null());
    for row in rows {
        assert_eq!(row["laws"].as_array().unwrap().len(), 2);
        for law in row["laws"].as_array().unwrap() {
            for window in ["full", "attack_32_ms", "after_attack"] {
                assert!(law[window]["raw_nrmse"].as_f64().unwrap().is_finite());
            }
        }
    }
    for (fine, coarse) in rows[4]["laws"]
        .as_array()
        .unwrap()
        .iter()
        .zip(rows[0]["laws"].as_array().unwrap())
    {
        assert!(
            fine["full"]["raw_nrmse"].as_f64().unwrap()
                < coarse["full"]["raw_nrmse"].as_f64().unwrap() * 0.2
        );
    }
    let before = fs::read(scratch.0.join("pickup-convergence.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(
        before,
        fs::read(scratch.0.join("pickup-convergence.json")).unwrap()
    );
    scratch.success(&[
        "converge-pickup",
        "--output",
        "fine.json",
        "--reference-steps",
        "256",
        "--note",
        "100",
        "--velocity",
        "0.2",
        "--seconds",
        "0.05",
    ]);
    let fine_report = scratch.json("fine.json");
    assert_eq!(fine_report["reference"]["internal_steps"], 256);
    assert_eq!(fine_report["frozen_reference_stride"], 64);
    let fine_rows = fine_report["comparisons"].as_array().unwrap();
    assert_eq!(fine_rows.len(), 7);
    assert_eq!(fine_rows[5]["internal_steps"], 128);
    for (fine, coarse) in fine_rows[5]["laws"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fine_rows[4]["laws"].as_array().unwrap())
    {
        assert!(
            fine["full"]["raw_nrmse"].as_f64().unwrap()
                < coarse["full"]["raw_nrmse"].as_f64().unwrap() * 0.4
        );
    }
    for flags in [
        vec!["--reference-steps", "64"],
        vec!["--reference-steps", "512"],
        vec!["--seconds", "0.049"],
        vec!["--seconds", "0.251"],
        vec!["--seconds", "NaN"],
        vec!["--velocity", "0.001"],
        vec!["--gap-mm", "0"],
        vec!["--offset-mm", "inf"],
        vec!["--note", "101"],
        vec!["--sample-rate", "44000"],
        vec!["--unknown", "1"],
        vec!["--note", "55", "--note", "57"],
        vec!["--note"],
    ] {
        let mut args = vec!["converge-pickup", "--output", "invalid.json"];
        args.extend(flags);
        assert!(!scratch.run(&args).status.success(), "{args:?}");
        assert!(!scratch.0.join("invalid.json").exists());
    }
}

#[test]
fn mechanical_pickup_pair_preserves_production_wav_and_all_existing_outputs() {
    let scratch = Scratch::new();
    let options = [
        "--note",
        "55",
        "--sample-rate",
        "44100",
        "--seconds",
        "0.8",
        "--hold",
        "0.7",
    ];
    let mut args = vec!["render-pickup-pair", "--output", "pair.wav"];
    args.extend(options);
    scratch.success(&args);
    let mut regular = vec!["render", "--output", "regular.wav"];
    regular.extend(options);
    scratch.success(&regular);
    assert_eq!(
        fs::read(scratch.0.join("pair.wav")).unwrap(),
        fs::read(scratch.0.join("regular.wav")).unwrap()
    );
    scratch.success(&["inspect", "pair-point-pole.wav"]);
    let report = scratch.json("pair-pickup-pair.json");
    assert_eq!(report["faults"], 0);
    assert_eq!(report["mechanics"]["internal_samples"], 141120);
    assert_eq!(
        report["tone_comparison"]["windows"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(
        report["point_pole_levels"]["rms"].as_f64().unwrap()
            > report["production_levels"]["rms"].as_f64().unwrap()
    );
    let before = fs::read(scratch.0.join("pair-pickup-pair.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(
        before,
        fs::read(scratch.0.join("pair-pickup-pair.json")).unwrap()
    );
    for existing in ["blocked-point-pole.wav", "blocked-pickup-pair.json"] {
        fs::write(scratch.0.join(existing), "preserve").unwrap();
        assert!(
            !scratch
                .run(&["render-pickup-pair", "--output", "blocked.wav"])
                .status
                .success()
        );
        assert!(!scratch.0.join("blocked.wav").exists());
        assert_eq!(fs::read(scratch.0.join(existing)).unwrap(), b"preserve");
        fs::remove_file(scratch.0.join(existing)).unwrap();
    }
    for flags in [
        vec!["--trace"],
        vec!["--seconds", "10.1"],
        vec!["--seconds", "0.64", "--hold", "0.6"],
        vec!["--hold", "0.59"],
        vec!["--velocity", "NaN"],
        vec!["--gap-mm", "0"],
        vec!["--offset-mm", "4"],
        vec!["--note", "27"],
        vec!["--velocity", "0.5", "--velocity", "0.6"],
    ] {
        let mut args = vec!["render-pickup-pair", "--output", "invalid.wav"];
        args.extend(flags);
        assert!(!scratch.run(&args).status.success(), "{args:?}");
        assert!(!scratch.0.join("invalid.wav").exists());
        assert!(!scratch.0.join("invalid-point-pole.wav").exists());
        assert!(!scratch.0.join("invalid-pickup-pair.json").exists());
    }
}

#[test]
fn pickup_transfer_reports_both_laws_and_rejects_invalid_or_existing_output() {
    let scratch = Scratch::new();
    let args = [
        "pickup-transfer",
        "--output",
        "transfer.json",
        "--period-frames",
        "32",
        "--amplitudes-mm",
        "0.25",
    ];
    scratch.success(&args);
    let report = scratch.json("transfer.json");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["frequency_hz"], 1378.125);
    assert_eq!(report["last_retained_harmonic"], 13);
    let rows = report["observations"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["law"], "production");
    assert_eq!(rows[1]["law"], "research_point_pole");
    assert_eq!(rows[0]["sampling_errors"].as_array().unwrap().len(), 5);
    assert_eq!(rows[1]["reference_harmonics"].as_array().unwrap().len(), 12);
    let before = fs::read(scratch.0.join("transfer.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(before, fs::read(scratch.0.join("transfer.json")).unwrap());
    for flags in [
        vec!["--period-frames", "15"],
        vec!["--period-frames", "513"],
        vec!["--sample-rate", "44000"],
        vec!["--gap-mm", "NaN"],
        vec!["--offset-mm", "inf"],
        vec!["--amplitudes-mm", "0"],
        vec!["--amplitudes-mm", "0.1,0.1"],
        vec!["--amplitudes-mm", "0.1,0.2,0.3,0.4,0.5,0.6"],
        vec!["--amplitudes-mm", "NaN"],
        vec!["--gap-mm", "1", "--gap-mm", "2"],
        vec!["--unknown", "1"],
        vec!["--period-frames"],
    ] {
        let mut args = vec!["pickup-transfer", "--output", "invalid.json"];
        args.extend(flags);
        assert!(!scratch.run(&args).status.success(), "{args:?}");
        assert!(!scratch.0.join("invalid.json").exists());
    }
}

#[test]
fn tone_comparison_roundtrip_needs_no_sustain_claim_and_preserves_output() {
    let scratch = Scratch::new();
    scratch.success(&[
        "render",
        "--output",
        "tone.wav",
        "--seconds",
        "0.8",
        "--hold",
        "0.7",
    ]);
    let args = [
        "compare-tone",
        "tone.wav",
        "tone.wav",
        "--note",
        "57",
        "--output",
        "tone-comparison.json",
    ];
    scratch.success(&args);
    let report = scratch.json("tone-comparison.json");
    let tone = &report["tone_comparison"];
    assert_eq!(tone["schema_version"], 1);
    assert_eq!(tone["windows"].as_array().unwrap().len(), 3);
    assert!(tone.get("decay").is_none());
    assert_eq!(
        tone["windows"][2]["harmonics"][0]["candidate_minus_reference_balance_db"],
        0.0
    );
    let before = fs::read(scratch.0.join("tone-comparison.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(
        before,
        fs::read(scratch.0.join("tone-comparison.json")).unwrap()
    );
    for flags in [
        vec![],
        vec!["--note", "128"],
        vec!["--note", "57", "--candidate-start", "0.3"],
        vec!["--note", "57", "--unknown", "1"],
        vec!["--note", "57", "--note", "55"],
    ] {
        let mut invalid = vec![
            "compare-tone",
            "tone.wav",
            "tone.wav",
            "--output",
            "bad.json",
        ];
        invalid.extend(flags);
        assert!(!scratch.run(&invalid).status.success());
        assert!(!scratch.0.join("bad.json").exists());
    }
}

#[test]
fn pickup_set_freezes_fit_geometry_and_gain_before_validation() {
    let scratch = Scratch::new();
    for (file, velocity, gap) in [
        ("soft.wav", "0.3", "2"),
        ("loud.wav", "0.7", "2"),
        ("held.wav", "0.5", "2"),
        ("different.wav", "0.5", "1"),
    ] {
        scratch.success(&[
            "render",
            "--output",
            file,
            "--note",
            "57",
            "--velocity",
            velocity,
            "--gap-mm",
            gap,
            "--offset-mm",
            "0.75",
            "--seconds",
            "0.7",
            "--hold",
            "0.6",
        ]);
    }
    let take = |id: &str, file: &str, role: &str, velocity: f64| {
        serde_json::json!({
            "id":id,"file":file,"role":role,"note":57,"velocity":velocity,
            "velocity_basis":"Known synthetic model input", "reference_start_seconds":0.1,
            "model_start_seconds":0.1,"seconds":0.4,"sustain_end_seconds":0.6,
        })
    };
    let mut manifest = serde_json::json!({
        "schema_version":1,"source":"Local synthetic fixture","source_revision":"Model 0.1.1",
        "license":"Project-generated test signal","instrument":"RF-Tines research model",
        "processing":"None","capture_gain":"Fixed engine output gain",
        "gaps_mm":[1.5,2],"offsets_mm":[0.5,0.75],
        "takes":[take("soft", "soft.wav", "fit", 0.3), take("loud", "loud.wav", "fit", 0.7),
            take("held", "held.wav", "validation", 0.5)]
    });
    fs::write(
        scratch.0.join("set.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    scratch.success(&["fit-pickup-set", "set.json", "--output", "fit.json"]);
    let report = scratch.json("fit.json");
    assert_eq!(report["best_candidate_index"], 3);
    assert_eq!(report["validation_objective_db"], 0.0);
    assert_eq!(report["candidates"][3]["fit"]["applied_gain"], 1.0);
    assert_eq!(report["validation"].as_array().unwrap().len(), 1);
    let before = fs::read(scratch.0.join("fit.json")).unwrap();
    assert!(
        !scratch
            .run(&["fit-pickup-set", "set.json", "--output", "fit.json"])
            .status
            .success()
    );
    assert_eq!(before, fs::read(scratch.0.join("fit.json")).unwrap());

    // Deliberately change only the held-out geometry. It must not affect fitting.
    manifest["takes"][2]["file"] = "different.wav".into();
    fs::write(
        scratch.0.join("set.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    scratch.success(&[
        "fit-pickup-set",
        "set.json",
        "--output",
        "validation-changed.json",
    ]);
    let changed = scratch.json("validation-changed.json");
    assert_eq!(changed["candidates"], report["candidates"]);
    assert_eq!(changed["ranking_indices"], report["ranking_indices"]);
    assert!(changed["validation_objective_db"].as_f64().unwrap() > 0.05);
    assert_eq!(
        changed["validation"][0]["metrics"]["applied_candidate_gain"],
        1.0
    );
    assert!(
        changed["validation"][0]["metrics"]["applied_gain_normalized_rmse"]
            .as_f64()
            .unwrap()
            > 0.1
    );

    for invalid in [
        {
            let mut m = manifest.clone();
            m["takes"][2]["velocity"] = 0.3.into();
            m
        },
        {
            let mut m = manifest.clone();
            m["takes"][2]["file"] = "soft.wav".into();
            m
        },
        {
            let mut m = manifest.clone();
            m["takes"][0]["sustain_end_seconds"] = 0.2.into();
            m
        },
        {
            let mut m = manifest.clone();
            m["processing"] = "".into();
            m
        },
        {
            let mut m = manifest.clone();
            m["takes"][1]["role"] = "validation".into();
            m
        },
        {
            let mut m = manifest.clone();
            m["schema_version"] = 2.into();
            m
        },
        {
            let mut m = manifest.clone();
            m["gaps_mm"] = serde_json::json!([1, 1]);
            m
        },
        {
            let mut m = manifest.clone();
            m["unknown_option"] = true.into();
            m
        },
    ] {
        fs::write(
            scratch.0.join("invalid.json"),
            serde_json::to_vec(&invalid).unwrap(),
        )
        .unwrap();
        assert!(
            !scratch
                .run(&["fit-pickup-set", "invalid.json", "--output", "bad.json"])
                .status
                .success()
        );
        assert!(!scratch.0.join("bad.json").exists());
    }
}

#[test]
fn pickup_sweep_recovers_known_geometry_and_preserves_files() {
    let scratch = Scratch::new();
    scratch.success(&[
        "render",
        "--output",
        "reference.wav",
        "--seconds",
        "0.8",
        "--hold",
        "0.7",
        "--note",
        "57",
        "--velocity",
        "0.7",
        "--gap-mm",
        "2",
        "--offset-mm",
        "0.75",
    ]);
    let reference_before = fs::read(scratch.0.join("reference.wav")).unwrap();
    let args = [
        "sweep-pickup",
        "reference.wav",
        "--output",
        "sweep.json",
        "--note",
        "57",
        "--velocity",
        "0.7",
        "--seconds",
        "0.5",
        "--reference-start",
        "0.1",
        "--model-start",
        "0.1",
        "--gaps-mm",
        "1.5,2",
        "--offsets-mm",
        "0.5,0.75",
    ];
    scratch.success(&args);
    let report = scratch.json("sweep.json");
    assert_eq!(report["best_candidate_index"], 3);
    assert_eq!(report["ranking_indices"].as_array().unwrap().len(), 4);
    assert_eq!(report["reference_start_frame"], 4800);
    assert_eq!(report["model_start_frame"], 4800);
    assert_eq!(report["compared_frames"], 24000);
    let metrics = &report["candidates"][3]["metrics"];
    assert_eq!(metrics["objective_db"], 0.0);
    assert_eq!(metrics["raw_normalized_rmse"], 0.0);
    assert_eq!(metrics["scored_windows"], 7);
    assert_eq!(metrics["windows"][0]["requested_samples"], 6144);
    assert!(
        metrics["windows"][0]["spectral_observation_samples"]
            .as_u64()
            .unwrap()
            > 6000
    );
    let report_before = fs::read(scratch.0.join("sweep.json")).unwrap();
    assert!(!scratch.run(&args).status.success());
    assert_eq!(
        report_before,
        fs::read(scratch.0.join("sweep.json")).unwrap()
    );
    assert_eq!(
        reference_before,
        fs::read(scratch.0.join("reference.wav")).unwrap()
    );

    for (flag, value) in [
        ("--gaps-mm", "1,1.0"),
        ("--offsets-mm", "NaN"),
        ("--seconds", "0.01"),
        ("--note", "101"),
        ("--velocity", "0"),
        ("--reference-start", "0.4"),
        ("--model-start", "3"),
        ("--channel", "1"),
    ] {
        let mut invalid = args.to_vec();
        invalid[3] = "bad.json";
        if let Some(index) = invalid.iter().position(|v| *v == flag) {
            invalid[index + 1] = value;
        } else {
            invalid.extend([flag, value]);
        }
        assert!(!scratch.run(&invalid).status.success(), "{flag} {value}");
        assert!(!scratch.0.join("bad.json").exists());
    }
    let mut missing = args.to_vec();
    missing[3] = "bad.json";
    missing.drain(6..8); // The strike velocity must be supplied explicitly.
    assert!(!scratch.run(&missing).status.success());
    assert!(!scratch.0.join("bad.json").exists());
}

#[test]
fn partial_comparison_roundtrip_and_invalid_regions_preserve_files() {
    let scratch = Scratch::new();
    scratch.success(&[
        "render",
        "--output",
        "a.wav",
        "--seconds",
        "1.2",
        "--hold",
        "1.1",
    ]);
    scratch.success(&[
        "compare-partials",
        "a.wav",
        "a.wav",
        "--output",
        "partials.json",
        "--seconds",
        "1",
    ]);
    let report = scratch.json("partials.json");
    let comparison = &report["partial_comparison"];
    assert_eq!(comparison["schema_version"], 1);
    assert_eq!(comparison["candidate_level_minus_reference_db"], 0.0);
    assert_eq!(comparison["reference_region"]["end_frame_exclusive"], 48000);
    assert!(!comparison["matches"].as_array().unwrap().is_empty());
    assert!(
        comparison["matches"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["rms_frequency_error_cents"] == 0.0 && p["rms_level_error_db"] == 0.0)
    );
    let before = fs::read(scratch.0.join("partials.json")).unwrap();
    assert!(
        !scratch
            .run(&[
                "compare-partials",
                "a.wav",
                "a.wav",
                "--output",
                "partials.json",
                "--seconds",
                "1"
            ])
            .status
            .success()
    );
    assert_eq!(before, fs::read(scratch.0.join("partials.json")).unwrap());
    for flags in [
        vec![],
        vec!["--seconds", "NaN"],
        vec!["--seconds", "2"],
        vec!["--seconds", "1", "--candidate-start", "0.3"],
        vec!["--seconds", "1", "--match-cents", "200"],
        vec!["--seconds", "1", "--seconds", "1"],
        vec!["--seconds", "1", "--unknown", "1"],
    ] {
        let mut args = vec!["compare-partials", "a.wav", "a.wav", "--output", "bad.json"];
        args.extend(flags);
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
    }
}

#[test]
fn convergence_reports_reference_and_refinement_without_overwriting() {
    let scratch = Scratch::new();
    scratch.success(&[
        "converge",
        "--output",
        "convergence.json",
        "--seconds",
        "0.05",
    ]);
    let report = scratch.json("convergence.json");
    assert_eq!(report["reference"]["substeps"], 64);
    assert_eq!(report["comparisons"].as_array().unwrap().len(), 4);
    assert_eq!(report["frames"], 2400);
    for row in report["comparisons"].as_array().unwrap() {
        assert_eq!(row["full_audio"]["candidate_delay_samples"], 0);
        assert_eq!(row["full_audio"]["compared_frames"], 2400);
        assert!(row["mechanics"]["separation_seconds"].as_f64().unwrap() > 0.0);
        assert!(
            row["displacement_normalized_rmse"]
                .as_f64()
                .unwrap()
                .is_finite()
        );
    }
    assert!(
        report["comparisons"][3]["displacement_normalized_rmse"]
            .as_f64()
            .unwrap()
            < report["comparisons"][0]["displacement_normalized_rmse"]
                .as_f64()
                .unwrap()
    );
    let before = fs::read(scratch.0.join("convergence.json")).unwrap();
    assert!(
        !scratch
            .run(&["converge", "--output", "convergence.json"])
            .status
            .success()
    );
    assert_eq!(
        before,
        fs::read(scratch.0.join("convergence.json")).unwrap()
    );
    for args in [
        vec!["converge", "--output", "bad.json", "--velocity", "NaN"],
        vec!["converge", "--output", "bad.json", "--velocity", "1e-100"],
        vec!["converge", "--output", "bad.json", "--seconds", "10"],
        vec![
            "converge", "--output", "bad.json", "--note", "57", "--note", "58",
        ],
        vec!["converge", "--output", "bad.json", "--sample-rate", "8000"],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
    }
}

#[test]
fn rendered_audio_can_be_analyzed_and_compared_without_overwriting() {
    let scratch = Scratch::new();
    scratch.success(&[
        "render",
        "--output",
        "a.wav",
        "--seconds",
        "0.4",
        "--hold",
        "0.3",
    ]);
    scratch.success(&[
        "analyze",
        "a.wav",
        "--output",
        "analysis.json",
        "--note",
        "57",
        "--sustain-end",
        "0.29",
    ]);
    let report = scratch.json("analysis.json");
    assert_eq!(report["analysis"]["schema_version"], 2);
    assert!(
        !report["analysis"]["inharmonic_tracking"]["tracks"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        report["analysis"]["inharmonic_tracking"]["minimum_separation_hz"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert!(
        report["analysis"]["fundamental"]["frequency_hz"]
            .as_f64()
            .unwrap()
            > 200.0
    );
    scratch.success(&["compare", "a.wav", "a.wav", "--output", "comparison.json"]);
    scratch.success(&[
        "analyze",
        "a.wav",
        "--output",
        "short-window.json",
        "--partial-window-ms",
        "32",
    ]);
    let short_window = scratch.json("short-window.json");
    assert_eq!(
        short_window["analysis"]["inharmonic_tracking"]["observed_samples"],
        1536
    );
    assert_eq!(
        short_window["analysis"]["inharmonic_tracking"]["window_seconds"],
        0.032
    );
    assert!(
        short_window["analysis"]["inharmonic_tracking"]["frames"]
            .as_array()
            .unwrap()
            .len()
            > report["analysis"]["inharmonic_tracking"]["frames"]
                .as_array()
                .unwrap()
                .len()
    );
    let report = scratch.json("comparison.json");
    assert_eq!(report["comparison"]["candidate_delay_samples"], 0);
    assert_eq!(report["comparison"]["raw_normalized_rmse"], 0.0);
    let before = fs::read(scratch.0.join("analysis.json")).unwrap();
    assert!(
        !scratch
            .run(&["analyze", "a.wav", "--output", "analysis.json"])
            .status
            .success()
    );
    assert_eq!(before, fs::read(scratch.0.join("analysis.json")).unwrap());
    for args in [
        vec![
            "analyze", "a.wav", "--output", "bad.json", "--note", "57", "--note", "58",
        ],
        vec![
            "analyze",
            "a.wav",
            "--output",
            "bad.json",
            "--sustain-end",
            "NaN",
        ],
        vec!["analyze", "a.wav", "--output", "bad.json", "--unknown", "1"],
        vec!["analyze", "missing.wav", "--output", "bad.json"],
        vec![
            "analyze",
            "a.wav",
            "--output",
            "bad.json",
            "--partial-window-ms",
            "0",
        ],
        vec![
            "analyze",
            "a.wav",
            "--output",
            "bad.json",
            "--partial-window-ms",
            "NaN",
        ],
        vec![
            "analyze",
            "a.wav",
            "--output",
            "bad.json",
            "--partial-window-ms",
            "2048",
        ],
        vec![
            "analyze",
            "a.wav",
            "--output",
            "bad.json",
            "--partial-window-ms",
            "32",
            "--partial-window-ms",
            "128",
        ],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
    }
}

#[test]
fn comparison_rate_mismatch_fails_without_creating_a_report() {
    let scratch = Scratch::new();
    scratch.success(&[
        "render",
        "--output",
        "a.wav",
        "--seconds",
        "0.1",
        "--hold",
        "0.05",
    ]);
    scratch.success(&[
        "render",
        "--output",
        "b.wav",
        "--seconds",
        "0.1",
        "--hold",
        "0.05",
        "--sample-rate",
        "44100",
    ]);
    let result = scratch.run(&["compare", "a.wav", "b.wav", "--output", "bad.json"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("equal sample rates"));
    assert!(!scratch.0.join("bad.json").exists());
}

#[test]
fn spring_tuning_preflights_every_output_and_rejects_invalid_reference() {
    let scratch = Scratch::new();
    for file in ["pair.wav", "pair-before.wav", "pair.json"] {
        fs::write(scratch.0.join(file), b"preserve").unwrap();
        let out = scratch.run(&["tune-modal-pitch", "missing.json", "--output", "pair.wav"]);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("refusing to overwrite"));
        assert_eq!(fs::read(scratch.0.join(file)).unwrap(), b"preserve");
        fs::remove_file(scratch.0.join(file)).unwrap();
    }
    fs::write(scratch.0.join("invalid.json"), b"{}").unwrap();
    let out = scratch.run(&["tune-modal-pitch", "invalid.json", "--output", "pair.wav"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("qualified G3"));
    assert!(!scratch.0.join("pair.wav").exists());
    assert!(!scratch.0.join("pair-before.wav").exists());
    assert!(!scratch.0.join("pair.json").exists());
}

#[test]
fn pickup_mixing_rejects_invalid_arguments_and_preserves_existing_output() {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("existing.json"), b"preserve").unwrap();
    let result = scratch.run(&["pickup-mixing", "--output", "existing.json"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("new .json file"));
    assert_eq!(
        fs::read(scratch.0.join("existing.json")).unwrap(),
        b"preserve"
    );
    for args in [
        vec!["pickup-mixing"],
        vec!["pickup-mixing", "--unknown", "bad.json"],
        vec!["pickup-mixing", "--output", "bad.json", "--unknown"],
        vec!["pickup-mixing", "--output", "bad.wav"],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
}

#[test]
fn pickup_decay_reports_controls_and_preserves_existing_output() {
    let scratch = Scratch::new();
    scratch.success(&["pickup-decay", "--output", "decay.json"]);
    let report = scratch.json("decay.json");
    assert_eq!(report["all_cases_qualified"], true);
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    for case in cases {
        assert_eq!(case["qualified"], true);
        assert_eq!(case["observations"].as_array().unwrap().len(), 6);
        assert_eq!(case["decay_fits"].as_array().unwrap().len(), 2);
        assert!(case["max_refinement_relative_error"].as_f64().unwrap() < 1e-8);
    }
    let before = fs::read(scratch.0.join("decay.json")).unwrap();
    assert!(
        !scratch
            .run(&["pickup-decay", "--output", "decay.json"])
            .status
            .success()
    );
    assert_eq!(before, fs::read(scratch.0.join("decay.json")).unwrap());
    for args in [
        vec!["pickup-decay"],
        vec!["pickup-decay", "--unknown", "bad.json"],
        vec!["pickup-decay", "--output", "bad.json", "--unknown"],
        vec!["pickup-decay", "--output", "bad.wav"],
    ] {
        assert!(!scratch.run(&args).status.success());
        assert!(!scratch.0.join("bad.json").exists());
        assert!(!scratch.0.join("bad.wav").exists());
    }
}
