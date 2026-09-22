//! RackForge adapter. Device access and persistence remain host responsibilities.
mod electronics;
mod program;
mod settings;
use rackforge_plugin_sdk::{
    MIDI_FAMILY_BEND, MIDI_FAMILY_CONTROL, MIDI_FAMILY_NOTE, MIDI2_FLAG_ORIGIN_7BIT,
    MIDI2_KIND_CONTROL_CHANGE, MIDI2_KIND_NOTE_OFF, MIDI2_KIND_NOTE_ON, MIDI2_KIND_PITCH_BEND,
    MidiEvent, MidiEvent2, ParameterEvent, Processor, export_processor,
};
use rf_tines_dsp::Engine;
pub use settings::{DEFAULT_GAIN, LAW_NAMES, PARAMETERS, Settings, presets};
use std::collections::BTreeMap;

pub const MAX_FRAMES: u32 = 4096;
pub const MAX_EVENTS: usize = 256;
pub const STATE_VERSION: u32 = 6;
/// V5 prefix (124 bytes: the V4 prefix plus seven electronic-control f64
/// fields), followed by four mechanical f64 fields.
pub const STATE_BYTES: usize = 156;
/// The V5 length, still loadable; its four mechanical controls take the
/// nominal profile, so a session saved before them sounds as it was saved.
const V5_STATE_BYTES: usize = 124;
pub const PARAMETER_GAIN: u32 = 0;
pub const PARAMETER_LAW: u32 = 1;
pub const PARAMETER_DISTANCE: u32 = 2;
pub const PARAMETER_ALIGNMENT: u32 = 3;
pub const PARAMETER_HARDNESS: u32 = 4;
pub const PARAMETER_SUSTAIN: u32 = 5;
pub const PARAMETER_BELL: u32 = 6;
pub const PARAMETER_DYNAMICS: u32 = 7;

pub struct RfTinesProcessor {
    engine: Option<Box<Engine>>,
    settings: Settings,
    electronics: electronics::Electronics,
    programs: BTreeMap<String, rackforge_program_api::ProgramDocument>,
    maximum_frames: u32,
    channels: u32,
}

impl Default for RfTinesProcessor {
    fn default() -> Self {
        let settings = presets()
            .into_iter()
            .find(|preset| preset.0 == "portable-bark-1972")
            .expect("default factory program exists")
            .3;
        Self {
            engine: None,
            settings,
            electronics: electronics::Electronics::new(48000.0, settings),
            programs: BTreeMap::new(),
            maximum_frames: 0,
            channels: 0,
        }
    }
}

impl RfTinesProcessor {
    fn apply_settings(&mut self, settings: Settings) -> bool {
        if !settings.valid() {
            return false;
        }
        let voicing_changed = (1..8).any(|i| self.settings.parameter(i) != settings.parameter(i));
        self.electronics.target(settings);
        self.settings = settings;
        if let Some(engine) = &mut self.engine {
            engine.set_gain(settings.gain);
            if voicing_changed {
                engine.set_profile(settings.profile());
            }
        }
        true
    }

    fn midi1(&mut self, event: &MidiEvent) {
        let Some(engine) = &mut self.engine else {
            return;
        };
        let [status, index, value] = event.data;
        match status & 0xf0 {
            0x90 => {
                engine.note_on(status & 15, index, value as f64 / 127.0);
            }
            0x80 => {
                engine.note_off(status & 15, index);
            }
            0xb0 => {
                engine.control_change(status & 15, index, value as f64 / 127.0);
            }
            0xe0 => {
                engine.pitch_bend(status & 15, bend_14(index, value));
            }
            _ => {}
        }
    }

