use serde_json::{Value, json};

pub const PROTOCOL: &str = "rackforge.plugin.web@1";
/// The status once a parameter exchange has succeeded.
const CONNECTED: &str = "Connected to RackForge";
/// Gain, pickup law, distance, alignment, hardness, sustain, bell, dynamics,
/// the seven panel electronics, then hammer mass, tine mass, pole radius and
/// axis twist.
pub const PARAMETERS: usize = 19;
/// What the panel shows before the host answers, which has to be the program
/// the plugin opens on. These were still the retired decade voicing after the
/// catalog moved; they are now Portable Bark 1972, the current default.
pub const DEFAULTS: [f64; PARAMETERS] = [
    0.1, 2.0, 0.6, 0.45, 0.45, 0.5, 0.26, 0.5, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.85, 0.46, 0.52,
    0.62, 0.18,
];

#[derive(Clone, Debug, PartialEq)]
pub enum Operation {
    Fetch,
    Set(usize, f64),
}

/// A program as the panel shows it. RackForge's program selector names and
/// chooses programs; the panel follows the host's `selected_sound_id`.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub id: String,
    pub detail: String,
}

pub struct Client {
    pub sounds: Vec<Sound>,
    pub selected: String,
    refresh: bool,
    revision: u64,
    pending_revision: u64,
    pub values: [f64; PARAMETERS],
    pub loaded: bool,
    pub status: String,
    queued: [Option<f64>; PARAMETERS],
    pending: Option<(String, Operation, f64)>,
    serial: u64,
}

impl Default for Client {
    fn default() -> Self {
        Self {
            sounds: Vec::new(),
            selected: String::new(),
            refresh: false,
            revision: 0,
            pending_revision: 0,
            values: DEFAULTS,
            loaded: false,
            status: "Connecting to RackForge...".into(),
            queued: [None; PARAMETERS],
            pending: None,
            serial: 0,
        }
    }
}

pub fn valid(index: usize, value: f64) -> bool {
    value.is_finite()
        && match index {
            0 => (0.0..=2.0).contains(&value),
            1 => [0.0, 1.0, 2.0].contains(&value),
            2 => (0.5..=3.0).contains(&value),
            3 => (-1.0..=1.5).contains(&value),
            8 | 9 => (-12.0..=12.0).contains(&value),
            10 | 13 => [0.0, 1.0].contains(&value),
            11 => (0.5..=12.0).contains(&value),
            12 | 14 => (0.0..=1.0).contains(&value),
            // Unit controls: the four voicing ones, and the four mechanical
            // ones the panel gained with the second motion coordinate.
            4..=7 | 15..=18 => (0.0..=1.0).contains(&value),
            _ => false,
        }
}

impl Client {
    pub fn context(&mut self, instance: &Value) {
        self.sounds = instance["sounds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|sound| {
                Some(Sound {
                    id: sound["id"].as_str()?.to_owned(),
                    detail: sound["detail"].as_str().unwrap_or("").to_owned(),
                })
            })
            .collect();
        let selected = instance["selected_sound_id"].as_str().unwrap_or("");
        if selected != self.selected {
            self.selected = selected.to_owned();
            self.revision = self.revision.wrapping_add(1);
            self.queued.fill(None);
            self.loaded = false;
            self.refresh = true;
        }
    }

    pub fn queue(&mut self, index: usize, value: f64) {
        if self.loaded && valid(index, value) {
            self.queued[index] = Some(value);
        }
    }

    pub fn next(&mut self, now: f64, poll: bool) -> Option<Value> {
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, _, sent)| now - sent > 5000.0)
        {
            self.pending = None;
            self.refresh = true;
            self.queued.fill(None);
            self.loaded = false;
            self.status = "Host timed out. Reconnecting...".into();
        }
        if self.pending.is_some() {
            return None;
        }
        let operation = if self.refresh {
            self.refresh = false;
            Operation::Fetch
        } else if let Some(index) = self.queued.iter().position(Option::is_some) {
            Operation::Set(index, self.queued[index].take().expect("queued value"))
        } else if poll {
            Operation::Fetch
        } else {
            return None;
        };
        self.serial = self.serial.wrapping_add(1);
        let id = format!("rf-tines-ui-{}", self.serial);
        self.pending_revision = self.revision;
        self.pending = Some((id.clone(), operation.clone(), now));
        let (method, params) = match operation {
            Operation::Fetch => ("plugin.parameters", json!({})),
            Operation::Set(index, value) => (
                "plugin.set_parameter",
                json!({"parameter_index": index, "value": value}),
            ),
        };
        Some(
            json!({"protocol": PROTOCOL, "kind": "request", "request_id": id, "method": method, "params": params}),
        )
    }

    pub fn response(&mut self, message: &Value) {
        let Some((id, operation, _)) = &self.pending else {
            return;
        };
        if message["request_id"].as_str() != Some(id) {
            return;
        }
        let operation = operation.clone();
        self.pending = None;
        // A host-side program change invalidates any earlier parameter reply.
        if self.pending_revision != self.revision {
            self.refresh = true;
            return;
        }
        if message["ok"].as_bool() != Some(true) {
            self.queued.fill(None);
            self.loaded = false;
            self.status = "Host rejected the request. Reconnecting...".into();
            return;
        }
        let updated = match operation {
            Operation::Set(index, _) => message["result"]["value"]
                .as_f64()
                .filter(|value| valid(index, *value))
                .map(|value| {
                    let mut values = self.values;
                    values[index] = value;
                    values
                }),
            Operation::Fetch => snapshot(&message["result"]),
        };
        if let Some(values) = updated {
            self.values = values;
            self.loaded = true;
            self.status = CONNECTED.into();
        } else {
            self.loaded = false;
            self.queued.fill(None);
            self.status = "Invalid host parameter response. Reconnecting...".into();
        }
    }

    /// The link to the host is sound: the last exchange succeeded. A program
    /// change reloads the controls without breaking it, so the panel has
    /// nothing to say about the connection meanwhile.
    pub fn linked(&self) -> bool {
        self.status == CONNECTED
    }

    pub fn display(&self, index: usize) -> f64 {
        self.queued[index]
            .or_else(|| match self.pending.as_ref().map(|(_, op, _)| op) {
                Some(Operation::Set(i, value)) if *i == index => Some(*value),
                _ => None,
            })
            .unwrap_or(self.values[index])
    }
}

