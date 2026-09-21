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
            let mut previous = voice.probe().displacement_m;
            let mut crossings = Vec::new();
            for frame in 0..(internal_rate * 0.12) as usize {
                voice.tick();
                let now = voice.probe().displacement_m;
                if previous < 0.0 && now >= 0.0 {
                    crossings.push(frame as f64 + (-previous) / (now - previous));
                }
                previous = now;
            }
            assert!(crossings.len() > 2);
            let frequency = (crossings.len() - 1) as f64 * internal_rate
                / (crossings.last().unwrap() - crossings[0]);
            let expected = 440.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0);
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
    let mut previous = after.displacement_m;
    let mut crossings = Vec::new();
    for frame in 0..(internal_rate * 0.12) as usize {
        voice.tick();
        let now = voice.probe().displacement_m;
        if previous < 0.0 && now >= 0.0 {
            crossings.push(frame as f64 + (-previous) / (now - previous));
        }
        previous = now;
    }
    let frequency =
        (crossings.len() - 1) as f64 * internal_rate / (crossings.last().unwrap() - crossings[0]);
    let expected = 440.0 * 2.0_f64.powf((57.0 - 69.0) / 12.0) * ratio;
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
    assert!(apart <= 1e-7 * moved, "{apart} apart on a signal of {moved}");
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
    assert!(leak <= 1e-12 * swing, "leaked {leak} against a swing of {swing}");
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
    assert!(
        across > along * 0.01,
        "transverse {across} against {along}"
    );
    assert!(area > 0.0, "the tip stayed on a line");
}

/// The two-dimensional flux is the same flux, not a second law. On the axis it
/// returns exactly what the ten-square-root reduction returns, and its
/// transverse component is zero there because the mirrored halves of the pole
/// ring cancel.
#[test]
fn the_planar_flux_agrees_with_the_axial_reduction_on_the_axis() {
    use rf_tines_dsp::{APERTURE_PICKUP, AxialAperture, PlanarAperture, aperture_voltage,
        planar_voltage};
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
