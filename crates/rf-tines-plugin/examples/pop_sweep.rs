//! How hard must the contact be for the attack to match the instrument?
//!
//! Softening the contact from 4e10 to 4e8 removed a 5-10 kHz tick a listener
//! heard and the reference did not. Measured afterwards at MIDI 52, every
//! preset now falls short of the reference's attack instead: the real Rhodes
//! carries its first 15 ms at -26.5 dB under the tone, `portable-bark-1972`
//! at -32.2. The listener puts it the same way -- "le falta ese pop inicial".
//!
//! So the softening may have overshot. This sweeps `hardness` on the default
//! preset and scores the attack band by band against the reference, on
//! training notes only. Matching the instrument is the objective; "enough
//! pop" is not, because that is a taste knob and the tick came back last time
//! it was turned by ear.
//!
//! Writes nothing and promotes nothing.
//!
//! `cargo run --release -p rf-tines-plugin --example pop_sweep -- SAMPLES`
use rackforge_plugin_sdk::{MidiEvent, Processor};
use rf_tines_plugin::RfTinesProcessor;
use std::{f64::consts::TAU, path::Path};

const RATE: f64 = 48_000.0;
const FRAMES: usize = 256;
const PARAMETER_HARDNESS: u32 = 4;
/// Training notes in the register the report came from. 64 is reserved and
/// 52 is where the gap was measured, so neither is scored here.
const NOTES: [(u8, &str); 4] = [(38, "D2"), (50, "D3"), (55, "G3"), (59, "B3")];
/// Octave centres from the body of the tone up through the tick's band.
const BANDS: [f64; 6] = [750.0, 1500.0, 3000.0, 5000.0, 7500.0, 10_000.0];

fn read_wav(path: &Path) -> Result<(Vec<f64>, f64), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let (mut i, mut fmt, mut data) = (12, None, None);
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into()?) as usize;
        let body = &bytes[i + 8..(i + 8 + size).min(bytes.len())];
        match id {
            b"fmt " => fmt = Some(body.to_vec()),
            b"data" => data = Some(body.to_vec()),
            _ => {}
        }
        i += 8 + size + (size & 1);
    }
    let fmt = fmt.ok_or("no fmt")?;
    let data = data.ok_or("no data")?;
    let tag = u16::from_le_bytes(fmt[0..2].try_into()?);
    let channels = usize::from(u16::from_le_bytes(fmt[2..4].try_into()?));
    let rate = f64::from(u32::from_le_bytes(fmt[4..8].try_into()?));
    let bits = u16::from_le_bytes(fmt[14..16].try_into()?);
    let all: Vec<f64> = match (tag, bits) {
        (3, 32) => data
            .chunks_exact(4)
            .map(|c| f64::from(f32::from_le_bytes(c.try_into().unwrap())))
            .collect(),
        (1, 16) => data
            .chunks_exact(2)
            .map(|c| f64::from(i16::from_le_bytes(c.try_into().unwrap())) / 32768.0)
            .collect(),
        (1, 24) => data
            .chunks_exact(3)
            .map(|c| {
                let v = i32::from(c[0]) | i32::from(c[1]) << 8 | (i32::from(c[2] as i8)) << 16;
                f64::from(v) / 8_388_608.0
            })
            .collect(),
        _ => return Err(format!("unsupported wav: tag {tag} bits {bits}").into()),
    };
    Ok((all.iter().step_by(channels).copied().collect(), rate))
}

fn render(hardness: Option<f64>, note: u8) -> Vec<f64> {
    let mut plugin = RfTinesProcessor::default();
    assert!(plugin.load_preset("portable-bark-1972"));
    if let Some(value) = hardness {
        assert!(plugin.set_parameter(PARAMETER_HARDNESS, value));
    }
    assert!(plugin.prepare(RATE, FRAMES as u32, 0, 2));
    let on = [MidiEvent {
        frame: 0,
        data: [0x90, note, 108],
        length: 3,
    }];
    let blocks = (RATE * 2.0 / FRAMES as f64) as usize;
    let mut out = Vec::with_capacity(blocks * FRAMES);
    for block in 0..blocks {
        let mut audio = [0.0_f32; FRAMES * 2];
        let events: &[MidiEvent] = if block == 0 { &on } else { &[] };
        plugin.process(&[], &mut audio, events, &[], FRAMES as u32, 0, 2);
        out.extend(audio.as_chunks::<2>().0.iter().map(|f| f64::from(f[0])));
    }
    out
}

