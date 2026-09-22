use crate::client::{Client, PROTOCOL};
use js_sys::{JSON, Object};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{
    Document, Element, Event, HtmlElement, HtmlInputElement, HtmlSelectElement, KeyboardEvent,
    MessageEvent, PointerEvent, Window,
};

struct App {
    window: Window,
    document: Document,
    origin: String,
    connected: bool,
    client: Client,
}
type Shared = Rc<RefCell<App>>;

const SECTIONS: [&str; 5] = ["instrument", "hammer", "resonator", "pickup", "setup"];

fn select_section(document: &Document, selected: usize) -> Result<(), JsValue> {
    for (index, id) in SECTIONS.iter().enumerate() {
        let tab = document
            .get_element_by_id(&format!("tab-{id}"))
            .expect("section tab");
        tab.set_attribute(
            "aria-selected",
            if index == selected { "true" } else { "false" },
        )?;
        tab.set_attribute("tabindex", if index == selected { "0" } else { "-1" })?;
        let panel = document.get_element_by_id(id).expect("section panel");
        if index == selected {
            panel.remove_attribute("hidden")?;
        } else {
            panel.set_attribute("hidden", "")?;
        }
    }
    Ok(())
}

fn section_events(document: &Document) -> Result<(), JsValue> {
    for (index, id) in SECTIONS.iter().enumerate() {
        let tab = document
            .get_element_by_id(&format!("tab-{id}"))
            .expect("section tab");
        let page = document.clone();
        let click = Closure::<dyn FnMut(Event)>::new(move |_| {
            let _ = select_section(&page, index);
        });
        tab.add_event_listener_with_callback("click", click.as_ref().unchecked_ref())?;
        click.forget();
        let page = document.clone();
        let key = Closure::<dyn FnMut(KeyboardEvent)>::new(move |event: KeyboardEvent| {
            let next = match event.key().as_str() {
                "ArrowRight" => (index + 1) % SECTIONS.len(),
                "ArrowLeft" => (index + SECTIONS.len() - 1) % SECTIONS.len(),
                "Home" => 0,
                "End" => SECTIONS.len() - 1,
                _ => return,
            };
            event.prevent_default();
            let _ = select_section(&page, next);
            if let Some(tab) = page.get_element_by_id(&format!("tab-{}", SECTIONS[next])) {
                let _ = tab.unchecked_into::<HtmlElement>().focus();
            }
        });
        tab.add_event_listener_with_callback("keydown", key.as_ref().unchecked_ref())?;
        key.forget();
    }
    select_section(document, 0)
}

/// Sound-page controls: element id, parameter index, displayed decimals, unit.
const CONTROLS: [(&str, usize, usize, &str); 18] = [
    ("law", 1, 0, ""),
    ("distance", 2, 2, " mm"),
    ("alignment", 3, 2, " mm"),
    ("hardness", 4, 3, ""),
    ("sustain", 5, 3, ""),
    ("bell", 6, 3, ""),
    ("dynamics", 7, 3, ""),
    ("bass", 8, 1, " dB"),
    ("treble", 9, 1, " dB"),
    ("vibrato", 10, 0, ""),
    ("speed", 11, 2, " Hz"),
    ("intensity", 12, 3, ""),
    ("preamp", 13, 0, ""),
    ("bass-boost", 14, 3, ""),
    // The Hammer page section already owns id="hammer".
    ("hammer-mass", 15, 3, ""),
    ("tine", 16, 3, ""),
    ("pole", 17, 3, ""),
    ("twist", 18, 3, ""),
];

