//! Control-thread JSON only. Audio rendering and parameter automation never serialize.
use crate::{RfTinesProcessor, STATE_VERSION, Settings};
use rackforge_program_api::{
    PreparedProgram, ProgramDocument, ProgramEditRequest, ProgramEditorValue, ProgramEditorView,
    ProgramFieldEditRequest,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;
use std::collections::BTreeMap;

const PLUGIN_ID: &str = "org.rackforge.rftines";
const MAX_PROGRAMS: usize = 8;
const MAX_TRANSFER: usize = 16384;

fn read<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    if bytes.len() > MAX_TRANSFER {
        return None;
    }
    serde_json::from_slice(bytes).ok()
}

fn write<T: Serialize>(value: &T, destination: &mut [u8]) -> Option<usize> {
    let bytes = serde_json::to_vec(value).ok()?;
    if bytes.len() > MAX_TRANSFER {
        return None;
    }
    destination.get_mut(..bytes.len())?.copy_from_slice(&bytes);
    Some(bytes.len())
}

pub fn settings(document: &ProgramDocument) -> Option<Settings> {
    document.validate().ok()?;
    if document.plugin_id != PLUGIN_ID
        || ![4, STATE_VERSION].contains(&document.plugin_state_version)
        || document.payload_version != 1
        || document.id.len() > 64
        || document.name.len() > 64
    {
        return None;
    }
    let value: Settings = serde_json::from_value(document.payload.clone()).ok()?;
    value.valid().then_some(value)
}

fn envelope(document: ProgramDocument) -> Option<PreparedProgram> {
    settings(&document)?;
    let prepared = PreparedProgram {
        schema_version: 1,
        storage_path: format!("programs/{}.rackforge-program.json", document.id),
        preview_sound_id: format!("custom.{}", document.id),
        document,
        artifacts: Vec::new(),
    };
    prepared.validate().ok()?;
    Some(prepared)
}

pub fn validated_prepared(bytes: &[u8]) -> Option<ProgramDocument> {
    let prepared: PreparedProgram = read(bytes)?;
    prepared.validate().ok()?;
    let expected = envelope(prepared.document.clone())?;
    (prepared == expected).then_some(prepared.document)
}

pub fn begin(plugin: &RfTinesProcessor, request: &[u8], destination: &mut [u8]) -> Option<usize> {
    let request: ProgramEditRequest = read(request)?;
    request.validate().ok()?;
    if let Some(id) = request
        .program_id
        .as_deref()
        .and_then(|id| id.strip_prefix("custom."))
    {
        return write(&envelope(plugin.programs.get(id)?.clone())?, destination);
    }
    let factory = request
        .program_id
        .as_deref()
        .map(|id| crate::settings::presets().into_iter().find(|p| p.0 == id));
    let base = match factory {
        None => plugin.settings,
        Some(Some(preset)) => preset.3,
        Some(None) => return None,
    };
    // A read-only export still needs a draft envelope, even when the user
    // library is full. Keep one extra temporary identity available; install()
    // remains the authority that refuses a ninth persisted program.
    let id = (1..=MAX_PROGRAMS + 1)
        .map(|i| format!("lab-{i}"))
        .find(|id| !plugin.programs.contains_key(id))?;
    let document = ProgramDocument {
        schema_version: 1,
        id,
        name: "Voicing".into(),
        plugin_id: PLUGIN_ID.into(),
        plugin_version: env!("CARGO_PKG_VERSION").into(),
        plugin_state_version: STATE_VERSION,
        payload_version: 1,
        category: Some("Electric Piano".into()),
        tags: vec!["uncalibrated".into()],
        payload: serde_json::to_value(base).ok()?,
    };
    write(&envelope(document)?, destination)
}

pub fn prepare(bytes: &[u8], destination: &mut [u8]) -> Option<usize> {
    write(&envelope(read(bytes)?)?, destination)
}

pub fn install(plugin: &mut RfTinesProcessor, bytes: &[u8]) -> bool {
    let Some(document) = validated_prepared(bytes) else {
        return false;
    };
    if !plugin.programs.contains_key(&document.id) && plugin.programs.len() >= MAX_PROGRAMS {
        return false;
    }
    let mut programs = plugin.programs.clone();
    programs.insert(document.id.clone(), document);
    if catalog(&programs, &mut [0; MAX_TRANSFER]).is_none() {
        return false;
    }
    plugin.programs = programs;
    true
}

