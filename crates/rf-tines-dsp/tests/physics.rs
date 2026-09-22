use rf_tines_dsp::{Engine, FIRST_NOTE, LAST_NOTE, OVERSAMPLE, PickupLaw, Profile, Voice};

#[test]
fn contact_is_passive_and_separates_across_registers_and_rates() {
    for rate in [44_100.0, 48_000.0, 96_000.0, 192_000.0] {
        for note in [FIRST_NOTE, 45, 57, 69, LAST_NOTE] {
            for velocity in [0.01, 0.2, 0.7, 1.0] {
                let mut voice = Voice::new(rate, note, Profile::default()).unwrap();
                voice.strike(velocity);
                let mut previous = voice.probe().mechanical_energy_j;
                let mut peak = 0.0_f64;
                for _ in 0..(rate * OVERSAMPLE as f64 * 0.04) as usize {
                    let value = voice.tick();
                    let probe = voice.probe();
                    assert!(value.is_finite());
                    assert!(probe.contact_force_n >= 0.0);
                    assert!(
                        probe.mechanical_energy_j <= previous * (1.0 + 1e-8) + 1e-15,
                        "energy increased at {rate}/{note}/{velocity}: {} > {previous}",
                        probe.mechanical_energy_j
                    );
                    previous = probe.mechanical_energy_j;
                    peak = peak.max(value.abs());
                }
                assert!(!voice.probe().contact_active, "contact did not separate");
                assert!(peak > 1e-9, "silent strike");
            }
        }
    }
}

#[test]
fn dampers_remove_energy_and_retrigger_preserves_motion() {
    let mut voice = Voice::new(48_000.0, 57, Profile::default()).unwrap();
    voice.strike(0.8);
    for _ in 0..10_000 {
        voice.tick();
    }
    let before = voice.probe();
    voice.strike(0.6);
    assert_eq!(voice.probe().displacement_m, before.displacement_m);
    assert_eq!(voice.probe().velocity_m_s, before.velocity_m_s);
    for _ in 0..10_000 {
        voice.tick();
    }
    voice.set_damped(true);
    for _ in 0..100_000 {
        voice.tick();
    }
    assert!(voice.probe().mechanical_energy_j < 1e-18);
}

#[test]
fn invalid_inputs_are_rejected_without_poisoning_audio() {
    for rate in [f64::NAN, f64::INFINITY, 0.0, 8000.0] {
        assert!(Engine::new(rate, Profile::default()).is_err());
    }
    assert!(
        Engine::new(
            48_000.0,
            Profile {
                pickup_gap_m: 0.0,
                ..Profile::default()
            }
        )
        .is_err()
    );
    let mut engine = Engine::new(48_000.0, Profile::default()).unwrap();
    assert!(!engine.note_on(16, 57, 1.0));
    assert!(!engine.note_on(0, 0, 1.0));
    assert!(!engine.note_on(0, 57, f64::NAN));
    assert!(!engine.set_gain(f64::INFINITY));
    assert!(!engine.control_change(0, 64, f64::NAN));
    for _ in 0..1000 {
        assert_eq!(engine.next_sample(), 0.0);
    }
}

#[test]
fn pedal_and_all_notes_off_are_channel_aware() {
    let mut engine = Engine::new(48_000.0, Profile::default()).unwrap();
    engine.note_on(0, 57, 0.8);
    engine.control_change(0, 64, 1.0);
    engine.control_change(0, 123, 0.0);
    for _ in 0..24_000 {
        engine.next_sample();
    }
    assert!(engine.probe(57).unwrap().mechanical_energy_j > 1e-7);
    engine.control_change(1, 64, 0.0);
    for _ in 0..100 {
        engine.next_sample();
    }
    assert!(engine.probe(57).unwrap().mechanical_energy_j > 1e-7);
    engine.control_change(0, 64, 0.0);
    for _ in 0..24_000 {
        engine.next_sample();
    }
    assert!(engine.probe(57).unwrap().mechanical_energy_j < 1e-18);
}

#[test]
fn silence_reset_and_render_are_deterministic() {
    let mut a = Engine::new(48_000.0, Profile::default()).unwrap();
    let mut b = Engine::new(48_000.0, Profile::default()).unwrap();
    a.note_on(2, 57, 0.7);
    b.note_on(2, 57, 0.7);
    for _ in 0..4096 {
        assert_eq!(a.next_sample(), b.next_sample());
    }
    a.reset();
    for _ in 0..100 {
        assert_eq!(a.next_sample(), 0.0);
    }
    assert_eq!(a.faults(), 0);
}

#[test]
fn all_sound_off_stops_released_tails() {
    let mut engine = Engine::new(48_000.0, Profile::default()).unwrap();
    engine.note_on(3, 57, 1.0);
    for _ in 0..1000 {
        engine.next_sample();
    }
    engine.note_off(3, 57);
    engine.control_change(3, 120, 0.0);
    for _ in 0..64 {
        engine.next_sample();
    }
    assert_eq!(engine.next_sample(), 0.0);
}

#[test]
fn full_keyboard_extreme_profile_stays_finite() {
    let profile = Profile {
        contact_stiffness: 1e12,
        maximum_hammer_speed_m_s: 3.0,
        pickup_gap_m: 0.0005,
        modal_mass_kg: 0.0001,
        ..Profile::default()
    };
    let mut engine = Engine::new(44_100.0, profile).unwrap();
    for note in FIRST_NOTE..=LAST_NOTE {
        engine.note_on(0, note, 1.0);
    }
    for _ in 0..4096 {
        assert!(engine.next_sample().is_finite());
    }
    assert_eq!(engine.faults(), 0);
}

#[test]
fn late_pedal_recaptures_a_released_tail() {
    let mut engine = Engine::new(48_000.0, Profile::default()).unwrap();
    engine.note_on(0, 57, 0.8);
    for _ in 0..5000 {
        engine.next_sample();
    }
    engine.note_off(0, 57);
    for _ in 0..200 {
        engine.next_sample();
    }
    engine.control_change(0, 64, 1.0);
    for _ in 0..24_000 {
        engine.next_sample();
    }
    assert!(engine.probe(57).unwrap().mechanical_energy_j > 1e-7);
}

#[test]
fn one_channels_panic_does_not_kill_another_channels_held_key() {
    let mut engine = Engine::new(48_000.0, Profile::default()).unwrap();
    engine.note_on(0, 57, 0.7);
    engine.note_on(1, 57, 0.7);
    for _ in 0..5000 {
        engine.next_sample();
    }
    engine.control_change(0, 120, 0.0);
    assert!(engine.probe(57).unwrap().mechanical_energy_j > 1e-7);
    engine.control_change(1, 120, 0.0);
    assert_eq!(engine.probe(57).unwrap().mechanical_energy_j, 0.0);
}

