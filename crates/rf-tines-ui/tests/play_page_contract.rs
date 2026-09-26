//! The PLAY page has to carry every element the panel reaches for.
//!
//! `App::element` resolves an id and unwraps, on every frame, so one missing
//! or duplicated id does not degrade the panel: it stops the panel dead
//! before its first request, and what a player sees is the opening
//! "Connecting to RackForge..." forever, with no error anywhere. That is
//! exactly what shipped when four controls were added and one of them was
//! given `id="hammer"`, which the Hammer page section already owned:
//! `get_element_by_id` returned the section, the panel cast it to an input,
//! and the whole surface died.
//!
//! Nothing caught it, because the panel's tests cover its logic and nothing
//! looked at the page. This does, as text, on the native target, so it costs
//! nothing and runs everywhere.
const PAGE: &str = include_str!("../../../package/web/play.html");
const PANEL: &str = include_str!("../src/browser.rs");

/// Ids the panel resolves directly, outside the control table.
const FIXED: [&str; 7] = [
    "status",
    "program-detail",
    "gain",
    "gain-number",
    "stage-controls",
    "console-controls",
    "vibrato-control",
];
/// Controls the panel drives as a `<select>`; they carry no knob or number.
const SELECTS: [&str; 3] = ["law", "preamp", "vibrato"];

fn control_ids() -> Vec<String> {
    let start = PANEL
        .find("const CONTROLS")
        .expect("the panel declares a control table");
    let body = &PANEL[start..start + PANEL[start..].find("\n];").expect("table ends")];
    body.lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("(\"")?;
            Some(rest[..rest.find('"')?].to_string())
        })
        .collect()
}

fn occurrences(id: &str) -> usize {
    PAGE.matches(&format!("id=\"{id}\"")).count()
}

#[test]
fn every_id_the_panel_resolves_exists_exactly_once_on_the_page() {
    let controls = control_ids();
    assert!(
        controls.len() >= 14,
        "only found {} controls",
        controls.len()
    );
    let mut wanted: Vec<String> = FIXED.iter().map(|id| (*id).to_string()).collect();
    for id in &controls {
        wanted.push(id.clone());
        wanted.push(format!("{id}-value"));
        if !SELECTS.contains(&id.as_str()) {
            wanted.push(format!("{id}-number"));
            wanted.push(format!("{id}-knob"));
        }
    }
    wanted.push("gain-knob".into());
    for id in wanted {
        assert_eq!(
            occurrences(&id),
            1,
            "the page carries id {id:?} {} times, and the panel needs exactly one",
            occurrences(&id)
        );
    }
}

/// A duplicate anywhere is a trap even when the panel does not reach for it
/// today, because `get_element_by_id` silently answers with the first one.
#[test]
fn the_page_has_no_duplicate_ids_at_all() {
    let mut seen: Vec<&str> = PAGE
        .match_indices("id=\"")
        .filter_map(|(at, _)| {
            let rest = &PAGE[at + 4..];
            rest.find('"').map(|end| &rest[..end])
        })
        .collect();
    seen.sort_unstable();
    let mut duplicates: Vec<&str> = seen
        .windows(2)
        .filter(|w| w[0] == w[1])
        .map(|w| w[0])
        .collect();
    duplicates.dedup();
    assert!(
        duplicates.is_empty(),
        "duplicated on the page: {duplicates:?}"
    );
}

/// Every control the page offers must be one the panel knows how to drive,
/// or a player can move a knob that reaches nothing.
#[test]
fn the_page_offers_no_control_the_panel_ignores() {
    let controls = control_ids();
    for (at, _) in PAGE.match_indices("data-rackforge-parameter-index=") {
        // Back up to the tag this attribute sits in; it may be an input or a
        // select, so look for the opening angle bracket rather than a name.
        let open = PAGE[..at].rfind('<').expect("the attribute sits in a tag");
        let close = at + PAGE[at..].find('>').expect("the tag closes");
        let tag = &PAGE[open..close];
        let found = tag
            .find("id=\"")
            .expect("a parameter control carries an id");
        let rest = &tag[found + 4..];
        let id = &rest[..rest.find('"').expect("closed id")];
        assert!(
            controls.iter().any(|known| known == id) || id == "gain",
            "the page offers {id:?}, which the panel does not drive"
        );
    }
}
