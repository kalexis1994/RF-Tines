use rackforge_plugin_sdk::{
    MIDI2_FLAG_ORIGIN_7BIT, MIDI2_KIND_NOTE_ON, MIDI2_KIND_PITCH_BEND, MidiEvent, MidiEvent2,
    ParameterEvent, Processor,
};
use rf_tines_plugin::{DEFAULT_GAIN, RfTinesProcessor, STATE_BYTES};

fn prepared() -> RfTinesProcessor {
    let mut plugin = RfTinesProcessor::default();
    assert!(plugin.prepare(48_000.0, 4096, 0, 2));
    plugin
}

fn render(block: usize, note: u8) -> Vec<f32> {
    let events = [
        (17, [0x90, note, 110]),
        (230, [0xb0, 64, 127]),
        (477, [0x80, note, 0]),
        (700, [0xb0, 64, 32]),
        (900, [0xb0, 64, 96]),
        (997, [0x90, note, 70]),
        (1900, [0x80, note, 0]),
        (2000, [0xb0, 64, 64]),
        (2100, [0xb0, 64, 0]),
    ];
    let mut plugin = prepared();
    let mut result = Vec::new();
    for start in (0..4096).step_by(block) {
        let frames = block.min(4096 - start);
        let midi: Vec<_> = events
            .iter()
            .filter(|(t, _)| *t >= start && *t < start + frames)
            .map(|(t, data)| MidiEvent {
                frame: (t - start) as u32,
                data: *data,
                length: 3,
            })
            .collect();
        // Includes law, geometry and loss changes while notes ring, with simultaneous MIDI.
        let parameters: Vec<_> = [
            (230, 1, 1.0),
            (477, 1, 0.0),
            (510, 2, 0.8),
            (800, 3, 0.0),
            (997, 5, 0.5),
            (1234, 0, 0.3),
            (2100, 6, 0.3),
            (2400, 8, 6.0),
            (2410, 9, -3.0),
            (2500, 10, 1.0),
            (2500, 12, 0.8),
            (2700, 11, 7.0),
            (3100, 13, 0.0),
            (3150, 14, 0.4),
            (3600, 13, 1.0),
        ]
        .into_iter()
        .filter(|(t, _, _)| (start..start + frames).contains(t))
        .map(|(t, index, value)| ParameterEvent {
            frame: (t - start) as u32,
            index,
            value,
        })
        .collect();
        let mut out = vec![0.0; frames * 2];
        plugin.process(&[], &mut out, &midi, &parameters, frames as u32, 0, 2);
        result.extend(out);
    }
    result
}

#[test]
fn block_size_does_not_change_sound_or_event_timing() {
    for note in [57, 100] {
        let reference = render(4096, note);
        assert!(reference[..34].iter().all(|v| *v == 0.0));
        assert!(reference.iter().any(|v| v.abs() > 1e-5));
        for block in [1, 64, 127, 128, 256, 512] {
            assert_eq!(render(block, note), reference);
        }
    }
}

#[test]
fn state_is_versioned_and_rejected_atomically() {
    let mut plugin = prepared();
    plugin.set_parameter(0, 0.4);
    plugin.set_parameter(1, 1.0);
    plugin.set_parameter(3, 1.0);
    let mut state = [0; STATE_BYTES];
    assert_eq!(plugin.save_state(&mut state), Some(STATE_BYTES));
    plugin.set_parameter(0, 0.8);
    assert!(plugin.load_state(&state));
    assert_eq!(plugin.get_parameter(0), Some(0.4));
    assert_eq!(plugin.get_parameter(1), Some(1.0));
    assert_eq!(plugin.get_parameter(3), Some(1.0));
    for len in 0..STATE_BYTES {
        assert!(!plugin.load_state(&state[..len]));
    }
    state[8..16].copy_from_slice(&f64::NAN.to_le_bytes());
    assert!(!plugin.load_state(&state));
    assert_eq!(plugin.get_parameter(0), Some(0.4));
    state[4] = 5;
    assert!(!plugin.load_state(&state));
    assert!(!plugin.load_preset("unknown"));
}