/// Pitch by a harmonic scan rather than by counting zero crossings.
///
/// Counting crossings measures whatever partial happens to cross, so a voice
/// carrying a strong inharmonic partial reads sharp even when its pitch is
/// exact. That already caught this project out once; the fix is the method,
/// not the tolerance.
fn scanned_pitch(samples: &[f64], rate: f64, expected: f64) -> f64 {
    let magnitude = |frequency: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        let n = samples.len();
        for (i, value) in samples.iter().enumerate() {
            let w = 0.5 - 0.5 * (core::f64::consts::TAU * i as f64 / n as f64).cos();
            let phase = core::f64::consts::TAU * frequency * i as f64 / rate;
            re += value * w * phase.cos();
            im += value * w * phase.sin();
        }
        (re * re + im * im).sqrt() / n as f64
    };
    let mut best = (0.0, expected);
    for step in -300..=300 {
        let f = expected * 2.0_f64.powf(f64::from(step) / 12_000.0);
        let score: f64 = (1..=4)
            .map(|h| magnitude(f * f64::from(h)) / f64::from(h))
            .sum();
        if score > best.0 {
            best = (score, f);
        }
    }
    best.1
}

#[test]
fn sustained_fundamental_tracks_target_pitch_at_all_output_rates() {
    for rate in [44_100.0, 48_000.0, 96_000.0] {
        for note in [40, 57, 81, 100] {
            let mut voice = Voice::new(rate, note, Profile::default()).unwrap();
            voice.strike(0.5);
            let internal_rate = rate * OVERSAMPLE as f64;
            for _ in 0..(internal_rate * 0.12) as usize {
                voice.tick();
            }
            let mut samples = Vec::new();
            for _ in 0..(internal_rate * 0.25) as usize {
                voice.tick();
                samples.push(voice.probe().displacement_m);
            }
            let expected = 440.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0);
            let frequency = scanned_pitch(&samples, internal_rate, expected);
            let cents = 1200.0 * (frequency / expected).log2();
            assert!(cents.abs() < 0.1, "{rate}/{note}: {cents} cents");
        }
    }
}

#[test]
fn pitch_ratio_retunes_a_ringing_voice_without_resetting_it() {
    let rate = 48_000.0;
    let mut voice = Voice::new(rate, 57, Profile::default()).unwrap();
    voice.strike(0.5);
    let internal_rate = rate * OVERSAMPLE as f64;
    for _ in 0..(internal_rate * 0.12) as usize {
        voice.tick();
    }
    let before = voice.probe();
    let ratio = 2.0_f64.powf(2.0 / 12.0);
    assert!(voice.set_pitch_ratio(ratio));
    let after = voice.probe();
    assert_eq!(after.displacement_m, before.displacement_m);
    assert_eq!(after.velocity_m_s, before.velocity_m_s);
    let mut samples = Vec::new();
    for _ in 0..(internal_rate * 0.25) as usize {
        voice.tick();
        samples.push(voice.probe().displacement_m);
    }
    let expected = 440.0 * 2.0_f64.powf((57.0 - 69.0) / 12.0) * ratio;
    let frequency = scanned_pitch(&samples, internal_rate, expected);
    let cents = 1200.0 * (frequency / expected).log2();
    assert!(cents.abs() < 0.1, "pitch bend: {cents} cents");
    assert!(!voice.set_pitch_ratio(f64::NAN));
    assert!(!voice.set_pitch_ratio(3.0));
}

#[test]
fn calibrated_sustain_profile_validates_and_rings_longer_on_both_partials() {
    let calibrated = Profile::calibrated_sustain();
    calibrated.validate(48_000.0).unwrap();
    assert_eq!(calibrated.decay_seconds, 20.0);
    assert_eq!(calibrated.bar_partial_decay_seconds, 2.3);
    let default = Profile::default();
    assert_eq!(
        calibrated.third_partial_decay_seconds,
        default.third_partial_decay_seconds
    );
    assert_eq!(calibrated.pickup_gap_m, default.pickup_gap_m);
    for bad in [
        Profile {
            decay_seconds: 80.5,
            ..default
        },
        Profile {
            bar_partial_decay_seconds: 0.0,
            ..default
        },
        Profile {
            third_partial_decay_seconds: f64::NAN,
            ..default
        },
    ] {
        assert!(bad.validate(48_000.0).is_err());
    }
    // Energy after one second of free ringing follows the first-partial T60.
    let energy_after = |profile: Profile, seconds: f64| {
        let mut voice = Voice::new(48_000.0, 55, profile).unwrap();
        voice.strike(0.7);
        let mut energy = 0.0;
        for _ in 0..(48_000.0 * OVERSAMPLE as f64 * seconds) as usize {
            voice.tick();
            energy = voice.probe().mechanical_energy_j;
        }
        energy
    };
    let long = energy_after(calibrated, 1.0);
    let short = energy_after(default, 1.0);
    assert!(long > 2.0 * short, "{long} vs {short}");
    // Only the bar partial changed: its energy at 300 ms is far larger while the
    // first partial alone would differ by under 4%.
    let bar_only = Profile {
        bar_partial_decay_seconds: 2.3,
        ..default
    };
    let first_only = Profile {
        decay_seconds: 20.0,
        ..default
    };
    let bar = energy_after(bar_only, 0.3);
    let first = energy_after(first_only, 0.3);
    let base = energy_after(default, 0.3);
    assert!(bar > base && first > base, "{bar} {first} {base}");
    assert!(energy_after(calibrated, 0.3) > first);
    // The laboratory engine accepts the calibrated profile but not another geometry.
    assert!(Engine::new_laboratory_with(48_000.0, calibrated).is_ok());
    assert!(
        Engine::new_laboratory_with(
            48_000.0,
            Profile {
                pickup_gap_m: 0.001,
                ..default
            }
        )
        .is_err()
    );
}