impl App {
    fn element(&self, id: &str) -> Element {
        self.document
            .get_element_by_id(id)
            .expect("static UI element")
    }
    fn text(&self, id: &str, text: &str) {
        self.element(id).set_text_content(Some(text));
    }
    fn render(&self) {
        let ready = self.connected && self.client.loaded && !self.client.selecting();
        self.render_programs(ready);
        let stage = self.client.display(13) == 0.0;
        for (id, hidden) in [
            ("stage-controls", !stage),
            ("console-controls", stage),
            ("vibrato-control", stage),
        ] {
            let element = self.element(id);
            if hidden {
                let _ = element.set_attribute("hidden", "");
            } else {
                let _ = element.remove_attribute("hidden");
            }
        }
        for id in CONTROLS.iter().map(|c| c.0).chain(["gain", "gain-number"]) {
            let element = self.element(id);
            if !matches!(id, "law" | "preamp" | "vibrato") && id != "gain" && id != "gain-number" {
                let number = self.element(&format!("{id}-number"));
                if ready {
                    let _ = number.remove_attribute("disabled");
                } else {
                    let _ = number.set_attribute("disabled", "");
                }
            }
            if ready {
                let _ = element.remove_attribute("disabled");
            } else {
                let _ = element.set_attribute("disabled", "");
            }
        }
        let focused = self.document.active_element().map(|element| element.id());
        for (id, index, decimals, unit) in CONTROLS {
            let value = self.client.display(index);
            let element = self.element(id);
            if matches!(id, "law" | "preamp" | "vibrato") {
                element
                    .unchecked_into::<HtmlSelectElement>()
                    .set_value(&format!("{value}"));
                self.text(
                    &format!("{id}-value"),
                    if id == "preamp" {
                        if value == 0.0 { "Passive" } else { "Console" }
                    } else if id == "vibrato" {
                        if value == 0.0 { "Off" } else { "On" }
                    } else if value == 2.0 {
                        "Register Aperture"
                    } else if value == 1.0 {
                        "Aperture"
                    } else {
                        "Production"
                    },
                );
            } else {
                element
                    .unchecked_ref::<HtmlInputElement>()
                    .set_value_as_number(value);
                let number_id = format!("{id}-number");
                if focused.as_deref() != Some(&number_id) {
                    self.element(&number_id)
                        .unchecked_ref::<HtmlInputElement>()
                        .set_value(&format!("{value:.decimals$}"));
                }
                self.render_knob(id, value);
                self.text(&format!("{id}-value"), &format!("{value:.decimals$}{unit}"));
            }
        }
        let gain = self.client.display(0);
        self.render_knob("gain", gain);
        for id in ["gain", "gain-number"] {
            // Only the numeric text field can contain an uncommitted edit.
            // Focus alone must not freeze sliders or selectors during MIDI automation.
            if id != "gain-number" || focused.as_deref() != Some(id) {
                self.element(id)
                    .unchecked_into::<HtmlInputElement>()
                    .set_value_as_number(gain);
            }
        }
        self.text(
            "gain-db",
            &if gain > 0.0 {
                format!("{:.1} dB", 20.0 * gain.log10())
            } else {
                "Muted".into()
            },
        );
        self.text("status", &self.client.status);
        let _ = self
            .element("status")
            .set_attribute("data-ready", if ready { "true" } else { "false" });
    }

    fn render_knob(&self, id: &str, value: f64) {
        let input = self.element(id).unchecked_into::<HtmlInputElement>();
        let min = input.min().parse::<f64>().unwrap_or(0.0);
        let max = input.max().parse::<f64>().unwrap_or(1.0);
        let angle = -135.0 + 270.0 * ((value - min) / (max - min)).clamp(0.0, 1.0);
        let _ = self
            .element(&format!("{id}-knob"))
            .set_attribute("style", &format!("--angle: {angle}deg"));
    }

    fn render_programs(&self, ready: bool) {
        let list = self.element("program-list");
        let select = self.element("program-select");
        let signature = format!("{:?}", self.client.sounds);
        if list.get_attribute("data-catalog").as_deref() != Some(&signature) {
            list.set_text_content(None);
            select.set_text_content(None);
            let placeholder = self.document.create_element("option").expect("option");
            let _ = placeholder.set_attribute("value", "");
            let _ = placeholder.set_attribute("disabled", "");
            placeholder.set_text_content(Some("Select a program"));
            let _ = select.append_child(&placeholder);
            for sound in &self.client.sounds {
                let button = self
                    .document
                    .create_element("button")
                    .expect("program button");
                let _ = button.set_attribute("type", "button");
                let _ = button.set_attribute("data-sound-id", &sound.id);
                let _ = button.set_attribute("title", &sound.detail);
                button.set_text_content(Some(&sound.name));
                let _ = list.append_child(&button);
                let option = self
                    .document
                    .create_element("option")
                    .expect("program option");
                let _ = option.set_attribute("value", &sound.id);
                option.set_text_content(Some(&sound.name));
                let _ = select.append_child(&option);
            }
            let _ = list.set_attribute("data-catalog", &signature);
        }
        let children = list.children();
        for index in 0..children.length() {
            if let Some(button) = children.item(index) {
                let selected =
                    button.get_attribute("data-sound-id").as_deref() == Some(&self.client.selected);
                let _ =
                    button.set_attribute("aria-pressed", if selected { "true" } else { "false" });
                if ready {
                    let _ = button.remove_attribute("disabled");
                } else {
                    let _ = button.set_attribute("disabled", "");
                }
            }
        }
        select
            .unchecked_ref::<HtmlSelectElement>()
            .set_value(&self.client.selected);
        let current = self
            .client
            .sounds
            .iter()
            .find(|s| s.id == self.client.selected);
        self.text(
            "program-name",
            current.map_or("Choose your voice", |s| s.name.as_str()),
        );
        self.text(
            "program-detail",
            current.map_or("Select a starting point, then shape its character.", |s| {
                s.detail.as_str()
            }),
        );
        self.text(
            "program-error",
            self.client.selection_error.as_deref().unwrap_or(""),
        );
        for id in ["program-select", "program-prev", "program-next"] {
            let element = self.element(id);
            if ready && !self.client.sounds.is_empty() {
                let _ = element.remove_attribute("disabled");
            } else {
                let _ = element.set_attribute("disabled", "");
            }
        }
    }