fn snapshot(result: &Value) -> Option<[f64; PARAMETERS]> {
    let mut values = [None; PARAMETERS];
    for entry in result["values"].as_array()? {
        let index = usize::try_from(entry["index"].as_u64()?).ok()?;
        if index >= PARAMETERS {
            continue;
        }
        let value = entry["value"].as_f64()?;
        if !valid(index, value) || values[index].replace(value).is_some() {
            return None;
        }
    }
    let mut complete = [0.0; PARAMETERS];
    for (slot, value) in complete.iter_mut().zip(values) {
        *slot = value?;
    }
    Some(complete)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reply(id: &Value, result: Value) -> Value {
        json!({"request_id": id["request_id"], "ok": true, "result": result})
    }
    fn connect(client: &mut Client) {
        let request = client.next(0.0, true).unwrap();
        let values: Vec<Value> = DEFAULTS
            .iter()
            .enumerate()
            .map(|(i, v)| json!({"index": i, "value": v}))
            .collect();
        client.response(&reply(&request, json!({"values": values})));
        assert!(client.loaded);
    }
    #[test]
    fn edits_coalesce_and_serialize_without_stale_acknowledgements_overwriting_them() {
        let mut client = Client::default();
        connect(&mut client);
        client.queue(0, 0.2);
        let first = client.next(1.0, false).unwrap();
        client.queue(0, 0.3);
        client.queue(0, 0.4);
        client.queue(5, 1.0);
        assert!(client.next(2.0, true).is_none());
        client.response(&reply(&first, json!({"value": 0.2})));
        assert_eq!(client.display(0), 0.4);
        let second = client.next(3.0, false).unwrap();
        assert_eq!(second["params"]["value"], 0.4);
        client.response(&reply(&first, json!({"value": 0.2})));
        assert!(client.next(4.0, false).is_none());
        client.response(&reply(&second, json!({"value": 0.4})));
        assert_eq!(
            client.next(5.0, false).unwrap()["params"]["parameter_index"],
            5
        );
    }
    #[test]
    fn timeout_discards_ambiguous_writes_and_recovers_through_a_fresh_snapshot() {
        let mut client = Client::default();
        connect(&mut client);
        client.queue(3, 1.0);
        let old = client.next(1.0, false).unwrap();
        client.queue(1, 1.0);
        let fresh = client.next(6000.0, true).unwrap();
        assert_eq!(fresh["method"], "plugin.parameters");
        assert!(!client.loaded);
        client.response(&reply(&old, json!({"value": 1.0})));
        assert!(!client.loaded);
        client.response(&reply(&fresh, json!({"values":[]})));
        assert!(!client.loaded);
    }
    #[test]
    fn packaged_html_contains_the_required_program_and_parameter_controls() {
        let html = include_str!("../../../package/web/play.html");
        for id in [
            "program-selector",
            "program-detail",
            "program-error",
            "save-program-open",
            "save-program-dialog",
            "save-program-form",
            "save-program-name",
            "save-program-submit",
            "save-program-feedback",
            "status",
            "gain",
            "gain-number",
            "gain-db",
            "law",
            "law-value",
            "preamp",
            "preamp-value",
            "vibrato",
            "vibrato-value",
            "stage-controls",
            "console-controls",
            "tab-instrument",
            "tab-setup",
        ] {
            assert_eq!(
                html.matches(&format!("id=\"{id}\"")).count(),
                1,
                "missing or duplicate {id}"
            );
        }
        for id in [
            "hardness",
            "dynamics",
            "distance",
            "alignment",
            "sustain",
            "bell",
        ] {
            for suffix in ["", "-number", "-knob", "-value"] {
                assert_eq!(html.matches(&format!("id=\"{id}{suffix}\"")).count(), 1);
            }
        }
        assert!(html.contains("src=\"play-programs.mjs\""));
        // RackForge's selector chooses programs; the panel keeps no list of its own.
        assert!(html.contains("<rf-program-select"));
        for retired in [
            "program-list",
            "program-select",
            "program-prev",
            "program-next",
        ] {
            assert!(
                !html.contains(&format!("id=\"{retired}\"")),
                "{retired} is back"
            );
        }
    }
    #[test]
    fn packaged_config_surface_contains_the_portable_program_workflow() {
        let html = include_str!("../../../package/web/config.html");
        for id in [
            "connection",
            "export-source",
            "export-button",
            "import-file",
            "import-button",
            "activity",
            "error",
        ] {
            assert_eq!(
                html.matches(&format!("id=\"{id}\"")).count(),
                1,
                "missing or duplicate {id}"
            );
        }
        assert!(html.contains("accept=\".rftines,application/json\""));
        assert!(html.contains("id=\"recover-button\""));
        assert!(html.contains("src=\"config.mjs\""));
        assert!(!html.contains("id=\"save-name\""));
        let manifest = include_str!("../../../package/rackforge-plugin.toml");
        assert!(manifest.contains("config_mode = true"));
        assert!(manifest.contains("kind = \"config\"\nentry = \"web/config.html\""));
    }
    fn catalog(client: &mut Client, selected: &str) {
        client.context(&json!({"selected_sound_id": selected, "sounds": [
            {"id":"a", "name":"First"}, {"id":"b", "name":"Second", "detail":"Bright"}
        ]}));
    }
    /// A program change reloads the controls, but the link stays sound, so the
    /// panel has no connection message to show for it.
    #[test]
    fn a_program_change_keeps_the_link_sound_while_the_controls_reload() {
        let mut client = Client::default();
        assert!(!client.linked(), "not linked before the first exchange");
        catalog(&mut client, "a");
        connect(&mut client);
        assert!(client.linked());
        catalog(&mut client, "b");
        assert!(!client.loaded);
        assert!(client.linked());
        client.next(1.0, false).unwrap();
        client.next(6_001.0, false);
        assert!(!client.linked(), "a timeout breaks the link");
    }

    #[test]
    fn a_program_chosen_elsewhere_drops_queued_edits_and_reads_the_new_controls() {
        let mut client = Client::default();
        catalog(&mut client, "a");
        connect(&mut client);
        client.queue(0, 0.2);
        let write = client.next(1.0, false).unwrap();
        client.queue(5, 0.9);
        catalog(&mut client, "b");
        assert!(!client.loaded);
        client.queue(4, 0.8);
        client.response(&reply(&write, json!({"value":0.2})));
        let fetch = client.next(2.0, false).unwrap();
        assert_eq!(fetch["method"], "plugin.parameters");
        assert_eq!(client.selected, "b");
        assert_eq!(client.sounds[1].detail, "Bright");
    }
    #[test]
    fn external_program_change_invalidates_an_inflight_parameter_snapshot() {
        let mut client = Client::default();
        catalog(&mut client, "a");
        connect(&mut client);
        let old = client.next(1.0, true).unwrap();
        catalog(&mut client, "b");
        client.response(&reply(
            &old,
            json!({"values": DEFAULTS.iter().enumerate()
            .map(|(i,v)| json!({"index":i,"value":v})).collect::<Vec<_>>()}),
        ));
        assert!(!client.loaded);
        assert_eq!(client.selected, "b");
        assert_eq!(
            client.next(2.0, false).unwrap()["method"],
            "plugin.parameters"
        );
    }
    #[test]
    fn snapshots_require_all_parameters_with_valid_domains_and_no_duplicates() {
        assert!(snapshot(&json!({"values":[{"index":1,"value":0.5}]})).is_none());
        assert!(!valid(1, 0.5));
        assert!(valid(1, 2.0));
        assert!(!valid(1, 3.0));
        assert!(!valid(0, f64::NAN));
        assert!(!valid(19, 0.0));
        assert!(valid(15, 0.0) && valid(18, 1.0) && !valid(16, 1.1));
        assert!(valid(2, 0.5) && !valid(2, 0.4) && valid(3, -1.0) && !valid(3, 1.6));
        assert!(valid(7, 1.0) && !valid(4, 1.5));
        let mut client = Client::default();
        client.queue(0, 0.9);
        assert!(client.next(0.0, false).is_none());
    }
}