#[test]
fn bar_partial_ratio_and_strike_weight_are_validated_profile_fields() {
    let calibrated = Profile::calibrated();
    calibrated.validate(48_000.0).unwrap();
    assert_eq!(calibrated.bar_partial_strike_weight, -0.02);
    assert_eq!(calibrated.bar_partial_ratio, 6.267);
    assert_eq!(
        calibrated.decay_seconds,
        Profile::calibrated_sustain().decay_seconds
    );
    let default = Profile::default();
    assert_eq!(default.bar_partial_ratio, 6.267);
    assert_eq!(default.bar_partial_strike_weight, -0.3);
    for bad in [
        Profile {
            bar_partial_ratio: 1.9,
            ..default
        },
        Profile {
            bar_partial_ratio: 12.1,
            ..default
        },
        Profile {
            bar_partial_strike_weight: -1.5,
            ..default
        },
        Profile {
            bar_partial_strike_weight: f64::NAN,
            ..default
        },
    ] {
        assert!(bad.validate(48_000.0).is_err());
    }
    // Excite the second partial alone by damping the first and third quickly, then
    // count zero crossings of the tip over 0.5 s: the frequency follows the ratio.
    let rate = 192_000.0;
    let isolated = |ratio: f64, weight: f64| Profile {
        bar_partial_ratio: ratio,
        bar_partial_strike_weight: weight,
        decay_seconds: 0.25,
        third_partial_decay_seconds: 0.005,
        bar_partial_decay_seconds: 10.0,
        ..default
    };
    let crossings = |ratio: f64| {
        let mut voice = Voice::new(rate, 55, isolated(ratio, -0.3)).unwrap();
        voice.strike(0.8);
        let settle = (rate * OVERSAMPLE as f64 * 1.5) as usize;
        let count_window = (rate * OVERSAMPLE as f64 * 0.5) as usize;
        let mut previous = 0.0;
        let mut count = 0u32;
        for i in 0..settle + count_window {
            voice.tick();
            let x = voice.probe().displacement_m;
            if i >= settle && (x > 0.0) != (previous > 0.0) {
                count += 1;
            }
            previous = x;
        }
        count
    };
    let fundamental = 440.0 * 2.0_f64.powf((55.0 - 69.0) / 12.0);
    for ratio in [6.0, 6.267] {
        let expected = 2.0 * ratio * fundamental * 0.5;
        let observed = f64::from(crossings(ratio));
        assert!(
            (observed - expected).abs() <= 0.02 * expected + 2.0,
            "ratio {ratio}: {observed} crossings, expected {expected}"
        );
    }
    // The second partial's remaining energy after the first has died scales with
    // the square of its strike weight.
    let residual = |weight: f64| {
        let mut voice = Voice::new(rate, 55, isolated(6.267, weight)).unwrap();
        voice.strike(0.8);
        for _ in 0..(rate * OVERSAMPLE as f64 * 1.5) as usize {
            voice.tick();
        }
        voice.probe().mechanical_energy_j
    };
    let strong = residual(-0.3);
    let weak = residual(-0.02);
    assert!(strong > 0.0 && weak > 0.0);
    let ratio = strong / weak;
    assert!((ratio / 225.0 - 1.0).abs() < 0.2, "energy ratio {ratio}");
}

#[test]
fn pickup_law_velocity_exponent_and_level_compensation_behave() {
    use rf_tines_dsp::{APERTURE_PICKUP, AxialAperture, PickupLaw, aperture_voltage};
    let default = Profile::default();
    // The default pickup compensates to exactly one, so retained renders stand.
    assert_eq!(default.level_compensation(), 1.0);
    assert_eq!(default.pickup_law, PickupLaw::Production);
    assert_eq!(default.velocity_exponent, 1.4);
    // The production law is even in the offset and falls with the gap.
    let with = |gap: f64, offset: f64, law: PickupLaw| Profile {
        pickup_gap_m: gap,
        pickup_offset_m: offset,
        pickup_law: law,
        ..default
    };
    let production =
        |gap: f64, offset: f64| with(gap, offset, PickupLaw::Production).pickup_sensitivity();
    assert_eq!(production(0.0015, 0.0005), production(0.0015, -0.0005));
    assert!(production(0.0005, 0.0005) > production(0.0015, 0.0005));
    assert!(production(0.0015, 0.0005) > production(0.003, 0.0005));
    // A centred production pickup still senses the reference swing through its
    // even harmonics, so the compensation stays finite.
    let centred = with(0.0015, 0.0, PickupLaw::Production).level_compensation();
    assert!(
        centred.is_finite() && centred > 1.0 && centred < 100.0,
        "{centred}"
    );
    // The aperture law at the Close Aperture geometry reproduces the laboratory
    // path's voltage from the voice's own tip motion, sample for sample.
    let aperture_profile = with(0.0005, 0.0005, PickupLaw::Aperture);
    assert_eq!(
        aperture_profile.pickup_pole_radius_m,
        APERTURE_PICKUP.pole_radius_m
    );
    let mut voice = Voice::new(48_000.0, 55, aperture_profile).unwrap();
    let mut shadow = Voice::new(48_000.0, 55, default).unwrap();
    let pickup = AxialAperture::new(APERTURE_PICKUP).unwrap();
    voice.strike(0.7);
    shadow.strike(0.7);
    for _ in 0..4096 {
        let signal = voice.tick();
        shadow.tick();
        let p = shadow.probe();
        assert_eq!(
            signal,
            aperture_voltage(&pickup, p.displacement_m, p.velocity_m_s)
        );
    }
    // The compensation of that geometry is the reference sine's RMS ratio.
    let compensation = aperture_profile.level_compensation();
    assert!((1.5..4.0).contains(&compensation), "{compensation}");
    // A larger velocity exponent launches a soft strike more slowly.
    let launch = |exponent: f64| {
        let mut voice = Voice::new(
            48_000.0,
            55,
            Profile {
                velocity_exponent: exponent,
                ..default
            },
        )
        .unwrap();
        voice.strike(0.3);
        voice.probe().mechanical_energy_j
    };
    assert!(launch(2.0) < launch(1.4) && launch(1.4) < launch(1.0));
    // Invalid fields are rejected; the laboratory engine keeps the production law.
    assert!(
        Profile {
            pickup_pole_radius_m: 0.0001,
            ..default
        }
        .validate(48_000.0)
        .is_err()
    );
    assert!(
        Profile {
            velocity_exponent: 0.1,
            ..default
        }
        .validate(48_000.0)
        .is_err()
    );
    assert!(
        Engine::new_laboratory_with(48_000.0, with(0.0015, 0.0005, PickupLaw::Aperture)).is_err()
    );
    // The engine smooths the compensation toward its target after a profile change.
    let mut engine = Engine::new(48_000.0, default).unwrap();
    assert_eq!(engine.level_compensation(), 1.0);
    let wide = with(0.003, 0.0005, PickupLaw::Production);
    let target = wide.level_compensation();
    assert!(target > 2.0, "{target}");
    // Raw engines never compensate, whatever the profile.
    assert!(engine.set_profile(wide));
    for _ in 0..100 {
        engine.next_sample();
    }
    assert_eq!(engine.level_compensation(), 1.0);
    engine.set_level_compensation(true);
    engine.next_sample();
    let first = engine.level_compensation();
    assert!(first > 1.0 && first < target);
    for _ in 0..4800 {
        engine.next_sample();
    }
    assert!((engine.level_compensation() - target).abs() < 1e-6 * target);
    engine.reset();
    assert_eq!(engine.level_compensation(), target);
    // The compensated wide-gap engine plays a medium note within 3 dB of the default.
    let rms = |profile: Profile| {
        let mut engine = Engine::new(48_000.0, profile).unwrap();
        engine.set_level_compensation(true);
        engine.reset();
        engine.note_on(0, 55, 0.6);
        let mut power = 0.0;
        for _ in 0..24_000 {
            power += f64::from(engine.next_sample()).powi(2);
        }
        (power / 24_000.0).sqrt()
    };
    let ratio = rms(wide) / rms(default);
    assert!((0.7..1.42).contains(&ratio), "{ratio}");
}