    fn send(&self, message: &Value) -> Result<(), JsValue> {
        let parent = self
            .window
            .parent()?
            .ok_or_else(|| JsValue::from_str("missing host"))?;
        let data = JSON::parse(&message.to_string())?;
        parent.post_message(&data, &self.origin)
    }

    fn pump(&mut self, poll: bool) {
        if !self.connected {
            return;
        }
        let now = self
            .window
            .performance()
            .expect("browser monotonic clock")
            .now();
        if let Some(request) = self.client.next(now, poll)
            && self.send(&request).is_err()
        {
            self.client.loaded = false;
            self.client.status = "Could not contact RackForge. Reconnecting...".into();
        }
        self.render();
    }
}

fn parameter_event(app: &Shared, id: &str, index: usize, event_name: &str) -> Result<(), JsValue> {
    let element = app.borrow().element(id);
    let control = element.clone();
    let app = app.clone();
    let callback = Closure::<dyn FnMut(Event)>::new(move |_| {
        let value = if let Some(input) = control.dyn_ref::<HtmlInputElement>() {
            input.value_as_number()
        } else {
            control
                .unchecked_ref::<HtmlSelectElement>()
                .value()
                .parse()
                .unwrap_or(f64::NAN)
        };
        let mut app = app.borrow_mut();
        app.client.queue(index, value);
        app.pump(false);
    });
    element.add_event_listener_with_callback(event_name, callback.as_ref().unchecked_ref())?;
    callback.forget();
    Ok(())
}

// The transparent native range keeps keyboard and accessibility semantics.
// Pointer capture implements vertical rotary drag without scrolling the page.
fn knob_events(app: &Shared, id: &str, index: usize) -> Result<(), JsValue> {
    let input = app
        .borrow()
        .element(id)
        .unchecked_into::<HtmlInputElement>();
    let drag = Rc::new(RefCell::new(None::<(i32, f64, f64)>));
    for event_name in [
        "pointerdown",
        "pointermove",
        "pointerup",
        "pointercancel",
        "lostpointercapture",
    ] {
        let shared = app.clone();
        let input = input.clone();
        let target = input.clone();
        let drag = drag.clone();
        let callback = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
            if event_name == "pointerdown" {
                if input.disabled() || event.button() != 0 {
                    return;
                }
                event.prevent_default();
                let _ = input.focus();
                if input.set_pointer_capture(event.pointer_id()).is_ok() {
                    *drag.borrow_mut() = Some((
                        event.pointer_id(),
                        f64::from(event.client_y()),
                        input.value_as_number(),
                    ));
                }
            } else if event_name == "pointermove" {
                let Some((pointer, previous_y, previous)) = *drag.borrow() else {
                    return;
                };
                if pointer != event.pointer_id() {
                    return;
                }
                event.prevent_default();
                let min = input.min().parse::<f64>().unwrap_or(0.0);
                let max = input.max().parse::<f64>().unwrap_or(1.0);
                let step = input.step().parse::<f64>().unwrap_or(0.001);
                let y = f64::from(event.client_y());
                let scale = if event.shift_key() { 1600.0 } else { 160.0 };
                let raw = (previous + (previous_y - y) * (max - min) / scale).clamp(min, max);
                *drag.borrow_mut() = Some((pointer, y, raw));
                let value = (min + ((raw - min) / step).round() * step).clamp(min, max);
                let mut app = shared.borrow_mut();
                app.client.queue(index, value);
                app.pump(false);
            } else if drag
                .borrow()
                .is_some_and(|(pointer, _, _)| pointer == event.pointer_id())
            {
                *drag.borrow_mut() = None;
                let _ = input.release_pointer_capture(event.pointer_id());
            }
        });
        target.add_event_listener_with_callback(event_name, callback.as_ref().unchecked_ref())?;
        callback.forget();
    }
    let shared = app.clone();
    let reset = Closure::<dyn FnMut(Event)>::new(move |_| {
        let mut app = shared.borrow_mut();
        app.client.queue(index, crate::client::DEFAULTS[index]);
        app.pump(false);
    });
    input.add_event_listener_with_callback("dblclick", reset.as_ref().unchecked_ref())?;
    reset.forget();
    Ok(())
}