pub fn catalog(
    programs: &BTreeMap<String, ProgramDocument>,
    destination: &mut [u8],
) -> Option<usize> {
    let mut catalog: serde_json::Value =
        serde_json::from_str(include_str!("../../../package/metadata/presets.json")).ok()?;
    let entries = catalog["presets"].as_array_mut()?;
    let factory = entries.len();
    for (i, document) in programs.values().enumerate() {
        entries.push(
            json!({"id": format!("custom.{}", document.id), "name": document.name,
            "bank": "user", "category": "Electric Piano", "order": factory + i,
            "tags": ["custom", "uncalibrated"], "editable": true,
            "description": "Saved voicing. Level compensated pickup; no limiter."}),
        );
    }
    write(&catalog, destination)
}

/// Editor fields carry integers: hundredths of a millimetre for the distances,
/// thousandths for the unit controls and millionths for the gain.
type Field = (
    &'static str,
    &'static str,
    &'static str,
    u32,
    f64,
    i64,
    i64,
    u32,
    &'static str,
);
const FIELDS: [Field; 19] = [
    (
        "distance",
        "Pickup Distance",
        "Gap 0.50..3.00 mm; compensated level.",
        2,
        100.0,
        50,
        300,
        2,
        "mm",
    ),
    (
        "alignment",
        "Tine Alignment",
        "Offset -1.00..1.50 mm; centre is hollow.",
        3,
        100.0,
        -100,
        150,
        2,
        "mm",
    ),
    (
        "hardness",
        "Hammer Hardness",
        "Contact stiffness, 0.500 is the original.",
        4,
        1000.0,
        0,
        1000,
        3,
        "",
    ),
    (
        "sustain",
        "Sustain",
        "Changes tine sustain and tonebar decay together.",
        5,
        1000.0,
        0,
        1000,
        3,
        "",
    ),
    (
        "bell",
        "Bell",
        "Strike excitation of the second bending mode.",
        6,
        1000.0,
        0,
        1000,
        3,
        "",
    ),
    (
        "dynamics",
        "Dynamics",
        "Velocity curve, 0.5 is the original 1.4 power.",
        7,
        1000.0,
        0,
        1000,
        3,
        "",
    ),
    (
        "gain",
        "Output Gain",
        "Start at 0.100x. Watch host meters; no limiter.",
        0,
        1_000_000.0,
        0,
        2_000_000,
        6,
        "x",
    ),
    (
        "law",
        "Pickup Model",
        "Original, aperture, or register-shaped aperture.",
        1,
        1.0,
        0,
        2,
        0,
        "",
    ),
    (
        "bass",
        "Bass",
        "Instrument electronics.",
        8,
        1000.0,
        -12000,
        12000,
        3,
        "dB",
    ),
    (
        "treble",
        "Treble",
        "Instrument electronics.",
        9,
        1000.0,
        -12000,
        12000,
        3,
        "dB",
    ),
    (
        "vibrato",
        "Vibrato",
        "Instrument electronics.",
        10,
        1.0,
        0,
        1,
        0,
        "",
    ),
    (
        "speed",
        "Speed",
        "Instrument electronics.",
        11,
        1000.0,
        500,
        12000,
        3,
        "Hz",
    ),
    (
        "intensity",
        "Intensity",
        "Instrument electronics.",
        12,
        1000.0,
        0,
        1000,
        3,
        "",
    ),
    (
        "preamp",
        "Panel",
        "Instrument electronics.",
        13,
        1.0,
        0,
        1,
        0,
        "",
    ),
    (
        "bass-boost",
        "Bass Boost",
        "Instrument electronics.",
        14,
        1000.0,
        0,
        1000,
        3,
        "",
    ),
    (
        "hammer",
        "Hammer Mass",
        "Mass of the hammer; 0.500 is the nominal four grams.",
        15,
        1000.0,
        0,
        1000,
        3,
        "",
    ),
    (
        "tine",
        "Tine Mass",
        "Mass the tine presents to the hammer; 0.500 is nominal.",
        16,
        1000.0,
        0,
        1000,
        3,
        "",
    ),
    (
        "pole",
        "Pole Radius",
        "Pickup pole face, 0.50..3.00 mm; 0.600 is the original 2 mm.",
        17,
        1000.0,
        0,
        1000,
        3,
        "",
    ),
    (
        "twist",
        "Axis Twist",
        "Turns the tine's bending axes off the strike; zero keeps it on a line.",
        18,
        1000.0,
        0,
        1000,
        3,
        "",
    ),
];