/// A voicing that predates the second coordinate must render through it
/// untouched. It does because the second axis is only ever driven by the
/// component of the strike that lies along it, and at a zero boundary angle
/// there is none: those three coordinates stay at exactly zero for the life of
/// the note, and the tip never leaves the axis the reduction assumes.
#[test]
fn a_tine_whose_axes_face_the_hammer_never_leaves_the_strike_direction() {
    for profile in [
        Profile::default(),
        Profile::calibrated(),
        Profile::calibrated_sustain(),
    ] {
        for note in [FIRST_NOTE, 55, LAST_NOTE] {
            let mut voice = Voice::new(48_000.0, note, profile).unwrap();
            voice.strike(0.9);
            for _ in 0..40_000 {
                voice.tick();
                assert_eq!(
                    voice.probe().transverse_displacement_m,
                    0.0,
                    "note {note} left the axis without being asked to"
                );
            }
        }
    }
}

/// Turning the tine's principal axes without splitting their frequencies is
/// not observable, and it should not be: a rotation of two degenerate
/// oscillators is a rotation of a circle. The strike divides between them by
/// cosine and sine, they answer in step, and recombining returns the original
/// motion exactly. This is what makes the boundary angle alone harmless and
/// the frequency split the thing that actually does the work.
#[test]
fn rotating_degenerate_axes_changes_nothing_at_all() {
    let straight = Profile::calibrated();
    let turned = Profile {
        tine_boundary_angle_rad: 0.3,
        tine_transverse_frequency_ratio: 1.0,
        ..straight
    };
    let mut a = Voice::new(48_000.0, 55, straight).unwrap();
    let mut b = Voice::new(48_000.0, 55, turned).unwrap();
    a.strike(0.8);
    b.strike(0.8);
    // Exact in arithmetic; in floating point the turned voice splits the drive
    // into two resonators and adds them back, so the two series separate only
    // by accumulated rounding. A wrong sign or a dropped projection would
    // separate them by a fraction of the signal, not by a billionth of it.
    let (mut moved, mut apart) = (0.0_f64, 0.0_f64);
    for _ in 0..20_000 {
        let (left, right) = (a.tick(), b.tick());
        moved = moved.max(left.abs());
        apart = apart.max((left - right).abs());
    }
    assert!(moved > 1e-6, "compared two silences");
    assert!(
        apart <= 1e-7 * moved,
        "{apart} apart on a signal of {moved}"
    );
    // The cancellation is exact in arithmetic and a rounding crumb in floating
    // point, so the transverse motion is judged against the swing the note
    // actually reaches rather than against whatever it happens to be at a zero
    // crossing. A projection that truly leaked would be a fraction of the
    // swing, not 1e-20 of it.
    let (swing, leak) = {
        let mut voice = Voice::new(48_000.0, 55, turned).unwrap();
        voice.strike(0.8);
        let (mut swing, mut leak) = (0.0_f64, 0.0_f64);
        for _ in 0..20_000 {
            voice.tick();
            let probe = voice.probe();
            swing = swing.max(probe.displacement_m.abs());
            leak = leak.max(probe.transverse_displacement_m.abs());
        }
        (swing, leak)
    };
    assert!(swing > 1e-6, "the turned voice never moved");
    assert!(
        leak <= 1e-12 * swing,
        "leaked {leak} against a swing of {swing}"
    );
}

/// With the axes split as well as turned, the tip stops travelling on a line.
/// Both principal directions are driven by the one blow, they drift out of
/// phase because they do not run at the same frequency, and the trajectory
/// opens into an ellipse whose orientation keeps turning. That is the motion
/// the high-speed measurements report and the motion a one-coordinate voice
/// cannot have.
#[test]
fn split_axes_open_the_tip_trajectory_into_an_ellipse() {
    let profile = Profile {
        tine_boundary_angle_rad: 0.3,
        tine_transverse_frequency_ratio: 1.02,
        ..Profile::calibrated()
    };
    let mut voice = Voice::new(48_000.0, 55, profile).unwrap();
    voice.strike(0.8);
    let (mut along, mut across, mut area) = (0.0_f64, 0.0_f64, 0.0_f64);
    let mut previous = (0.0, 0.0);
    for _ in 0..40_000 {
        voice.tick();
        let probe = voice.probe();
        let now = (probe.displacement_m, probe.transverse_displacement_m);
        along = along.max(now.0.abs());
        across = across.max(now.1.abs());
        // Twice the swept area: a straight line sweeps none however long it runs.
        area += (previous.0 * now.1 - previous.1 * now.0).abs();
        previous = now;
    }
    assert!(across > 0.0, "the second axis never moved");
    // The transverse swing is a real share of the driven one, not a rounding crumb.
    assert!(across > along * 0.01, "transverse {across} against {along}");
    assert!(area > 0.0, "the tip stayed on a line");
}

/// The two-dimensional flux is the same flux, not a second law. On the axis it
/// returns exactly what the ten-square-root reduction returns, and its
/// transverse component is zero there because the mirrored halves of the pole
/// ring cancel.
#[test]
fn the_planar_flux_agrees_with_the_axial_reduction_on_the_axis() {
    use rf_tines_dsp::{
        APERTURE_PICKUP, AxialAperture, PlanarAperture, aperture_voltage, planar_voltage,
    };
    let axial = AxialAperture::new(APERTURE_PICKUP).unwrap();
    let planar = PlanarAperture::new(APERTURE_PICKUP).unwrap();
    let places = [-0.003, -0.0005, 0.0, 0.0002, 0.001, 0.004];
    // At -0.5 mm the tine sits dead on the pole axis and both laws return zero,
    // so agreement is judged against the scale the law reaches elsewhere.
    let scale = places
        .iter()
        .map(|&x| aperture_voltage(&axial, x, 1.0).abs())
        .fold(0.0_f64, f64::max);
    assert!(scale > 0.0);
    for displacement in places {
        for speed in [-1.7, 0.0, 0.3] {
            let reduced = aperture_voltage(&axial, displacement, speed);
            let full = planar_voltage(&planar, [displacement, 0.0], [speed, 0.0]);
            assert!(
                (reduced - full).abs() <= 1e-12 * scale * speed.abs().max(1.0),
                "at {displacement} m, {speed} m/s: {reduced} against {full}"
            );
            let slope = planar.gradient_wb_per_m([displacement, 0.0]);
            assert!(
                slope[1].abs() <= 1e-12 * scale,
                "at {displacement} m the mirrored halves failed to cancel: {slope:?}"
            );
        }
    }
    // Off the axis the transverse slope is real, and it is what the axial
    // reduction has no way to report.
    let off = planar.gradient_wb_per_m([0.0, 0.0003]);
    assert!(off[1].abs() > 0.01 * off[0].abs().max(1e-30), "{off:?}");
    assert!(planar.gradient_wb_per_m([0.06, 0.0])[0].is_nan());
    assert!(planar.gradient_wb_per_m([0.0, f64::NAN])[0].is_nan());
}