    fn midi2(&mut self, event: &MidiEvent2) {
        let Some(engine) = &mut self.engine else {
            return;
        };
        match event.kind {
            MIDI2_KIND_NOTE_ON => {
                let velocity = if event.flags & MIDI2_FLAG_ORIGIN_7BIT != 0 {
                    (event.value >> 9) as f64 / 127.0
                } else {
                    // A genuine MIDI 2.0 Note On with zero velocity is not Note Off.
                    event.value.max(1) as f64 / 65535.0
                };
                engine.note_on(event.channel, event.index, velocity);
            }
            MIDI2_KIND_NOTE_OFF => {
                engine.note_off(event.channel, event.index);
            }
            MIDI2_KIND_CONTROL_CHANGE => {
                let value = if event.flags & MIDI2_FLAG_ORIGIN_7BIT != 0 {
                    (event.value >> 25) as f64 / 127.0
                } else {
                    event.value as f64 / u32::MAX as f64
                };
                engine.control_change(event.channel, event.index, value);
            }
            MIDI2_KIND_PITCH_BEND => {
                engine.pitch_bend(event.channel, bend_32(event.value));
            }
            _ => {}
        }
    }
}

impl Processor for RfTinesProcessor {
    fn prepare(&mut self, rate: f64, frames: u32, inputs: u32, outputs: u32) -> bool {
        if frames == 0 || frames > MAX_FRAMES || inputs != 0 || !(1..=2).contains(&outputs) {
            return false;
        }
        let Ok(mut engine) = Engine::new(rate, self.settings.profile()) else {
            return false;
        };
        engine.set_gain(self.settings.gain);
        engine.set_level_compensation(true);
        engine.reset();
        self.electronics = electronics::Electronics::new(rate, self.settings);
        self.engine = Some(Box::new(engine));
        self.maximum_frames = frames;
        self.channels = outputs;
        true
    }

    fn set_parameter(&mut self, index: u32, value: f64) -> bool {
        let Some(settings) = self.settings.with_parameter(index, value) else {
            return false;
        };
        self.apply_settings(settings)
    }

    fn get_parameter(&self, index: u32) -> Option<f64> {
        self.settings.parameter(index)
    }

    fn reset(&mut self) {
        self.electronics.reset(self.settings);
        if let Some(engine) = &mut self.engine {
            engine.reset();
        }
    }

    fn load_preset(&mut self, id: &str) -> bool {
        if let Some(preset) = presets().into_iter().find(|p| p.0 == id) {
            return self.apply_settings(preset.3);
        }
        let Some(settings) = id
            .strip_prefix("custom.")
            .and_then(|id| self.programs.get(id))
            .and_then(program::settings)
        else {
            return false;
        };
        self.apply_settings(settings)
    }

    fn save_state(&self, destination: &mut [u8]) -> Option<usize> {
        let bytes = destination.get_mut(..STATE_BYTES)?;
        bytes[..4].copy_from_slice(b"RFRH");
        bytes[4..8].copy_from_slice(&STATE_VERSION.to_le_bytes());
        let s = self.settings;
        for (i, value) in [
            s.gain,
            s.distance_mm,
            s.alignment_mm,
            s.hardness,
            s.sustain,
            s.bell,
            s.dynamics,
        ]
        .into_iter()
        .enumerate()
        {
            bytes[8 + 8 * i..16 + 8 * i].copy_from_slice(&value.to_le_bytes());
        }
        bytes[64..68].copy_from_slice(&[s.law, 0, 0, 0]);
        for (i, value) in [
            s.bass_db,
            s.treble_db,
            s.vibrato,
            s.speed_hz,
            s.intensity,
            s.preamp,
            s.bass_boost,
        ]
        .into_iter()
        .enumerate()
        {
            bytes[68 + 8 * i..76 + 8 * i].copy_from_slice(&value.to_le_bytes());
        }
        for (i, value) in [s.hammer, s.tine, s.pole, s.twist]
            .into_iter()
            .enumerate()
        {
            bytes[124 + 8 * i..132 + 8 * i].copy_from_slice(&value.to_le_bytes());
        }
        Some(STATE_BYTES)
    }