fn energy(block: &[f64], rate: f64, centre: f64) -> f64 {
    let n = block.len();
    if n < 64 {
        return 1e-30;
    }
    let mut total = 0.0;
    for step in 0..7 {
        let f = centre * 2.0_f64.powf((step as f64 / 6.0 - 0.5) / 1.5);
        let (mut re, mut im) = (0.0, 0.0);
        for (i, value) in block.iter().enumerate() {
            let w = 0.5 - 0.5 * (TAU * i as f64 / n as f64).cos();
            let phase = TAU * f * i as f64 / rate;
            re += value * w * phase.cos();
            im += value * w * phase.sin();
        }
        total += (re * re + im * im) / (n * n) as f64;
    }
    total
}

/// The attack's spectrum in the first 15 ms, in dB under the tone that
/// follows it, so the comparison survives any difference in level.
fn attack_shape(signal: &[f64], rate: f64) -> Vec<f64> {
    let peak = signal.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    let onset = signal
        .iter()
        .position(|v| v.abs() > peak * 0.02)
        .unwrap_or(0);
    let attack = &signal[onset..(onset + (rate * 0.015) as usize).min(signal.len())];
    let tone = &signal[onset..(onset + (rate * 0.25) as usize).min(signal.len())];
    let reference = energy(tone, rate, 300.0).max(1e-30);
    BANDS
        .iter()
        .map(|&c| 10.0 * (energy(attack, rate, c) / reference).max(1e-30).log10())
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::env::args().nth(1).ok_or("usage: pop_sweep SAMPLES")?;
    let source = Path::new(&source);
    let names = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];

    let mut reference = Vec::new();
    for (note, _) in NOTES {
        let name = format!("{}{}-f.wav", names[usize::from(note % 12)], note / 12 - 2);
        let (signal, rate) = read_wav(&source.join(name))?;
        reference.push(attack_shape(&signal, rate));
    }

    println!("ataque contra el Rhodes real, notas de entrenamiento {NOTES:?}");
    println!("bandas en Hz: {BANDS:?}\n");
    println!(
        "{:<10}{:>10}{:>44}",
        "hardness", "distancia", "exceso por banda, dB (+ = de mas)"
    );
    let mut best = (f64::INFINITY, 0.0);
    for step in 0..=10 {
        let hardness = 0.45 + 0.05 * f64::from(step);
        if hardness > 1.0 {
            break;
        }
        let mut sum = vec![0.0; BANDS.len()];
        let mut squared = 0.0;
        for (index, (note, _)) in NOTES.iter().enumerate() {
            let shape = attack_shape(&render(Some(hardness), *note), RATE);
            for (b, value) in shape.iter().enumerate() {
                let delta = value - reference[index][b];
                sum[b] += delta / NOTES.len() as f64;
                squared += delta * delta;
            }
        }
        let distance = (squared / (NOTES.len() * BANDS.len()) as f64).sqrt();
        if distance < best.0 {
            best = (distance, hardness);
        }
        let marker = if (hardness - 0.45).abs() < 1e-9 {
            " <- el que envia"
        } else {
            ""
        };
        print!("{hardness:<10.2}{distance:>10.1}   ");
        for value in &sum {
            print!("{value:>7.1}");
        }
        println!("{marker}");
    }
    println!(
        "\nmejor coincidencia: hardness {:.2} a {:.1} dB. El que envia es 0.45.",
        best.1, best.0
    );
    println!("Esto mide parecido al instrumento, no gusto. Falta la prueba de oido.");
    Ok(())
}