/// The contact stays passive with both axes in the solve. The hammer pushes
/// along one direction and each axis takes the share its own orientation
/// gives it, so the same energy ledger has to hold over six coordinates.
#[test]
fn a_split_tine_still_takes_no_energy_from_the_hammer() {
    let profile = Profile {
        tine_boundary_angle_rad: 0.4,
        tine_transverse_frequency_ratio: 1.05,
        ..Profile::calibrated()
    };
    for note in [FIRST_NOTE, 55, LAST_NOTE] {
        for velocity in [0.05, 0.5, 1.0] {
            let mut voice = Voice::new(48_000.0, note, profile).unwrap();
            voice.strike(velocity);
            let mut previous = voice.probe().mechanical_energy_j;
            for _ in 0..(48_000.0 * OVERSAMPLE as f64 * 0.04) as usize {
                let value = voice.tick();
                assert!(value.is_finite());
                let now = voice.probe().mechanical_energy_j;
                assert!(
                    now <= previous * (1.0 + 1e-8) + 1e-15,
                    "energy grew at {note}/{velocity}: {now} > {previous}"
                );
                previous = now;
            }
            assert!(!voice.probe().contact_active, "contact did not separate");
        }
    }
}

/// Whether the tip passes above or below the pole has to be audible, and only
/// an ellipse can make it so.
///
/// The pole's node ring is mirror symmetric, so while the tip stays on the
/// axis the flux it sees is an even function of the transverse offset: moving
/// the pickup the same distance the other way gives back the identical
/// waveform, and the offset is inaudible. Once the trajectory opens, the tip
/// spends its swing on one side of the pole rather than the other and the two
/// settings stop agreeing. This is the whole reason the second coordinate is
/// worth carrying: a voice that computed the flux at the axis, or that ignored
/// where the tine rests across it, would pass everything else and fail here.
#[test]
fn which_side_of_the_pole_the_ellipse_leans_to_is_audible() {
    let render = |profile: Profile| {
        let mut voice = Voice::new(48_000.0, 55, profile).unwrap();
        voice.strike(0.8);
        (0..20_000).map(|_| voice.tick()).collect::<Vec<_>>()
    };
    let separation = |profile: Profile| {
        let above = render(Profile {
            pickup_transverse_offset_m: 0.0003,
            ..profile
        });
        let below = render(Profile {
            pickup_transverse_offset_m: -0.0003,
            ..profile
        });
        let reach = above.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
        assert!(reach > 1e-6, "compared two silences");
        let apart = above
            .iter()
            .zip(&below)
            .fold(0.0_f64, |m, (x, y)| m.max((x - y).abs()));
        apart / reach
    };

    let flat = Profile {
        pickup_law: PickupLaw::Aperture,
        tine_boundary_angle_rad: 0.0,
        ..Profile::calibrated()
    };
    assert!(
        separation(flat) <= 1e-12,
        "a tip on the axis should not be able to tell the two sides apart"
    );

    let elliptical = Profile {
        tine_boundary_angle_rad: 0.3,
        tine_transverse_frequency_ratio: 1.02,
        ..flat
    };
    let opened = separation(elliptical);
    assert!(
        opened > 0.01,
        "the ellipse leaned one way and nothing changed: {opened}"
    );
}

/// Level compensation has to be measured through the law the voice will
/// actually run. The reference motion is a sine along the strike direction, so
/// a tine whose axes are only slightly split reaches nearly the same
/// sensitivity as one that is not split at all; a profile that quietly fell
/// back to the production law would be off by a large factor instead.
#[test]
fn a_two_plane_profile_is_compensated_through_its_own_pickup() {
    let flat = Profile {
        pickup_law: PickupLaw::Aperture,
        ..Profile::calibrated()
    };
    let split = Profile {
        tine_boundary_angle_rad: 0.3,
        tine_transverse_frequency_ratio: 1.02,
        ..flat
    };
    let ratio = split.pickup_sensitivity() / flat.pickup_sensitivity();
    assert!(
        (ratio - 1.0).abs() < 1e-12,
        "the same geometry measured two different ways: {ratio}"
    );
    // Moving the tine across the pole changes how much voltage a given swing
    // makes, and the compensation has to see that rather than ignore the
    // coordinate. Which way it moves is geometry, not intuition: with a 2 mm
    // pole a 0.8 mm lean carries the tine towards the node ring rather than
    // away from the magnet, so the sensitivity rises.
    let leaned = Profile {
        pickup_transverse_offset_m: 0.0008,
        ..split
    };
    let leaned_ratio = leaned.pickup_sensitivity() / flat.pickup_sensitivity();
    assert!(
        (leaned_ratio - 1.0).abs() > 0.05,
        "leaning the tine across the pole changed nothing: {leaned_ratio}"
    );
}

/// An uncoupled tonebar is not there at all.
///
/// Every voicing that predates the second prong has to render through it
/// untouched, and at zero coupling that is exact rather than close: the pair
/// separates, the tonebar has no participation at the tine, so it is neither
/// struck nor heard, and the tine's mode is the fundamental it always was.
#[test]
fn an_unjoined_tonebar_leaves_the_tine_exactly_as_it_was() {
    // Without elastic mixing *and* without the clamp, the bar reaches the
    // tine by no path at all and cannot be heard whatever it is made of.
    // Those were one setting until `tonebar_clamp` separated them; see
    // docs/PLAYABLE-TONEBAR.md. This test is about the mixing, so it pins
    // the clamp rather than relying on a default that has since moved.
    for base in [Profile::default(), Profile::calibrated()] {
        let profile = Profile {
            tonebar_clamp: 0.0,
            ..base
        };
        assert_eq!(profile.tonebar_coupling, 0.0);
        for note in [FIRST_NOTE, 55, LAST_NOTE] {
            let mut alone = Voice::new(48_000.0, note, profile).unwrap();
            // A tonebar that cannot be heard should not be heard whatever it
            // is made of, so the twin carries a wildly different one.
            let mut twin = Voice::new(
                48_000.0,
                note,
                Profile {
                    tonebar_frequency_ratio: 2.2,
                    tonebar_mass_ratio: 40.0,
                    tonebar_decay_seconds: 0.4,
                    ..profile
                },
            )
            .unwrap();
            alone.strike(0.8);
            twin.strike(0.8);
            for step in 0..20_000 {
                let (left, right) = (alone.tick(), twin.tick());
                assert_eq!(left, right, "note {note} step {step}");
            }
        }
    }
}

