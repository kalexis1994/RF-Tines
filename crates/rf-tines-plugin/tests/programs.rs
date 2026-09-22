use rackforge_plugin_sdk::{MidiEvent, ParameterEvent, Processor};
use rackforge_program_api::{
    PreparedProgram, ProgramEditRequest, ProgramEditorValue, ProgramEditorView,
    ProgramFieldEditRequest,
};
use rf_tines_plugin::{RfTinesProcessor, STATE_BYTES, Settings, presets};

fn begin(plugin: &mut RfTinesProcessor, id: Option<&str>) -> PreparedProgram {
    let request = ProgramEditRequest {
        schema_version: 1,
        program_id: id.map(str::to_string),
    };
    let mut destination = [0; 16384];
    let len = plugin
        .begin_program_edit(&serde_json::to_vec(&request).unwrap(), &mut destination)
        .unwrap();
    serde_json::from_slice(&destination[..len]).unwrap()
}

fn state(plugin: &RfTinesProcessor) -> [u8; STATE_BYTES] {
    let mut bytes = [0; STATE_BYTES];
    assert_eq!(plugin.save_state(&mut bytes), Some(STATE_BYTES));
    bytes
}

#[test]
fn editor_preview_install_catalog_reload_and_snapshot_agree() {
    let mut plugin = RfTinesProcessor::default();
    let mut draft = begin(&mut plugin, Some("research-direct"));
    let mut destination = [0; 16384];
    for (field, value, parameter, expected) in [
        ("law", ProgramEditorValue::Choice("1".into()), 1, 1.0),
        ("distance", ProgramEditorValue::Integer(75), 2, 0.75),
        ("alignment", ProgramEditorValue::Integer(-25), 3, -0.25),
        ("hardness", ProgramEditorValue::Integer(300), 4, 0.3),
        ("sustain", ProgramEditorValue::Integer(500), 5, 0.5),
        ("bell", ProgramEditorValue::Integer(258), 6, 0.258),
        ("dynamics", ProgramEditorValue::Integer(1000), 7, 1.0),
        ("gain", ProgramEditorValue::Integer(123456), 0, 0.123456),
    ] {
        let before = state(&plugin);
        let request = ProgramFieldEditRequest {
            schema_version: 1,
            document: draft.document,
            field_id: field.into(),
            value,
        };
        let bytes = serde_json::to_vec(&request).unwrap();
        assert!(plugin.apply_program_edit(&bytes, &mut [0; 1]).is_none());
        let len = plugin.apply_program_edit(&bytes, &mut destination).unwrap();
        assert_eq!(
            state(&plugin),
            before,
            "editing a draft must not change audio"
        );
        draft = serde_json::from_slice(&destination[..len]).unwrap();
        assert!(plugin.preview_program(&destination[..len]));
        assert_eq!(plugin.get_parameter(parameter), Some(expected));
    }
    let document = serde_json::to_vec(&draft.document).unwrap();
    let len = plugin
        .program_editor_view(&document, &mut destination)
        .unwrap();
    let view: ProgramEditorView = serde_json::from_slice(&destination[..len]).unwrap();
    view.validate().unwrap();
    assert_eq!(view.pages.len(), 5);
    assert_eq!(
        view.pages
            .iter()
            .map(|p| p.fields.len())
            .collect::<Vec<_>>(),
        vec![8, 1, 2, 3, 1]
    );
    assert_eq!(view.pages.iter().flat_map(|p| p.fields.iter()).count(), 15);
    assert!(
        view.pages
            .iter()
            .flat_map(|p| p.fields.iter())
            .all(|field| field.live_preview)
    );
    let len = plugin
        .prepare_program_save(&document, &mut destination)
        .unwrap();
    let prepared = destination[..len].to_vec();
    assert!(plugin.install_program(&prepared));
    let saved = state(&plugin);
    assert!(plugin.load_preset("research-direct"));
    assert!(plugin.load_preset(&draft.preview_sound_id));
    assert_eq!(state(&plugin), saved);
    assert_eq!(begin(&mut plugin, Some(&draft.preview_sound_id)), draft);
    let len = plugin.write_program_catalog(&mut destination).unwrap();
    let catalog: serde_json::Value = serde_json::from_slice(&destination[..len]).unwrap();
    assert_eq!(
        catalog["presets"].as_array().unwrap().len(),
        visible_factory_count() + 1
    );
    assert_eq!(
        catalog["presets"][visible_factory_count()]["id"],
        draft.preview_sound_id
    );
    // The host replays stored program documents into fresh instances.
    let mut restored = RfTinesProcessor::default();
    assert!(restored.install_program(&prepared));
    assert!(restored.load_preset(&draft.preview_sound_id));
    assert_eq!(state(&restored), saved);
    let mut snapshot = RfTinesProcessor::default();
    assert!(snapshot.load_state(&saved)); // Full settings, independent of custom catalog.
    assert_eq!(state(&snapshot), saved);
}

