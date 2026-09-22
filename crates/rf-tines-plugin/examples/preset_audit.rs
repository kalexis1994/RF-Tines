//! Renders every advertised preset through the plugin itself.
//!
//! The separation figures in docs/PERIOD-INSTRUMENTS.md were measured on the
//! laboratory renderer, driving the DSP profile directly. That is not the path
//! a player hears. The plugin resolves a `Settings` into a `Profile`, turns on
//! level compensation, applies the register geometry adjustment per note and
//! runs the output through the panel electronics. Any of those can flatten a
//! difference that exists in the model.
//!
//! So this writes what the host would actually produce, one file per preset
//! per dynamic layer, and leaves the comparison to whatever measures them.
//!
//! `cargo run --release -p rf-tines-plugin --example preset_audit -- OUT_DIR`
use rackforge_plugin_sdk::{MidiEvent, Processor};
use rf_tines_plugin::RfTinesProcessor;
use std::{fs, io::Write, path::Path};

const RATE: u32 = 48_000;
const FRAMES: usize = 256;
const SECONDS: f64 = 2.0;
const NOTE: u8 = 55;

/// The advertised catalog, in the order package/metadata/presets.json lists it.
const PRESETS: [&str; 8] = [
    "portable-bark-1972",
    "tine-bass-1960",
    "felt-1966",
    "console-bark-1973",
    "portable-bell-1977",
    "console-bell-1978",
    "portable-chime-1980",
    "wide-dynamics-1984",
];
const LAYERS: [(&str, u8); 2] = [("p", 32), ("f", 108)];

fn render(preset: &str, velocity: u8) -> Vec<[f32; 2]> {
    let mut plugin = RfTinesProcessor::default();
    assert!(plugin.load_preset(preset), "{preset} does not load");
    assert!(plugin.prepare(f64::from(RATE), FRAMES as u32, 0, 2));
    let note = [MidiEvent {
        frame: 0,
        data: [0x90, NOTE, velocity],
        length: 3,
    }];
    let blocks = (f64::from(RATE) * SECONDS / FRAMES as f64) as usize;
    let mut out = Vec::with_capacity(blocks * FRAMES);
    for block in 0..blocks {
        let mut audio = [0.0_f32; FRAMES * 2];
        let events: &[MidiEvent] = if block == 0 { &note } else { &[] };
        plugin.process(&[], &mut audio, events, &[], FRAMES as u32, 0, 2);
        out.extend(audio.as_chunks::<2>().0.iter().copied());
    }
    out
}

fn write_wav(path: &Path, samples: &[[f32; 2]]) -> std::io::Result<()> {
    let data_bytes = (samples.len() * 2 * 4) as u32;
    let mut file = fs::File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&3u16.to_le_bytes())?; // IEEE float
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&RATE.to_le_bytes())?;
    file.write_all(&(RATE * 2 * 4).to_le_bytes())?;
    file.write_all(&8u16.to_le_bytes())?;
    file.write_all(&32u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())?;
    for pair in samples {
        for value in pair {
            file.write_all(&value.to_le_bytes())?;
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .ok_or("usage: preset_audit OUT_DIRECTORY")?;
    let out = Path::new(&out);
    fs::create_dir_all(out)?;
    println!("{:<22}{:>10}{:>10}{:>10}", "preset", "capa", "pico", "rms");
    for preset in PRESETS {
        for (layer, velocity) in LAYERS {
            let samples = render(preset, velocity);
            let peak = samples
                .iter()
                .flatten()
                .fold(0.0_f32, |m, x| m.max(x.abs()));
            let rms = (samples
                .iter()
                .flatten()
                .map(|x| f64::from(*x) * f64::from(*x))
                .sum::<f64>()
                / (samples.len() * 2) as f64)
                .sqrt();
            write_wav(&out.join(format!("{preset}-{layer}.wav")), &samples)?;
            println!("{preset:<22}{layer:>10}{peak:>10.4}{rms:>10.5}");
        }
    }
    println!("\nescritos en {}", out.display());
    Ok(())
}