/// Joining the prongs has to change the shape of the decay, not only its
/// speed.
///
/// A lone tine dies as one exponential, which is a straight line in decibels,
/// because nothing takes energy from it and gives it back. A fork does not:
/// the two normal modes damp at different rates and beat against each other,
/// so the envelope bends and ripples. The service manual's own account of the
/// instrument is that restraining the tonebar costs sustain, which only means
/// anything if the prongs trade.
#[test]
fn a_joined_tonebar_bends_the_decay_it_used_to_be_a_straight_line() {
    let straightness = |coupling: f64, ratio: f64| {
        let profile = Profile {
            tonebar_coupling: coupling,
            tonebar_frequency_ratio: ratio,
            ..Profile::calibrated()
        };
        let mut voice = Voice::new(48_000.0, 55, profile).unwrap();
        voice.strike(0.7);
        // Envelope in decibels over two seconds, one point per 20 ms.
        let mut points = Vec::new();
        for _ in 0..100 {
            let mut power = 0.0;
            for _ in 0..(48_000 * 4 / 50) {
                let value = voice.tick();
                power += value * value;
            }
            points.push(10.0 * (power / 3840.0).max(1e-30).log10());
        }
        // Residual of the straight line through it, which is what a single
        // exponential would leave behind: nothing.
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
        (points
            .iter()
            .enumerate()
            .map(|(i, y)| {
                let fit = mean_y + slope * (i as f64 - mean_x);
                (y - fit) * (y - fit)
            })
            .sum::<f64>()
            / n)
            .sqrt()
    };
    let alone = straightness(0.0, 1.5);
    // Where the coupling pulls the two normal modes close together, the note
    // beats against itself and the envelope ripples hard.
    let close = straightness(0.2, 1.08);
    assert!(
        close > 10.0 * alone,
        "the fork barely bent the decay: {close:.3} dB against {alone:.3} dB alone"
    );
    // And this pins what the probe found: at the spacing the measurements
    // actually report, several hundred cents and more, the prongs are too far
    // apart to trade and the envelope stays as straight as a lone tine's.
    // docs/PLAYABLE-TONEBAR.md records that as a failed prediction rather
    // than hiding it, and this holds the finding in place.
    let apart = straightness(0.6, 1.5);
    assert!(
        apart < 2.0 * alone,
        "the far-spaced fork now bends the decay after all: {apart:.3} against {alone:.3}"
    );
}

/// The joined fork still takes no energy from the hammer, over eight
/// coordinates instead of six.
#[test]
fn a_fork_with_both_prongs_stays_passive() {
    let profile = Profile {
        tonebar_coupling: 0.8,
        tine_boundary_angle_rad: 0.4,
        tine_transverse_frequency_ratio: 1.05,
        ..Profile::calibrated()
    };
    for note in [FIRST_NOTE, 55, LAST_NOTE] {
        for velocity in [0.05, 0.5, 1.0] {
            let mut voice = Voice::new(48_000.0, note, profile).unwrap();
            voice.strike(velocity);
            let mut previous = voice.probe().mechanical_energy_j;
            for _ in 0..(48_000.0 * OVERSAMPLE as f64 * 0.04) as usize {
                let value = voice.tick();
                assert!(value.is_finite());
                let now = voice.probe().mechanical_energy_j;
                assert!(
                    now <= previous * (1.0 + 1e-8) + 1e-15,
                    "energy grew at {note}/{velocity}: {now} > {previous}"
                );
                previous = now;
            }
            assert!(!voice.probe().contact_active, "contact did not separate");
        }
    }
}

/// A tonebar asked for a short decay at a low frequency would stop
/// oscillating, and a mode that stops oscillating comes back as NaN rather
/// than as a thud. Every corner of the validated ranges has to stay audible
/// and finite.
#[test]
fn no_tonebar_the_ranges_allow_can_turn_a_note_into_nothing() {
    for ratio in [0.25, 1.0, 4.0] {
        for decay in [0.05, 80.0] {
            for mass in [0.1, 100.0] {
                for coupling in [0.001, 4.0] {
                    let profile = Profile {
                        tonebar_coupling: coupling,
                        tonebar_frequency_ratio: ratio,
                        tonebar_mass_ratio: mass,
                        tonebar_decay_seconds: decay,
                        ..Profile::calibrated()
                    };
                    for note in [FIRST_NOTE, LAST_NOTE] {
                        let mut voice = Voice::new(48_000.0, note, profile).unwrap();
                        voice.strike(1.0);
                        let mut reach = 0.0_f64;
                        for _ in 0..40_000 {
                            let value = voice.tick();
                            assert!(
                                value.is_finite(),
                                "{ratio}/{decay}/{mass}/{coupling} at {note} went non-finite"
                            );
                            reach = reach.max(value.abs());
                        }
                        assert!(
                            reach > 1e-9,
                            "{ratio}/{decay}/{mass}/{coupling} at {note} was silent"
                        );
                    }
                }
            }
        }
    }
}