#[test]
fn factory_presets_load_and_seed_drafts() {
    let mut plugin = RfTinesProcessor::default();
    for (id, _, _, settings) in presets() {
        assert!(plugin.load_preset(id), "{id}");
        for index in 0..15 {
            assert_eq!(
                plugin.get_parameter(index),
                settings.parameter(index),
                "{id} {index}"
            );
        }
        let draft = begin(&mut plugin, Some(id));
        let seeded: Settings = serde_json::from_value(draft.document.payload).unwrap();
        assert_eq!(seeded, settings);
    }
    assert!(plugin.load_preset("calibrated"));
    assert_eq!(plugin.get_parameter(1), Some(1.0));
    assert_eq!(plugin.get_parameter(5), Some(0.5));
    assert!(!plugin.load_preset("unknown"));
    let request = ProgramEditRequest {
        schema_version: 1,
        program_id: Some("unknown".into()),
    };
    assert!(
        plugin
            .begin_program_edit(&serde_json::to_vec(&request).unwrap(), &mut [0; 16384])
            .is_none()
    );
}

#[test]
fn malformed_programs_and_parameter_domains_reject_atomically() {
    let mut plugin = RfTinesProcessor::default();
    let draft = begin(&mut plugin, None);
    let before = state(&plugin);
    for (index, value) in [
        (0, f64::NAN),
        (0, -0.1),
        (1, 0.5),
        (1, 3.0),
        (2, 0.4),
        (2, 3.1),
        (3, -1.1),
        (3, 1.6),
        (4, 1.5),
        (5, -0.1),
        (6, f64::INFINITY),
        (7, 1.01),
        (19, 0.0),
        (15, 1.1),
        (16, -0.1),
        (17, 1.1),
        (18, -0.1),
        (8, 12.1),
        (9, -12.1),
        (10, 0.5),
        (11, 0.0),
        (12, 1.1),
        (13, 0.5),
        (14, -0.1),
    ] {
        assert!(!plugin.set_parameter(index, value), "{index} {value}");
        assert_eq!(state(&plugin), before);
    }
    let mut malformed = draft.clone();
    malformed.storage_path = "../outside.json".into();
    assert!(!plugin.install_program(&serde_json::to_vec(&malformed).unwrap()));
    malformed = draft.clone();
    malformed.document.payload["law"] = serde_json::json!(5);
    assert!(!plugin.preview_program(&serde_json::to_vec(&malformed).unwrap()));
    malformed = draft.clone();
    malformed.document.payload["distance_mm"] = serde_json::json!(9.0);
    assert!(!plugin.preview_program(&serde_json::to_vec(&malformed).unwrap()));
    malformed = draft.clone();
    malformed.document.plugin_id = "org.example.other".into();
    assert!(!plugin.install_program(&serde_json::to_vec(&malformed).unwrap()));
    assert_eq!(state(&plugin), before);
    // An editor value outside its field range is rejected.
    let request = ProgramFieldEditRequest {
        schema_version: 1,
        document: draft.document.clone(),
        field_id: "distance".into(),
        value: ProgramEditorValue::Integer(400),
    };
    assert!(
        plugin
            .apply_program_edit(&serde_json::to_vec(&request).unwrap(), &mut [0; 16384])
            .is_none()
    );
    assert!(plugin.prepare(48000.0, 128, 0, 2));
    let mut out = [1.0; 256];
    plugin.process(
        &[],
        &mut out,
        &[MidiEvent {
            frame: 0,
            length: 3,
            data: [0x90, 55, 100],
        }],
        &[
            ParameterEvent {
                frame: 0,
                index: 0,
                value: 0.2,
            },
            ParameterEvent {
                frame: 10,
                index: 2,
                value: 4.0,
            },
        ],
        128,
        0,
        2,
    );
    assert!(out.iter().all(|x| *x == 0.0));
    assert_eq!(state(&plugin), before);
}