fn program_events(app: &Shared) -> Result<(), JsValue> {
    for id in [
        "program-list",
        "program-select",
        "program-prev",
        "program-next",
    ] {
        let element = app.borrow().element(id);
        let shared = app.clone();
        let callback = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
            let mut app = shared.borrow_mut();
            let sound = match id {
                "program-select" => {
                    Some(app.element(id).unchecked_ref::<HtmlSelectElement>().value())
                }
                "program-list" => event
                    .target()
                    .and_then(|target| target.dyn_into::<Element>().ok())
                    .and_then(|target| target.closest("button[data-sound-id]").ok().flatten())
                    .and_then(|button| button.get_attribute("data-sound-id")),
                _ => {
                    let count = app.client.sounds.len();
                    if count == 0 {
                        return;
                    }
                    let current = app
                        .client
                        .sounds
                        .iter()
                        .position(|s| s.id == app.client.selected);
                    let next = current.map_or(0, |index| {
                        if id == "program-next" {
                            (index + 1) % count
                        } else {
                            (index + count - 1) % count
                        }
                    });
                    Some(app.client.sounds[next].id.clone())
                }
            };
            if let Some(sound) = sound {
                app.client.select(&sound);
                app.pump(false);
            }
        });
        element.add_event_listener_with_callback(
            if id == "program-select" {
                "change"
            } else {
                "click"
            },
            callback.as_ref().unchecked_ref(),
        )?;
        callback.forget();
    }
    Ok(())
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("missing window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("missing document"))?;
    let origin = window.location().origin()?;
    section_events(&document)?;
    let app = Rc::new(RefCell::new(App {
        window,
        document,
        origin,
        connected: false,
        client: Client::default(),
    }));
    for (id, index, event) in CONTROLS
        .iter()
        .map(|c| {
            (
                c.0,
                c.1,
                if matches!(c.0, "law" | "preamp" | "vibrato") {
                    "change"
                } else {
                    "input"
                },
            )
        })
        .chain([("gain", 0, "input"), ("gain-number", 0, "change")])
    {
        parameter_event(&app, id, index, event)?;
    }
    for (id, index, _, _) in CONTROLS {
        if !matches!(id, "law" | "preamp" | "vibrato") {
            parameter_event(&app, &format!("{id}-number"), index, "change")?;
            knob_events(&app, id, index)?;
        }
    }
    knob_events(&app, "gain", 0)?;
    program_events(&app)?;
    let messages = app.clone();
    let callback = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let mut app = messages.borrow_mut();
        let parent = app
            .window
            .parent()
            .ok()
            .flatten()
            .zip(event.source())
            .is_some_and(|(parent, source)| Object::is(parent.as_ref(), source.as_ref()));
        if !parent || event.origin() != app.origin {
            return;
        }
        let Some(text) = JSON::stringify(&event.data())
            .ok()
            .and_then(|text| text.as_string())
        else {
            return;
        };
        if text.len() > 262144 {
            return;
        }
        let Ok(message) = serde_json::from_str::<Value>(&text) else {
            return;
        };
        if message["protocol"] != PROTOCOL {
            return;
        }
        match message["kind"].as_str() {
            Some("context") if message["instance"]["plugin_id"] == "org.rackforge.rftines" => {
                let first = !app.connected;
                app.connected = true;
                app.client.context(&message["instance"]);
                if let Some(lighting @ ("day" | "stage")) = message["host"]["lighting"].as_str() {
                    let _ = app
                        .document
                        .document_element()
                        .expect("HTML root")
                        .set_attribute("data-lighting", lighting);
                }
                app.pump(first);
            }
            Some("response") => {
                app.client.response(&message);
                app.pump(false);
            }
            _ => {}
        }
    });
    app.borrow()
        .window
        .add_event_listener_with_callback("message", callback.as_ref().unchecked_ref())?;
    callback.forget();
    let timer = app.clone();
    let callback = Closure::<dyn FnMut()>::new(move || {
        timer.borrow_mut().pump(true);
    });
    app.borrow()
        .window
        .set_interval_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            500,
        )?;
    callback.forget();
    app.borrow().render();
    app.borrow()
        .send(&json!({"protocol": PROTOCOL, "kind": "ready"}))
}
