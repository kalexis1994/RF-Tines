use rf_tines_dsp::{Engine, OVERSAMPLE, Profile, Voice};

#[test]
fn intermediate_pedal_positions_order_the_released_tines_energy() {
    for rate in [44_100.0, 48_000.0, 96_000.0] {
        for note in [40, 57, 81] {
            let mut energies = Vec::new();
            for cc in [0, 32, 64, 96, 127] {
                let mut engine = Engine::new(rate, Profile::default()).unwrap();
                engine.control_change(0, 64, f64::from(cc) / 127.0);
                engine.note_on(0, note, 0.8);
                for _ in 0..(rate * 0.05) as usize {
                    engine.next_sample();
                }
                engine.note_off(0, note);
                for _ in 0..(rate * 0.25) as usize {
                    assert!(engine.next_sample().is_finite());
                }
                energies.push(engine.probe(note).unwrap().mechanical_energy_j);
                assert_eq!(engine.faults(), 0);
            }
            assert!(
                energies.windows(2).all(|pair| pair[1] > pair[0] * 1.2),
                "pedal response at {rate}/{note}: {energies:?}"
            );
        }
    }
}

#[test]
fn moving_the_damper_preserves_motion_and_never_adds_energy() {
    for rate in [44_100.0, 96_000.0, 192_000.0] {
        for note in [28, 57, 100] {
            let mut voice = Voice::new(rate, note, Profile::default()).unwrap();
            voice.strike(0.8);
            let mut previous = voice.probe().mechanical_energy_j;
            for frame in 0..(rate * OVERSAMPLE as f64 * 0.04) as usize {
                if frame % 64 == 0 {
                    let pressure = f64::from((frame / 64 % 127) as u8) / 127.0;
                    let before = voice.probe();
                    assert!(voice.set_damper_pressure(pressure));
                    let after = voice.probe();
                    assert_eq!(after.displacement_m, before.displacement_m);
                    assert_eq!(after.velocity_m_s, before.velocity_m_s);
                }
                assert!(voice.tick().is_finite());
                let energy = voice.probe().mechanical_energy_j;
                assert!(energy <= previous * (1.0 + 1e-8) + 1e-15);
                previous = energy;
            }
            assert!(!voice.set_damper_pressure(f64::NAN));
            assert!(!voice.set_damper_pressure(-0.1));
            assert!(!voice.set_damper_pressure(1.1));
        }
    }
}

#[test]
fn a_held_key_and_an_unrelated_channels_pedal_keep_their_ownership() {
    let mut reference = Engine::new(48_000.0, Profile::default()).unwrap();
    let mut moved = Engine::new(48_000.0, Profile::default()).unwrap();
    for engine in [&mut reference, &mut moved] {
        engine.note_on(0, 57, 0.8);
    }
    for frame in 0..6000 {
        if frame % 128 == 0 {
            moved.control_change(0, 64, f64::from((frame / 128 % 127) as u8) / 127.0);
        }
        assert_eq!(reference.next_sample(), moved.next_sample());
    }
    for engine in [&mut reference, &mut moved] {
        engine.control_change(0, 64, 0.7);
        engine.note_off(0, 57);
    }
    for frame in 0..6000 {
        if frame % 128 == 0 {
            moved.control_change(1, 64, f64::from((frame / 128 % 127) as u8) / 127.0);
        }
        assert_eq!(reference.next_sample(), moved.next_sample());
    }
}

#[test]
fn a_partial_pedal_can_recapture_a_tail_and_reset_controllers_releases_it() {
    let mut engine = Engine::new(48_000.0, Profile::default()).unwrap();
    engine.note_on(0, 57, 0.8);
    for _ in 0..3000 {
        engine.next_sample();
    }
    engine.note_off(0, 57);
    for _ in 0..200 {
        engine.next_sample();
    }
    let before = engine.probe(57).unwrap().mechanical_energy_j;
    engine.control_change(0, 64, 0.8);
    assert_eq!(engine.probe(57).unwrap().mechanical_energy_j, before);
    for _ in 0..12_000 {
        engine.next_sample();
    }
    assert!(engine.probe(57).unwrap().mechanical_energy_j > before * 0.1);
    engine.control_change(0, 121, 0.0);
    for _ in 0..24_000 {
        engine.next_sample();
    }
    assert!(engine.probe(57).unwrap().mechanical_energy_j < 1e-18);
    engine.control_change(0, 64, 0.8);
    engine.reset();
    engine.note_on(0, 57, 0.8);
    engine.note_off(0, 57);
    for _ in 0..24_000 {
        engine.next_sample();
    }
    assert!(engine.probe(57).unwrap().mechanical_energy_j < 1e-18);
}