#[test]
fn bounded_catalog_fits_transfer_and_rejects_overflow_without_losing_entries() {
    let mut plugin = RfTinesProcessor::default();
    let mut last = None;
    for _ in 0..8 {
        let mut draft = begin(&mut plugin, None);
        draft.document.name = "X".repeat(64);
        assert!(plugin.install_program(&serde_json::to_vec(&draft).unwrap()));
        last = Some(draft);
    }
    let mut out = [0; 16384];
    let len = plugin.write_program_catalog(&mut out).unwrap();
    let catalog: serde_json::Value = serde_json::from_slice(&out[..len]).unwrap();
    assert_eq!(
        catalog["presets"].as_array().unwrap().len(),
        visible_factory_count() + 8
    );
    let mut draft = last.unwrap();
    assert!(plugin.install_program(&serde_json::to_vec(&draft).unwrap()));
    draft.document.id = "overflow".into();
    draft.storage_path = "programs/overflow.json".into();
    draft.preview_sound_id = "custom.overflow".into();
    assert!(!plugin.install_program(&serde_json::to_vec(&draft).unwrap()));
    assert_eq!(plugin.write_program_catalog(&mut [0; 16384]), Some(len));
}

#[test]
fn older_state_schemas_map_onto_the_voicing_they_were_listening_to() {
    let mut plugin = RfTinesProcessor::default();
    let mut legacy = [0; 16];
    legacy[..4].copy_from_slice(b"RFRH");
    legacy[4..8].copy_from_slice(&1u32.to_le_bytes());
    legacy[8..].copy_from_slice(&0.7f64.to_le_bytes());
    assert!(plugin.set_parameter(5, 1.0));
    assert!(plugin.load_state(&legacy));
    assert_eq!(plugin.get_parameter(0), Some(0.7));
    assert_eq!(plugin.get_parameter(5), Some(0.0));
    // Schema 3: A = Current, B = Close Aperture, listening to B, Calibrated profile.
    let mut v3 = [0; 20];
    v3[..4].copy_from_slice(b"RFRH");
    v3[4..8].copy_from_slice(&3u32.to_le_bytes());
    v3[8..16].copy_from_slice(&0.25f64.to_le_bytes());
    v3[16..20].copy_from_slice(&[0, 3, 1, 2]);
    assert!(plugin.load_state(&v3));
    assert_eq!(plugin.get_parameter(0), Some(0.25));
    assert_eq!(plugin.get_parameter(1), Some(1.0));
    assert_eq!(plugin.get_parameter(2), Some(0.5));
    assert_eq!(plugin.get_parameter(3), Some(0.5));
    assert_eq!(plugin.get_parameter(5), Some(0.5));
    assert_eq!(plugin.get_parameter(6), Some(0.2582));
    // Listening to A instead: Current geometry with the same profile.
    v3[18] = 0;
    assert!(plugin.load_state(&v3));
    assert_eq!(plugin.get_parameter(1), Some(0.0));
    assert_eq!(plugin.get_parameter(2), Some(1.5));
    // Schema 2 required the reserved byte to be zero; the point-pole path maps to
    // the close production geometry.
    let mut v2 = v3;
    v2[4..8].copy_from_slice(&2u32.to_le_bytes());
    assert!(!plugin.load_state(&v2));
    v2[19] = 0;
    v2[16] = 2;
    assert!(plugin.load_state(&v2));
    assert_eq!(plugin.get_parameter(2), Some(0.5));
    assert_eq!(plugin.get_parameter(3), Some(0.25));
    assert_eq!(plugin.get_parameter(5), Some(0.0));
    // Schema 4 rejects every corrupted field atomically.
    assert!(plugin.set_parameter(1, 1.0));
    let before = state(&plugin);
    for (index, value) in [(0, 0), (4, 9), (64, 3), (65, 1), (67, 9)] {
        let mut malformed = before;
        malformed[index] = value;
        assert!(!plugin.load_state(&malformed), "{index}");
        assert_eq!(state(&plugin), before);
    }
    let mut malformed = before;
    malformed[16..24].copy_from_slice(&4.0f64.to_le_bytes());
    assert!(!plugin.load_state(&malformed));
    malformed = before;
    malformed[48..56].copy_from_slice(&f64::NAN.to_le_bytes());
    assert!(!plugin.load_state(&malformed));
    assert_eq!(state(&plugin), before);
    let mut destination = [42; STATE_BYTES - 1];
    assert_eq!(plugin.save_state(&mut destination), None);
    assert_eq!(destination, [42; STATE_BYTES - 1]);
}