    fn load_state(&mut self, state: &[u8]) -> bool {
        if ![16, 20, 68, V5_STATE_BYTES, STATE_BYTES].contains(&state.len())
            || &state[..4] != b"RFRH"
        {
            return false;
        }
        let version = u32::from_le_bytes(state[4..8].try_into().expect("validated state length"));
        let field = |i: usize| {
            f64::from_le_bytes(
                state[8 + 8 * i..16 + 8 * i]
                    .try_into()
                    .expect("validated state length"),
            )
        };
        let gain = field(0);
        let settings = match (version, state.len()) {
            (1, 16) => Settings {
                gain,
                ..Settings::default()
            },
            // Schemas 2 and 3 held two pickup paths, the listened side and, in 3,
            // a profile index; they map onto the voicing they were listening to.
            (2 | 3, 20) if state[16] < 4 && state[17] < 4 && state[18] <= 1 && state[19] < 3 => {
                if version == 2 && state[19] != 0 {
                    return false;
                }
                let path = if state[18] == 1 { state[17] } else { state[16] };
                let (law, distance_mm, alignment_mm) = match path {
                    0 => (0, 1.5, 0.5),
                    3 => (1, 0.5, 0.5),
                    _ => (0, 0.5, 0.25),
                };
                let (sustain, bell) = match state[19] {
                    0 => (0.0, 1.0),
                    1 => (0.5, 1.0),
                    _ => (0.5, 0.2582),
                };
                Settings {
                    gain,
                    law,
                    distance_mm,
                    alignment_mm,
                    sustain,
                    bell,
                    ..Settings::default()
                }
            }
            (4, 68) | (5, V5_STATE_BYTES) | (STATE_VERSION, STATE_BYTES)
                if state[65..68] == [0, 0, 0] =>
            {
                Settings {
                gain,
                law: state[64],
                distance_mm: field(1),
                alignment_mm: field(2),
                hardness: field(3),
                sustain: field(4),
                bell: field(5),
                dynamics: field(6),
                ..Settings::default()
                }
            }
            _ => return false,
        };
        // Each later schema appends a block of controls. A shorter state
        // keeps the defaults for everything it predates, which for the
        // mechanical block is the nominal profile it was saved under.
        let mut settings = settings;
        for (from_version, base, range) in [(5u32, 68usize, 8u32..15), (6, 124, 15..19)] {
            if version < from_version {
                break;
            }
            for index in range {
                let offset = base + (index - (if from_version == 5 { 8 } else { 15 })) as usize * 8;
                let value = f64::from_le_bytes(
                    state[offset..offset + 8]
                        .try_into()
                        .expect("validated state"),
                );
                let Some(updated) = settings.with_parameter(index, value) else {
                    return false;
                };
                settings = updated;
            }
        }
        self.apply_settings(settings)
    }

    fn program_editing_capabilities(&self) -> u32 {
        rackforge_plugin_sdk::PROGRAM_EDIT_BASIC
            | rackforge_plugin_sdk::PROGRAM_EDIT_PREVIEW
            | rackforge_plugin_sdk::PROGRAM_EDIT_DECLARATIVE
    }

    fn write_program_catalog(&mut self, destination: &mut [u8]) -> Option<usize> {
        program::catalog(&self.programs, destination)
    }

    fn begin_program_edit(&mut self, request: &[u8], destination: &mut [u8]) -> Option<usize> {
        program::begin(self, request, destination)
    }

    fn prepare_program_save(&mut self, document: &[u8], destination: &mut [u8]) -> Option<usize> {
        program::prepare(document, destination)
    }

    fn install_program(&mut self, prepared: &[u8]) -> bool {
        program::install(self, prepared)
    }

    fn preview_program(&mut self, prepared: &[u8]) -> bool {
        let Some(document) = program::validated_prepared(prepared) else {
            return false;
        };
        self.apply_settings(program::settings(&document).expect("validated program"))
    }

    fn program_editor_view(&mut self, document: &[u8], destination: &mut [u8]) -> Option<usize> {
        program::view(document, destination)
    }

    fn apply_program_edit(&mut self, request: &[u8], destination: &mut [u8]) -> Option<usize> {
        program::edit(request, destination)
    }