pub fn view(bytes: &[u8], destination: &mut [u8]) -> Option<usize> {
    let document: ProgramDocument = read(bytes)?;
    let settings = settings(&document)?;
    let laws: Vec<_> = crate::settings::LAW_NAMES
        .iter()
        .enumerate()
        .map(|(i, name)| json!({"value": i.to_string(), "label": name}))
        .collect();
    let mut fields = Vec::new();
    for (id, label, detail, index, scale, minimum, maximum, decimals, unit) in FIELDS {
        let value = settings.parameter(index)?;
        fields.push(if matches!(id, "law" | "preamp" | "vibrato") {
            let options = match id {
                "law" => laws.clone(),
                "preamp" => vec![
                    json!({"value":"0","label":"Passive"}),
                    json!({"value":"1","label":"Console"}),
                ],
                _ => vec![
                    json!({"value":"0","label":"Off"}),
                    json!({"value":"1","label":"On"}),
                ],
            };
            json!({"id": id, "label": label, "detail": detail,
                "value": {"type": "choice", "value": (value as u8).to_string()},
                "kind": {"type": "choice", "options": options}, "live_preview": true})
        } else {
            let mut kind = json!({"type": "number", "minimum": minimum, "maximum": maximum,
                "step": if decimals == 6 { 10_000 } else { 1 }, "decimals": decimals});
            if !unit.is_empty() {
                kind["unit"] = json!(unit);
            }
            json!({"id": id, "label": label, "detail": detail,
                "value": {"type": "integer", "value": (value * scale).round() as i64},
                "kind": kind, "live_preview": true})
        });
    }
    let groups = [
        (
            "hammer",
            "Hammer",
            "Contact and playing response.",
            vec!["hardness"],
        ),
        (
            "resonator",
            "Tine & Tonebar",
            "Decay and bending-mode excitation.",
            vec!["sustain", "bell"],
        ),
        (
            "pickup",
            "Pickup",
            "Magnetic pickup position and response.",
            vec!["distance", "alignment", "law"],
        ),
        (
            "instrument",
            "Instrument",
            "Player controls.",
            vec![
                "gain",
                "preamp",
                "bass-boost",
                "bass",
                "treble",
                "vibrato",
                "speed",
                "intensity",
            ],
        ),
        ("setup", "Setup", "Controller response.", vec!["dynamics"]),
    ];
    let pages: Vec<_> = [3usize, 0, 1, 2, 4]
        .into_iter()
        .map(|index| groups[index].clone())
        .map(|(id, label, detail, ids)| {
            let grouped: Vec<_> = ids
                .iter()
                .map(|id| {
                    fields
                        .iter()
                        .find(|f| f["id"] == *id)
                        .expect("known editor field")
                        .clone()
                })
                .collect();
            json!({"id":id,"label":label,"detail":detail,"fields":grouped})
        })
        .collect();
    let view: ProgramEditorView = serde_json::from_value(json!({
        "schema_version": 1, "title": "RF-Tines Voicing",
        "pages": pages
    }))
    .ok()?;
    view.validate().ok()?;
    write(&view, destination)
}

pub fn edit(bytes: &[u8], destination: &mut [u8]) -> Option<usize> {
    let request: ProgramFieldEditRequest = read(bytes)?;
    request.validate().ok()?;
    let current = settings(&request.document)?;
    let field = FIELDS.iter().find(|f| f.0 == request.field_id)?;
    let (index, value) = match (&request.value, field.0) {
        (ProgramEditorValue::Choice(value), "law" | "preamp" | "vibrato") => {
            (field.3, value.parse::<u8>().ok()? as f64)
        }
        (ProgramEditorValue::Integer(value), id) if !matches!(id, "law" | "preamp" | "vibrato") => {
            if !(field.5..=field.6).contains(value) {
                return None;
            }
            (field.3, *value as f64 / field.4)
        }
        _ => return None,
    };
    let updated = current.with_parameter(index, value)?;
    let mut document = request.document;
    document.plugin_state_version = STATE_VERSION;
    document.payload = serde_json::to_value(updated).ok()?;
    write(&envelope(document)?, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_library_can_still_open_a_temporary_export_draft() {
        let request = serde_json::to_vec(&ProgramEditRequest::new(None)).unwrap();
        let mut buffer = [0; MAX_TRANSFER];
        let mut plugin = RfTinesProcessor::default();
        let length = begin(&plugin, &request, &mut buffer).unwrap();
        let template: PreparedProgram = serde_json::from_slice(&buffer[..length]).unwrap();
        for index in 1..=MAX_PROGRAMS {
            let mut document = template.document.clone();
            document.id = format!("lab-{index}");
            plugin.programs.insert(document.id.clone(), document);
        }

        let length = begin(&plugin, &request, &mut buffer).unwrap();
        let draft: PreparedProgram = serde_json::from_slice(&buffer[..length]).unwrap();
        assert_eq!(draft.document.id, "lab-9");
        assert!(!install(&mut plugin, &serde_json::to_vec(&draft).unwrap()));
    }
}