#[test]
fn voicing_changes_keep_ringing_notes_and_stay_finite() {
    let mut plugin = RfTinesProcessor::default();
    assert!(plugin.prepare(48000.0, 128, 0, 2));
    let mut out = vec![0.0f32; 256];
    plugin.process(
        &[],
        &mut out,
        &[MidiEvent {
            frame: 0,
            data: [0x90, 55, 100],
            length: 3,
        }],
        &[],
        128,
        0,
        2,
    );
    let mut previous = out[254];
    for (index, value) in [
        (1, 1.0),
        (2, 0.5),
        (3, 0.0),
        (4, 1.0),
        (5, 1.0),
        (6, 0.0),
        (7, 0.0),
        (1, 0.0),
    ] {
        plugin.process(
            &[],
            &mut out,
            &[],
            &[ParameterEvent {
                frame: 5,
                index,
                value,
            }],
            128,
            0,
            2,
        );
        assert!(out.iter().all(|s| s.is_finite()));
        assert!(out.iter().any(|s| *s != 0.0), "{index}");
        assert_eq!(plugin.get_parameter(index), Some(value));
        // The block joins the previous one without a jump larger than the signal.
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((out[0] - previous).abs() <= peak.max(1e-6), "{index}");
        previous = out[254];
    }
}

#[test]
fn version_four_custom_programs_keep_their_voicing_with_neutral_electronics() {
    let mut plugin = RfTinesProcessor::default();
    let mut draft = begin(&mut plugin, Some("calibrated-register"));
    draft.document.plugin_state_version = 4;
    for key in [
        "bass_db",
        "treble_db",
        "vibrato",
        "speed_hz",
        "intensity",
        "preamp",
        "bass_boost",
    ] {
        draft.document.payload.as_object_mut().unwrap().remove(key);
    }
    let mut bytes = [0; 16384];
    let len = plugin
        .prepare_program_save(&serde_json::to_vec(&draft.document).unwrap(), &mut bytes)
        .unwrap();
    assert!(plugin.install_program(&bytes[..len]));
    assert!(plugin.load_preset(&draft.preview_sound_id));
    assert_eq!(plugin.get_parameter(1), Some(2.0));
    assert_eq!(plugin.get_parameter(6), Some(0.2582));
    for (index, value) in [
        (8, 0.0),
        (9, 0.0),
        (10, 0.0),
        (11, 4.0),
        (12, 0.0),
        (13, 1.0),
        (14, 1.0),
    ] {
        assert_eq!(plugin.get_parameter(index), Some(value));
    }
}

fn visible_factory_count() -> usize {
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../../../package/metadata/presets.json")).unwrap();
    catalog["presets"].as_array().unwrap().len()
}

#[test]
fn catalog_retires_research_programs_but_keeps_saved_ids_loadable() {
    let mut plugin = RfTinesProcessor::default();
    let mut bytes = [0; 16384];
    let len = plugin.write_program_catalog(&mut bytes).unwrap();
    let catalog: serde_json::Value = serde_json::from_slice(&bytes[..len]).unwrap();
    let ids: Vec<_> = catalog["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    // The catalog leads with the program the plugin opens on, then runs in
    // order of the year each voicing stands for.
    assert_eq!(
        ids,
        [
            "portable-bark-1972",
            "tine-bass-1960",
            "felt-1966",
            "console-bark-1973",
            "portable-bell-1977",
            "console-bell-1978",
            "portable-chime-1980",
            "wide-dynamics-1984"
        ]
    );
    assert!(
        catalog["banks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["id"] != "research")
    );
    for (id, _, _, expected) in presets().into_iter().take(10) {
        assert!(!ids.contains(&id), "{id} should not be advertised");
        assert!(plugin.load_preset(id), "{id} should still load");
        for index in 0..15 {
            assert_eq!(plugin.get_parameter(index), expected.parameter(index));
        }
    }
}

#[test]
fn initial_sound_and_declared_defaults_match_first_catalog_program() {
    let initial = RfTinesProcessor::default();
    let mut selected = RfTinesProcessor::default();
    assert!(selected.load_preset("portable-bark-1972"));
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../package/metadata/parameters.json")).unwrap();
    for item in schema["parameters"].as_array().unwrap() {
        let index = item["index"].as_u64().unwrap() as u32;
        assert_eq!(initial.get_parameter(index), selected.get_parameter(index));
        assert_eq!(
            initial.get_parameter(index),
            item["kind"]["default"].as_f64()
        );
    }
}