    fn process(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        midi: &[MidiEvent],
        parameters: &[ParameterEvent],
        frames: u32,
        inputs: u32,
        outputs: u32,
    ) {
        self.process_wide(
            input,
            output,
            midi,
            &[],
            parameters,
            frames,
            inputs,
            outputs,
        );
    }

    fn process_wide(
        &mut self,
        _input: &[f32],
        output: &mut [f32],
        midi: &[MidiEvent],
        midi2: &[MidiEvent2],
        parameters: &[ParameterEvent],
        frames: u32,
        inputs: u32,
        outputs: u32,
    ) {
        output.fill(0.0);
        let samples = (frames as usize).checked_mul(outputs as usize);
        if self.engine.is_none()
            || frames > self.maximum_frames
            || inputs != 0
            || outputs != self.channels
            || samples.is_none_or(|n| n > output.len())
            || !ordered(midi.iter().map(|e| e.frame), frames)
            || !ordered(midi2.iter().map(|e| e.frame), frames)
            || !ordered(parameters.iter().map(|e| e.frame), frames)
            || midi.iter().any(|e| !valid_midi1(e))
            || midi2.iter().any(|e| {
                e.channel >= 16
                    || e.index >= 128
                    || (matches!(e.kind, MIDI2_KIND_NOTE_ON | MIDI2_KIND_NOTE_OFF)
                        && e.value > 65535)
            })
            || parameters
                .iter()
                .any(|e| self.settings.with_parameter(e.index, e.value).is_none())
        {
            return;
        }
        let (mut p, mut m, mut w) = (0, 0, 0);
        for frame in 0..frames {
            // Explicit stable tie order: parameters, MIDI 1.0, then MIDI 2.0.
            while p < parameters.len() && parameters[p].frame == frame {
                self.set_parameter(parameters[p].index, parameters[p].value);
                p += 1;
            }
            while m < midi.len() && midi[m].frame == frame {
                self.midi1(&midi[m]);
                m += 1;
            }
            while w < midi2.len() && midi2[w].frame == frame {
                self.midi2(&midi2[w]);
                w += 1;
            }
            let sample = self.engine.as_mut().expect("prepared engine").next_sample();
            let offset = frame as usize * outputs as usize;
            let [left, right] = self.electronics.process(sample);
            output[offset] = left;
            if outputs == 2 {
                output[offset + 1] = right;
            }
        }
    }
}

fn ordered(frames: impl Iterator<Item = u32>, block_frames: u32) -> bool {
    let mut previous = 0;
    let mut count = 0;
    for frame in frames {
        count += 1;
        if count > MAX_EVENTS || frame >= block_frames || frame < previous {
            return false;
        }
        previous = frame;
    }
    true
}

fn valid_midi1(event: &MidiEvent) -> bool {
    if !(1..=3).contains(&event.length) || event.data[0] < 128 {
        return false;
    }
    if event.data[1..event.length as usize]
        .iter()
        .any(|b| *b >= 128)
    {
        return false;
    }
    !matches!(event.data[0] & 0xf0, 0x80 | 0x90 | 0xb0 | 0xe0) || event.length == 3
}

fn bend_14(lsb: u8, msb: u8) -> f64 {
    let value = u16::from(lsb) | (u16::from(msb) << 7);
    if value >= 8192 {
        f64::from(value - 8192) / 8191.0
    } else {
        -f64::from(8192 - value) / 8192.0
    }
}

fn bend_32(value: u32) -> f64 {
    const CENTER: u32 = 1 << 31;
    if value >= CENTER {
        f64::from(value - CENTER) / f64::from(u32::MAX - CENTER)
    } else {
        -f64::from(CENTER - value) / f64::from(CENTER)
    }
}

export_processor!(RfTinesProcessor,
    max_frames = 4096, max_input_channels = 0, max_output_channels = 2,
    max_midi_events = 256, max_parameter_events = 256, max_transfer_bytes = 16384,
    midi2 = { max_events = 256, families = MIDI_FAMILY_NOTE | MIDI_FAMILY_CONTROL | MIDI_FAMILY_BEND }
);