#[test]
fn malformed_blocks_are_silent_and_do_not_apply_partial_edits() {
    let mut plugin = prepared();
    let mut out = [1.0; 128];
    let parameters = [
        ParameterEvent {
            frame: 0,
            index: 0,
            value: 0.1,
        },
        ParameterEvent {
            frame: 65,
            index: 0,
            value: 0.5,
        },
    ];
    plugin.process(&[], &mut out, &[], &parameters, 64, 0, 2);
    assert_eq!(out, [0.0; 128]);
    assert_eq!(plugin.get_parameter(0), Some(DEFAULT_GAIN));
    plugin.process(
        &[],
        &mut out,
        &[MidiEvent {
            frame: 0,
            data: [0x90, 57, 255],
            length: 3,
        }],
        &[],
        64,
        0,
        2,
    );
    assert_eq!(out, [0.0; 128]);
    assert!(!plugin.prepare(f64::NAN, 128, 0, 2));
}

#[test]
fn upscaled_midi1_matches_original_and_native_wide_velocity_is_preserved() {
    let mut a = prepared();
    let mut b = prepared();
    let mut out_a = [0.0; 8192];
    let mut out_b = [0.0; 8192];
    a.process(
        &[],
        &mut out_a,
        &[MidiEvent {
            frame: 0,
            data: [0x90, 57, 90],
            length: 3,
        }],
        &[],
        4096,
        0,
        2,
    );
    let wide = MidiEvent2 {
        frame: 0,
        kind: MIDI2_KIND_NOTE_ON,
        channel: 0,
        index: 57,
        flags: MIDI2_FLAG_ORIGIN_7BIT,
        value: 90 << 9,
        extra: 0,
    };
    b.process_wide(&[], &mut out_b, &[], &[wide], &[], 4096, 0, 2);
    assert_eq!(out_a, out_b);
    let mut c = prepared();
    let mut d = prepared();
    c.process_wide(
        &[],
        &mut out_a,
        &[],
        &[MidiEvent2 {
            flags: 0,
            value: 40_000,
            ..wide
        }],
        &[],
        4096,
        0,
        2,
    );
    d.process_wide(
        &[],
        &mut out_b,
        &[],
        &[MidiEvent2 {
            flags: 0,
            value: 40_001,
            ..wide
        }],
        &[],
        4096,
        0,
        2,
    );
    assert_ne!(out_a, out_b);
}

#[test]
fn midi1_and_midi2_pitch_bend_share_endpoints_and_exact_center() {
    let note = MidiEvent {
        frame: 0,
        data: [0x90, 57, 90],
        length: 3,
    };
    let bend = MidiEvent {
        frame: 512,
        data: [0xe0, 127, 127],
        length: 3,
    };
    let wide_note = MidiEvent2 {
        frame: 0,
        kind: MIDI2_KIND_NOTE_ON,
        channel: 0,
        index: 57,
        flags: MIDI2_FLAG_ORIGIN_7BIT,
        value: 90 << 9,
        extra: 0,
    };
    let wide_bend = MidiEvent2 {
        frame: 512,
        kind: MIDI2_KIND_PITCH_BEND,
        channel: 0,
        index: 0,
        flags: 0,
        value: u32::MAX,
        extra: 0,
    };
    let mut midi1 = prepared();
    let mut midi2 = prepared();
    let mut out_midi1 = [0.0; 8192];
    let mut out_midi2 = [0.0; 8192];
    midi1.process(&[], &mut out_midi1, &[note, bend], &[], 4096, 0, 2);
    midi2.process_wide(
        &[],
        &mut out_midi2,
        &[],
        &[wide_note, wide_bend],
        &[],
        4096,
        0,
        2,
    );
    assert_eq!(out_midi1, out_midi2);
    assert!(out_midi1.iter().any(|sample| sample.abs() > 1e-5));

    let mut plain = prepared();
    let mut centered = prepared();
    plain.process(&[], &mut out_midi1, &[note], &[], 4096, 0, 2);
    centered.process(
        &[],
        &mut out_midi2,
        &[
            note,
            MidiEvent {
                frame: 512,
                data: [0xe0, 0, 64],
                length: 3,
            },
        ],
        &[],
        4096,
        0,
        2,
    );
    assert_eq!(out_midi1, out_midi2);
}