/// Joining the tonebar must not move the note.
///
/// A coupling spring stiffens the tine, so an assembled fork rings sharp: a
/// tenth of the tine's own stiffness is 83 cents and three times it is a full
/// octave. A real tine is tuned after assembly, with its tuning spring, and
/// the first version of this model was not — which made the calibration
/// search read the detuning as the tonebar and reject it at four to five
/// times the error. The search was right about the numbers and wrong about
/// the cause, and this is what stops that happening again.
#[test]
fn joining_the_tonebar_leaves_the_note_where_it_was() {
    for note in [FIRST_NOTE, 40, 55, 76, LAST_NOTE] {
        let target = 440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0);
        let mut alone = 0.0;
        for coupling in [0.0, 0.1, 0.8, 3.0] {
            let profile = Profile {
                tonebar_coupling: coupling,
                ..Profile::calibrated()
            };
            let mut voice = Voice::new(48_000.0, note, profile).unwrap();
            voice.strike(0.6);
            let samples: Vec<f64> = (0..(48_000 * 4))
                .map(|_| {
                    voice.tick();
                    voice.probe().displacement_m
                })
                .collect();
            // The loudest partial within a fourth of the note, found by
            // scanning rather than by counting zero crossings: two partials
            // of comparable size make a crossing counter read a frequency
            // that is not there, which is what it did here once the contact
            // softened and the balance between them changed.
            let energy = |frequency: f64| {
                let (mut re, mut im) = (0.0, 0.0);
                for (i, value) in samples.iter().enumerate() {
                    let turn = i as f64 / samples.len() as f64;
                    let window = 0.5 - 0.5 * (std::f64::consts::TAU * turn).cos();
                    let phase = std::f64::consts::TAU * frequency * i as f64 / 192_000.0;
                    re += value * window * phase.cos();
                    im += value * window * phase.sin();
                }
                (re * re + im * im).sqrt()
            };
            let mut heard = target;
            let mut loudest = 0.0;
            for step in -60..=60 {
                let frequency = target * 2.0_f64.powf(f64::from(step) * 5.0 / 1200.0);
                let level = energy(frequency);
                if level > loudest {
                    loudest = level;
                    heard = frequency;
                }
            }
            if coupling == 0.0 {
                alone = heard;
            }
            let cents = 1200.0 * (heard / target).log2();
            assert!(
                cents.abs() < 35.0,
                "note {note} at coupling {coupling} rang {heard} Hz against {target:.1}, \
                 {cents:.0} cents out"
            );
            assert!(
                (1200.0 * (heard / alone).log2()).abs() <= 12.0,
                "note {note} at coupling {coupling} moved from {alone:.1} to {heard:.1} Hz"
            );
        }
    }
}

/// A wedge-ground pole must not invert anywhere the tine goes.
///
/// docs/PICKUP-GEOMETRY-CEILING.md found the disc's flux slope changing sign
/// about 390 µm out at the geometry this model needs, which is inside the
/// swing the bass notes reach: their waveform turns over mid-cycle. That
/// spread belongs to the face being circular, so grinding it towards an edge
/// across the direction of travel has to remove it by construction, at every
/// gap the service manual allows and at the one the model actually uses.
///
/// This is the structural half of the wedge's prediction: it either inverts
/// or it does not.
#[test]
fn a_wedge_ground_pole_never_turns_the_flux_slope_over() {
    use rf_tines_dsp::{AxialAperture, SpatialPickupProfile, aperture_voltage};
    // The bottom note swings 1.83 mm at full velocity.
    const REACH_M: f64 = 0.002;
    for gap_mm in [0.5, 1.588, 2.4, 3.175] {
        for offset_mm in [0.1, 0.25, 0.5] {
            let build = |wedge: f64| {
                AxialAperture::new(SpatialPickupProfile {
                    gap_m: gap_mm * 1e-3,
                    offset_xy_m: [offset_mm * 1e-3, 0.0],
                    pole_radius_m: 0.002,
                    pole_wedge: wedge,
                    flux_scale_wb: 0.001,
                })
                .unwrap()
            };
            let inversions = |wedge: f64| {
                let pickup = build(wedge);
                let mut flips = 0;
                let mut previous = aperture_voltage(&pickup, -REACH_M, 1.0);
                let mut step = -REACH_M;
                while step < REACH_M {
                    step += 1e-6;
                    let now = aperture_voltage(&pickup, step, 1.0);
                    if now * previous < 0.0 {
                        flips += 1;
                    }
                    previous = now;
                }
                flips
            };
            // Exactly one, and only one. Every pickup's flux slope passes
            // through zero where the tine sits dead in front of the pole,
            // because the flux is at its extremum there; that crossing is
            // physics and cannot be removed. What the disc adds is a second
            // one further out, and that is the artefact.
            assert_eq!(
                inversions(1.0),
                1,
                "a wedge at {gap_mm} / {offset_mm} mm does not cross exactly once"
            );
        }
    }
    // And the disc it replaces crosses twice at the geometry the model
    // needs, so the test is measuring a real difference and not an empty
    // range. 390 um out is where docs/PICKUP-GEOMETRY-CEILING.md found the
    // second one, inside the swing the bass notes reach.
    let disc = AxialAperture::new(SpatialPickupProfile {
        gap_m: 0.0005,
        offset_xy_m: [0.0005, 0.0],
        pole_radius_m: 0.002,
        pole_wedge: 0.0,
        flux_scale_wb: 0.001,
    })
    .unwrap();
    let mut flips = 0;
    let mut previous = aperture_voltage(&disc, -REACH_M, 1.0);
    let mut step = -REACH_M;
    while step < REACH_M {
        step += 1e-6;
        let now = aperture_voltage(&disc, step, 1.0);
        if now * previous < 0.0 {
            flips += 1;
        }
        previous = now;
    }
    assert!(
        flips > 1,
        "the disc stopped adding a crossing, so this test proves nothing: {flips}"
    );
}

/// Grinding nothing off the face leaves the pole exactly as it was.
#[test]
fn a_pole_with_no_wedge_is_the_disc_it_always_was() {
    use rf_tines_dsp::{AxialAperture, SpatialPickupProfile, aperture_voltage};
    let build = |wedge: f64| {
        AxialAperture::new(SpatialPickupProfile {
            gap_m: 0.0005,
            offset_xy_m: [0.0005, 0.0],
            pole_radius_m: 0.002,
            pole_wedge: wedge,
            flux_scale_wb: 0.001,
        })
        .unwrap()
    };
    let (plain, unground) = (build(0.0), build(0.0));
    for step in -20..=20 {
        let x = f64::from(step) * 1e-4;
        assert_eq!(
            aperture_voltage(&plain, x, 0.7),
            aperture_voltage(&unground, x, 0.7)
        );
    }
    // And a ground one is genuinely different, or the parameter does nothing.
    let ground = build(1.0);
    let apart = (-20..=20)
        .map(|step| {
            let x = f64::from(step) * 1e-4;
            (aperture_voltage(&plain, x, 0.7) - aperture_voltage(&ground, x, 0.7)).abs()
        })
        .fold(0.0_f64, f64::max);
    assert!(apart > 1e-3, "grinding the face changed nothing: {apart}");
}

