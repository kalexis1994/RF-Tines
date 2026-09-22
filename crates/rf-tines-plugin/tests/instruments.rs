use rackforge_plugin_sdk::{MidiEvent, Processor};
use rf_tines_plugin::{RfTinesProcessor, STATE_BYTES, Settings, presets};

#[test]
fn v4_state_migrates_to_neutral_electronics_and_identical_audio() {
    let mut old = RfTinesProcessor::default();
    assert!(old.load_preset("calibrated-register"));
    let mut bytes = [0; STATE_BYTES];
    old.save_state(&mut bytes).unwrap();
    bytes[4..8].copy_from_slice(&4u32.to_le_bytes());
    let mut migrated = RfTinesProcessor::default();
    assert!(migrated.load_state(&bytes[..68]));
    for index in 0..15 {
        assert_eq!(old.get_parameter(index), migrated.get_parameter(index));
    }
    for plugin in [&mut old, &mut migrated] {
        assert!(plugin.prepare(48000.0, 256, 0, 2));
    }
    let note = [MidiEvent {
        frame: 0,
        data: [0x90, 60, 110],
        length: 3,
    }];
    for block in 0..32 {
        let mut a = [0.0; 512];
        let mut b = a;
        let events = if block == 0 { &note[..] } else { &[] };
        old.process(&[], &mut a, events, &[], 256, 0, 2);
        migrated.process(&[], &mut b, events, &[], 256, 0, 2);
        assert_eq!(a, b);
    }
    let mut json = serde_json::to_value(Settings::default()).unwrap();
    for key in [
        "bass_db",
        "treble_db",
        "vibrato",
        "speed_hz",
        "intensity",
        "preamp",
        "bass_boost",
    ] {
        json.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(
        serde_json::from_value::<Settings>(json).unwrap(),
        Settings::default()
    );
}

#[test]
fn electronic_fields_roundtrip_and_invalid_state_is_atomic() {
    let mut plugin = RfTinesProcessor::default();
    for (index, value) in [
        (8, 6.0),
        (9, -3.0),
        (10, 1.0),
        (11, 7.5),
        (12, 0.8),
        (13, 0.0),
        (14, 0.4),
    ] {
        assert!(plugin.set_parameter(index, value));
    }
    let mut state = [0; STATE_BYTES];
    plugin.save_state(&mut state).unwrap();
    let mut restored = RfTinesProcessor::default();
    assert!(restored.load_state(&state));
    for i in 0..15 {
        assert_eq!(restored.get_parameter(i), plugin.get_parameter(i));
    }
    for i in 0..7 {
        let mut bad = state;
        bad[68 + i * 8..76 + i * 8].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(!restored.load_state(&bad));
        let mut after = [0; STATE_BYTES];
        restored.save_state(&mut after).unwrap();
        assert_eq!(after, state);
    }
}

#[test]
fn instrument_programs_have_distinct_physics_and_bounded_dense_chords() {
    let factory = presets();
    for a in 5..factory.len() {
        for b in a + 1..factory.len() {
            let x = factory[a].3;
            let y = factory[b].3;
            assert_ne!(
                [
                    x.hardness, x.sustain, x.bell, x.distance_mm, x.alignment_mm,
                    x.hammer, x.tine, x.pole, x.twist
                ],
                [
                    y.hardness, y.sustain, y.bell, y.distance_mm, y.alignment_mm,
                    y.hammer, y.tine, y.pole, y.twist
                ]
            );
        }
        let (id, _, _, settings) = factory[a];
        let mut plugin = RfTinesProcessor::default();
        assert!(plugin.load_preset(id));
        assert!(plugin.prepare(48000.0, 256, 0, 2));
        let mut peak = 0.0_f32;
        let mut stereo = false;
        for block in 0..375 {
            let mut notes = Vec::new();
            if [0, 94, 188].contains(&block) {
                for note in [40, 47, 52, 55, 59, 62, 64, 67, 71, 76] {
                    notes.push(MidiEvent {
                        frame: 0,
                        data: [0x90, note, 127],
                        length: 3,
                    });
                }
            }
            let mut audio = [0.0; 512];
            plugin.process(&[], &mut audio, &notes, &[], 256, 0, 2);
            for pair in audio.as_chunks::<2>().0 {
                assert!(pair[0].is_finite() && pair[1].is_finite());
                peak = peak.max(pair[0].abs()).max(pair[1].abs());
                stereo |= (pair[0] - pair[1]).abs() > 1e-5;
            }
        }
        println!("{id}: peak={peak:.6}, stereo={stereo}");
        assert!(peak > 0.001 && peak < 1.0, "{id}: {peak}");
        assert_eq!(stereo, settings.preamp == 1.0 && settings.vibrato == 1.0);
    }
}

/// Every instrument preset has to be a different sound, not a different set
/// of numbers.
///
/// The five decade presets these replaced failed that: the controls they
/// varied to tell the eras apart -- tip hardness above all -- move the
/// spectrum by well under a decibel over the spans they used, so the eras
/// were mostly a label. Measured on the physics alone, the instruments here
/// separate by 2.4 to 5.9 dB, with three pairs closer.
///
/// Two of those three are the same piano twice, a Stage and the Suitcase of
/// the same years, and they are meant to share a mechanism: what separates
/// them is the preamp and its stereo vibrato. That is worth pinning, because
/// it is the only thing keeping them apart.
#[test]
fn a_suitcase_does_not_sound_like_the_stage_it_shares_a_mechanism_with() {
    let capture = |id: &str| {
        let mut plugin = RfTinesProcessor::default();
        assert!(plugin.load_preset(id), "{id}");
        assert!(plugin.prepare(48_000.0, 256, 0, 2));
        let note = [MidiEvent {
            frame: 0,
            data: [0x90, 55, 100],
            length: 3,
        }];
        let mut left = Vec::new();
        let mut right = Vec::new();
        for block in 0..80 {
            let mut audio = [0.0_f32; 512];
            let events: &[MidiEvent] = if block == 0 { &note } else { &[] };
            plugin.process(&[], &mut audio, events, &[], 256, 0, 2);
            for pair in audio.as_chunks::<2>().0 {
                left.push(pair[0]);
                right.push(pair[1]);
            }
        }
        (left, right)
    };

    for (stage, suitcase) in [
        ("portable-bark-1972", "console-bark-1973"),
        ("portable-bell-1977", "console-bell-1978"),
    ] {
        let (dry, dry_right) = capture(stage);
        let (wet, wet_right) = capture(suitcase);
        let reach = dry.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
        assert!(reach > 1e-3, "{stage} barely sounded");

        // The Stage is passive and mono; the Suitcase swings its output
        // between two speakers, so its channels must part company.
        let stage_spread = dry
            .iter()
            .zip(&dry_right)
            .fold(0.0_f32, |m, (l, r)| m.max((l - r).abs()));
        let suitcase_spread = wet
            .iter()
            .zip(&wet_right)
            .fold(0.0_f32, |m, (l, r)| m.max((l - r).abs()));
        assert!(stage_spread <= 1e-6, "{stage} is not mono: {stage_spread}");
        assert!(
            suitcase_spread > 0.1 * reach,
            "{suitcase} has no stereo movement: {suitcase_spread}"
        );

        // And the preamp's own tone shaping has to be audible before the
        // vibrato is taken into account, so compare the two channels summed.
        let apart = dry
            .iter()
            .zip(&dry_right)
            .zip(wet.iter().zip(&wet_right))
            .fold(0.0_f32, |m, ((a, b), (c, d))| m.max(((a + b) - (c + d)).abs()));
        assert!(
            apart > 0.05 * reach,
            "{stage} and {suitcase} came out the same: {apart} against {reach}"
        );
    }
}

/// A preset has to be a different sound, and this is the test the earlier
/// ones never had.
///
/// Five decade presets shipped once whose whole claim was that they were
/// different instruments; measured, they differed by well under a decibel in
/// the places their descriptions pointed at, because the controls that were
/// varied could not reach the physics. Nothing caught it, because nothing
/// measured what came out.
///
/// So this measures what comes out. Each advertised preset is rendered
/// through the plugin -- the whole path, level compensation, register
/// geometry and panel electronics included -- and reduced to a coarse
/// log-spaced colour, normalised to its own peak so a loudness difference is
/// not counted as a timbre difference. Every pair must part company, except
/// the two that are one instrument heard through two amplifiers.
#[test]
fn every_advertised_preset_is_audibly_its_own_instrument() {
    const RATE: f64 = 48_000.0;
    const FRAMES: u32 = 256;
    const ADVERTISED: [&str; 8] = [
        "portable-bark-1972",
        "tine-bass-1960",
        "felt-1966",
        "console-bark-1973",
        "portable-bell-1977",
        "console-bell-1978",
        "portable-chime-1980",
        "wide-dynamics-1984",
    ];
    /// One piano, two amplifiers. The electronics separate these, and
    /// `a_suitcase_does_not_sound_like_the_stage_it_shares_a_mechanism_with`
    /// is what holds them apart.
    const SHARE_A_MECHANISM: [(&str, &str); 2] = [
        ("portable-bark-1972", "console-bark-1973"),
        ("portable-bell-1977", "console-bell-1978"),
    ];

    let colour = |id: &str| {
        let mut plugin = RfTinesProcessor::default();
        assert!(plugin.load_preset(id), "{id}");
        assert!(plugin.prepare(RATE, FRAMES, 0, 2));
        let note = [MidiEvent {
            frame: 0,
            data: [0x90, 55, 108],
            length: 3,
        }];
        let mut mono = Vec::new();
        for block in 0..160 {
            let mut audio = [0.0_f32; 512];
            let events: &[MidiEvent] = if block == 0 { &note } else { &[] };
            plugin.process(&[], &mut audio, events, &[], FRAMES, 0, 2);
            mono.extend(
                audio
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| f64::from(pair[0]) + f64::from(pair[1])),
            );
        }
        assert!(mono.iter().any(|x| x.abs() > 1e-4), "{id} was silent");
        // Four probes per octave from 80 Hz, over the first second.
        let block = &mono[..mono.len().min(RATE as usize)];
        let mut bands: Vec<f64> = (0..29)
            .map(|step| {
                let frequency = 80.0 * 2.0_f64.powf(step as f64 / 4.0);
                let (mut re, mut im) = (0.0, 0.0);
                for (i, value) in block.iter().enumerate() {
                    let phase = std::f64::consts::TAU * frequency * i as f64 / RATE;
                    re += value * phase.cos();
                    im += value * phase.sin();
                }
                (re * re + im * im).sqrt() / block.len() as f64
            })
            .collect();
        let peak = bands.iter().fold(0.0_f64, |m, x| m.max(*x)).max(1e-30);
        for value in &mut bands {
            *value = 20.0 * (*value / peak).max(1e-5).log10();
        }
        bands
    };

    let prints: Vec<(&str, Vec<f64>)> = ADVERTISED.iter().map(|id| (*id, colour(id))).collect();
    let mut spread = Vec::new();
    for (index, (a, left)) in prints.iter().enumerate() {
        for (b, right) in &prints[index + 1..] {
            if SHARE_A_MECHANISM.contains(&(a, b)) || SHARE_A_MECHANISM.contains(&(b, a)) {
                continue;
            }
            let apart = (left
                .iter()
                .zip(right)
                .map(|(x, y)| (x - y) * (x - y))
                .sum::<f64>()
                / left.len() as f64)
                .sqrt();
            assert!(apart > 2.0, "{a} and {b} are the same sound: {apart:.2} dB");
            spread.push(apart);
        }
    }
    // Pair by pair is not enough on its own: a later edit could walk every
    // preset towards the middle and still have each pair scrape past the
    // floor. The set has to stay spread out as a whole. The measured mean is
    // around 6 dB; this floor is well under it, so it catches a collapse
    // rather than fencing in the current numbers.
    let mean = spread.iter().sum::<f64>() / spread.len() as f64;
    assert!(mean > 4.0, "the set has flattened to a mean of {mean:.2} dB");
}