/// A voice must ring at one pitch.
///
/// The calibration objective scores the levels of the second, third and
/// fourth harmonics against the fundamental, in two windows. It has no term
/// for a *second pitch*. So a search is free to park a strong partial a
/// semitone from the fundamental if that happens to move those levels the
/// right way, and one did: a fitted candidate put 78 cents and -10.7 dB next
/// to the note, which beats against it at 9 Hz and is heard as an out-of-tune
/// instrument with a wobble. Five hundred tests and the promotion gate all
/// passed it; a listener caught it in one note.
///
/// This is the missing term, as a guard. Near the fundamental there is room
/// for exactly one partial, and everything else has to be far enough down
/// that it cannot be a competing pitch.
#[test]
fn a_voice_rings_at_one_pitch_and_not_two() {
    /// How close another partial may sit, in cents, before it stops being an
    /// overtone and starts being a second note.
    const NEIGHBOURHOOD_CENTS: f64 = 350.0;
    /// And how far below the fundamental it has to stay if it sits there.
    const HEADROOM_DB: f64 = 24.0;

    let offenders = |profile: Profile, note: u8| -> Vec<(f64, f64)> {
        let target = 440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0);
        let mut voice = Voice::new(48_000.0, note, profile).unwrap();
        voice.strike(0.85);
        // Past the attack, where a second pitch would be heard as tuning.
        for _ in 0..(48_000 * 4 * 3 / 10) {
            voice.tick();
        }
        // Two seconds, windowed. A first attempt used a bare sum over one
        // second and flagged the shipping profile's lowest note, which was
        // its own spectral leakage and not a second pitch: without a window
        // the skirt of a 41 Hz tone is still 6 dB down a quarter tone away.
        const SECONDS: f64 = 2.0;
        let taken = (48_000.0 * 4.0 * SECONDS) as usize;
        let samples: Vec<f64> = (0..taken).map(|_| voice.tick()).collect();
        let energy = |frequency: f64| {
            let (mut re, mut im) = (0.0, 0.0);
            for (i, value) in samples.iter().enumerate() {
                let turn = i as f64 / samples.len() as f64;
                let window = 0.5 - 0.5 * (std::f64::consts::TAU * turn).cos();
                let phase = std::f64::consts::TAU * frequency * i as f64 / (48_000.0 * 4.0);
                re += value * window * phase.cos();
                im += value * window * phase.sin();
            }
            ((re * re + im * im).sqrt() / samples.len() as f64).max(1e-30)
        };
        let root = energy(target);
        // What the analysis itself cannot resolve, it must not judge: a Hann
        // window over this long spreads a pure tone across about three bins,
        // so anything nearer than that is the fundamental seen twice.
        let blind_hz = 3.0 / SECONDS;
        let mut found = Vec::new();
        let mut cents = -NEIGHBOURHOOD_CENTS;
        while cents <= NEIGHBOURHOOD_CENTS {
            let frequency = target * 2.0_f64.powf(cents / 1200.0);
            if (frequency - target).abs() >= blind_hz {
                let level = 20.0 * (energy(frequency) / root).log10();
                if level > -HEADROOM_DB {
                    found.push((cents, level));
                }
            }
            cents += 12.5;
        }
        found
    };

    for profile in [
        Profile::default(),
        Profile::calibrated(),
        Profile::calibrated_sustain(),
    ] {
        for note in [FIRST_NOTE, 40, 55, 76, LAST_NOTE] {
            let found = offenders(profile, note);
            assert!(
                found.is_empty(),
                "note {note} rings at more than one pitch: {found:?}"
            );
        }
    }

    // And the guard has to be able to fail, or it guards nothing. This is
    // the whole fitted candidate a listener rejected on hearing -- not just
    // its tonebar, because which normal mode leads, and therefore where the
    // second pitch lands, depends on the entire configuration. At G3 it puts
    // 205 Hz beside 196 Hz at -10.7 dB. See docs/WEDGE-POLE.md.
    let two_pitched = Profile {
        pickup_gap_m: 0.001377,
        pickup_offset_m: 0.0006359,
        pickup_transverse_offset_m: -0.0004375,
        tine_boundary_angle_rad: 0.3250,
        tine_transverse_frequency_ratio: 1.0434,
        maximum_hammer_speed_m_s: 1.3250,
        velocity_exponent: 1.1656,
        tonebar_coupling: 0.5625,
        tonebar_frequency_ratio: 1.4016,
        pickup_pole_wedge: 1.0,
        pickup_pole_radius_m: 0.0010625,
        pickup_law: PickupLaw::Aperture,
        ..Profile::calibrated()
    };
    let caught = offenders(two_pitched, 55);
    assert!(
        !caught.is_empty(),
        "the guard did not catch the candidate a listener rejected"
    );
}

/// The circuit law cannot invert, and cannot be made to.
///
/// The disc sums a source across the direction the tine travels, so its flux
/// slope can turn over inside the playing range, and 390 µm out it does. The
/// wedge removes that by collapsing the spread, but it had to be arranged and
/// an orientation had to be assumed. A circuit cannot do it at all: flux is
/// the magnet's drive over a reluctance that grows monotonically as the tine
/// slides off the pole, so the slope has exactly one zero, at the axis, for
/// every geometry there is.
///
/// This is the structural half of the prediction in docs/RELUCTANCE-PICKUP.md,
/// and it is checked by exhausting the validated ranges rather than by
/// sampling a few.
#[test]
fn the_circuit_law_has_one_zero_and_no_geometry_adds_another() {
    use rf_tines_dsp::{ReluctancePickup, SpatialPickupProfile, reluctance_voltage};
    // Wider than the bottom note's 1.83 mm swing, and past the pole's edge.
    const REACH_M: f64 = 0.004;
    let mut checked = 0;
    for gap_mm in [0.5, 1.0, 1.588, 2.4, 3.175, 5.0] {
        for offset_mm in [-1.0, -0.25, 0.0, 0.1, 0.5, 1.5] {
            for width_mm in [0.2, 0.5, 1.0, 2.0, 3.0] {
                for floor in [0.0, 0.25, 1.0, 4.0, 100.0] {
                    let pickup = ReluctancePickup::new(
                        SpatialPickupProfile {
                            gap_m: gap_mm * 1e-3,
                            offset_xy_m: [offset_mm * 1e-3, 0.0],
                            pole_radius_m: width_mm * 1e-3,
                            pole_wedge: 0.0,
                            flux_scale_wb: 0.001,
                        },
                        floor,
                    )
                    .unwrap();
                    let mut flips = 0;
                    let mut previous = reluctance_voltage(&pickup, -REACH_M, 1.0);
                    let mut step = -REACH_M;
                    while step < REACH_M {
                        step += 2e-6;
                        let now = reluctance_voltage(&pickup, step, 1.0);
                        assert!(now.is_finite(), "{gap_mm}/{offset_mm}/{width_mm}/{floor}");
                        if now * previous < 0.0 {
                            flips += 1;
                        }
                        previous = now;
                    }
                    assert_eq!(
                        flips, 1,
                        "gap {gap_mm} offset {offset_mm} width {width_mm} floor {floor} \
                         crossed {flips} times"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 900);
}
