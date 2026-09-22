use crate::{
    PARAMETER_ALIGNMENT, PARAMETER_BELL, PARAMETER_DISTANCE, PARAMETER_DYNAMICS, PARAMETER_GAIN,
    PARAMETER_HARDNESS, PARAMETER_LAW, PARAMETER_SUSTAIN,
};
use rf_tines_dsp::{PickupLaw, Profile};
use serde::{Deserialize, Serialize};

/// Conservative starting gain for the measured ten-key repeated-strike case.
/// Not a limiter or a guarantee for arbitrary accumulated mechanical energy.
pub const DEFAULT_GAIN: f64 = 0.1;
/// Parameter order: gain, law, distance, alignment, hardness, sustain, bell,
/// dynamics, then the seven panel electronics, then the four mechanical
/// controls: hammer mass, tine mass, pole radius and axis twist.
pub const PARAMETERS: usize = 19;
pub const LAW_NAMES: [&str; 3] = ["Production", "Aperture", "Register Aperture"];

/// The Sound page in physical and normalized terms. Every field maps to a
/// `Profile` through `profile`; defaults are the retained 0.1.2 engine.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub gain: f64,
    /// Index into `LAW_NAMES`.
    pub law: u8,
    /// Pickup distance, the gap, 0.5..=3 mm.
    pub distance_mm: f64,
    /// Tine alignment, the lateral offset, -1..=1.5 mm.
    pub alignment_mm: f64,
    /// Hammer hardness 0..=1: contact stiffness 4e10 * 25^(2h - 1) N/m^2.
    pub hardness: f64,
    /// Sustain 0..=1: first partial T60 at A3 of 5 * 16^s seconds, the bar
    /// partial 0.16 * 14.375^(2s) capped at 10 s.
    pub sustain: f64,
    /// Bell 0..=1: second partial strike weight -0.3 * b^2.
    pub bell: f64,
    /// Dynamics 0..=1: velocity exponent 1.4 * 2^(2d - 1).
    pub dynamics: f64,
    #[serde(default)]
    pub bass_db: f64,
    #[serde(default)]
    pub treble_db: f64,
    #[serde(default)]
    pub vibrato: f64,
    #[serde(default = "default_speed")]
    pub speed_hz: f64,
    #[serde(default)]
    pub intensity: f64,
    #[serde(default = "one")]
    pub preamp: f64,
    #[serde(default = "one")]
    pub bass_boost: f64,
    /// Mass of the hammer, as a share of the nominal four grams. The one
    /// documented change this instrument could never express: a hammer that
    /// went from a wood and plastic hybrid to fully plastic in 1975, reported
    /// as a tighter response against the earlier bounce.
    pub hammer: f64,
    /// Mass the tine presents to the hammer. A heavier tine takes the same
    /// blow less far and holds the hammer longer, which is a different
    /// instrument rather than a different regulation of one.
    pub tine: f64,
    /// Radius of the pickup's pole face. Where the tine sits relative to the
    /// pole ring decides how sharply the flux turns over as it swings.
    pub pole: f64,
    /// How far the tine's principal bending axes are turned off the strike,
    /// and how far apart their frequencies sit. At zero the tip travels on a
    /// line, which is what every voicing before this control did; above it
    /// the path opens into a slowly turning ellipse. One control because both
    /// come from the same thing, the anisotropy of the mount.
    pub twist: f64,
}

fn default_speed() -> f64 {
    4.0
}
fn one() -> f64 {
    1.0
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            gain: DEFAULT_GAIN,
            law: 0,
            distance_mm: 1.5,
            alignment_mm: 0.5,
            hardness: 0.5,
            sustain: 0.0,
            bell: 1.0,
            dynamics: 0.5,
            bass_db: 0.0,
            treble_db: 0.0,
            vibrato: 0.0,
            speed_hz: 4.0,
            intensity: 0.0,
            preamp: 1.0,
            bass_boost: 1.0,
            // The nominal profile: 4 g hammer, 1.5 g tine, a 2 mm pole and a
            // tip that stays on one axis. Every preset written before these
            // controls inherits them and renders exactly as it did.
            hammer: 0.5,
            tine: 0.5,
            pole: 0.6,
            twist: 0.0,
        }
    }
}

/// Factory presets: id, name, description, settings.
/// Loadable factory IDs, including retired research IDs for saved-session compatibility.
/// Only entries in metadata/presets.json are advertised to the host.
///
/// The instrument presets from `felt-1966` onward each stand for a documented
/// change to the real instrument rather than a point on a smooth ramp. The
/// five decade entries before them are the earlier inspired approximations,
/// retired but still loadable so saved sessions keep working.
///
/// None of them is named after anybody's product. This project already
/// dropped one name for that reason -- RF-Rhodes, then RF-73, then RF-Tines
/// (docs/RENAMING.md) -- and a preset label carries the same weight as the
/// plugin's own. Visible names say what the voicing is and when the change
/// happened; the instruments, suppliers and preamps behind them are named
/// only in docs/PERIOD-INSTRUMENTS.md, where citing a source is the point.
///
/// What the sources support, and what they do not, is set out in
/// docs/PERIOD-INSTRUMENTS.md. Two limits are worth repeating here. The
/// hammer changed from a wood and plastic hybrid to fully plastic in 1975,
/// which is reported as a tighter response against the earlier bounce; this
/// model has no hammer mass control, so no preset expresses it. And the
/// pickup geometry these presets carry is a model parameter, not a recovered
/// dimension: docs/PICKUP-GEOMETRY-CEILING.md shows the whole usable range
/// sits below what the service manual permits, for reasons that are still
/// open.
pub fn presets() -> [(&'static str, &'static str, &'static str, Settings); 18] {
    let default = Settings::default();
    [
        (
            "research-direct",
            "Original",
            "The 0.1.2 engine: production pickup at 1.5 / 0.5 mm, original losses. No limiter.",
            default,
        ),
        (
            "close-original",
            "Close Original",
            "Production pickup at 0.5 / 0.25 mm; level compensated. Original losses.",
            Settings {
                distance_mm: 0.5,
                alignment_mm: 0.25,
                ..default
            },
        ),
        (
            "close-aperture",
            "Close Aperture",
            "Finite-aperture pickup at 0.5 / 0.5 mm with a 2 mm pole; original losses.",
            Settings {
                law: 1,
                distance_mm: 0.5,
                alignment_mm: 0.5,
                ..default
            },
        ),
        (
            "calibrated",
            "Calibrated",
            "Aperture pickup at 0.5 / 0.5 mm, recording-derived sustain, soft second partial.",
            Settings {
                law: 1,
                distance_mm: 0.5,
                alignment_mm: 0.5,
                sustain: 0.5,
                bell: 0.2582,
                ..default
            },
        ),
        (
            "calibrated-register",
            "Calibrated Register",
            "Calibrated with the validated upper-register pickup geometry; normal MIDI dynamics.",
            Settings {
                law: 2,
                distance_mm: 0.5,
                alignment_mm: 0.5,
                sustain: 0.5,
                bell: 0.2582,
                ..default
            },
        ),
        (
            "stage-early-70s",
            "Stage 73 - Early '70s",
            "Rounded early-stage inspired voicing; passive-style bass control.",
            Settings {
                law: 2,
                hardness: 0.42,
                sustain: 0.48,
                bell: 0.22,
                distance_mm: 0.75,
                alignment_mm: 0.48,
                preamp: 0.0,
                bass_db: 0.0,
                treble_db: 0.0,
                vibrato: 0.0,
                speed_hz: 4.0,
                intensity: 0.0,
                bass_boost: 0.9,
                ..default
            },
        ),
        (
            "suitcase-mid-70s",
            "Suitcase 73 - Mid '70s",
            "Mid-seventies inspired voicing with warm EQ and slow stereo tremolo.",
            Settings {
                law: 2,
                hardness: 0.46,
                sustain: 0.52,
                bell: 0.28,
                distance_mm: 0.58,
                alignment_mm: 0.4,
                preamp: 1.0,
                bass_db: 1.0,
                treble_db: -1.0,
                vibrato: 1.0,
                speed_hz: 3.2,
                intensity: 0.55,
                bass_boost: 1.0,
                ..default
            },
        ),
        (
            "stage-late-70s",
            "Stage 73 - Late '70s",
            "Late-stage inspired voicing; firmer attack and a more open bell component.",
            Settings {
                law: 2,
                hardness: 0.55,
                sustain: 0.45,
                bell: 0.38,
                distance_mm: 0.7,
                alignment_mm: 0.55,
                preamp: 0.0,
                bass_db: 0.0,
                treble_db: 0.0,
                vibrato: 0.0,
                speed_hz: 4.0,
                intensity: 0.0,
                bass_boost: 0.82,
                ..default
            },
        ),
        (
            "suitcase-late-70s",
            "Suitcase 73 - Late '70s",
            "Late-suitcase inspired voicing; clearer attack and lively stereo movement.",
            Settings {
                law: 2,
                hardness: 0.57,
                sustain: 0.46,
                bell: 0.4,
                distance_mm: 0.62,
                alignment_mm: 0.52,
                preamp: 1.0,
                bass_db: -1.0,
                treble_db: 1.5,
                vibrato: 1.0,
                speed_hz: 4.6,
                intensity: 0.5,
                bass_boost: 1.0,
                ..default
            },
        ),
        (
            "stage-80s",
            "Stage 73 - '80s",
            "Eighties-stage inspired voicing; articulate attack and restrained decay.",
            Settings {
                law: 2,
                hardness: 0.61,
                sustain: 0.4,
                bell: 0.46,
                distance_mm: 0.85,
                alignment_mm: 0.6,
                preamp: 0.0,
                bass_db: 0.0,
                treble_db: 0.0,
                vibrato: 0.0,
                speed_hz: 4.0,
                intensity: 0.0,
                bass_boost: 0.78,
                ..default
            },
        ),
        // --- Instruments, one per documented change ---------------------
        (
            "felt-1966",
            "Felt Tips 1966",
            "The softest hammer tips and the darkest attack of the set.",
            Settings {
                law: 2,
                hardness: 0.22,
                sustain: 0.5,
                bell: 0.18,
                distance_mm: 0.8,
                alignment_mm: 0.35,
                dynamics: 0.5,
                preamp: 0.0,
                vibrato: 0.0,
                intensity: 0.0,
                bass_boost: 0.85,
                // A light felt-tipped hammer against a heavier early tine, a wide
                // pole and a mount whose axes sit well off the strike.
                hammer: 0.34,
                tine: 0.6,
                pole: 0.72,
                twist: 0.3,
                ..default
            },
        ),
        (
            "portable-bark-1972",
            "Portable Bark 1972",
            "Cube rubber tips and a close pickup: the warm bark. Passive tone.",
            Settings {
                law: 2,
                hardness: 0.45,
                sustain: 0.5,
                bell: 0.26,
                distance_mm: 0.6,
                alignment_mm: 0.45,
                dynamics: 0.5,
                preamp: 0.0,
                vibrato: 0.0,
                intensity: 0.0,
                bass_boost: 0.85,
                // The nominal hardware, near enough: this is the voicing everything
                // else is heard against.
                hammer: 0.46,
                tine: 0.52,
                pole: 0.62,
                twist: 0.18,
                ..default
            },
        ),
        (
            "console-bark-1973",
            "Console Bark 1973",
            "The same mechanism through an active console preamp and stereo vibrato.",
            Settings {
                law: 2,
                hardness: 0.45,
                sustain: 0.5,
                bell: 0.26,
                distance_mm: 0.6,
                alignment_mm: 0.48,
                dynamics: 0.5,
                preamp: 1.0,
                bass_db: 1.5,
                treble_db: -0.5,
                vibrato: 1.0,
                speed_hz: 4.4,
                intensity: 0.5,
                bass_boost: 1.0,
                // The same hardware as 1972. Only the amplification differs.
                hammer: 0.46,
                tine: 0.52,
                pole: 0.62,
                twist: 0.18,
                ..default
            },
        ),
        (
            "portable-bell-1977",
            "Portable Bell 1977",
            "Graduated tips and a later tine: the bark turning towards bell.",
            Settings {
                law: 2,
                hardness: 0.62,
                sustain: 0.5,
                bell: 0.4,
                distance_mm: 0.68,
                alignment_mm: 0.52,
                dynamics: 0.5,
                preamp: 0.0,
                vibrato: 0.0,
                intensity: 0.0,
                bass_boost: 0.85,
                // A heavier hammer on a lighter tine over a narrower pole, which is
                // where the bark starts giving way to bell.
                hammer: 0.6,
                tine: 0.44,
                pole: 0.5,
                twist: 0.12,
                ..default
            },
        ),
        (
            "console-bell-1978",
            "Console Bell 1978",
            "The later mechanism through the second console preamp, brighter in its EQ.",
            Settings {
                law: 2,
                hardness: 0.62,
                sustain: 0.5,
                bell: 0.4,
                distance_mm: 0.66,
                alignment_mm: 0.5,
                dynamics: 0.5,
                preamp: 1.0,
                bass_db: -1.0,
                treble_db: 1.5,
                vibrato: 1.0,
                speed_hz: 5.2,
                intensity: 0.45,
                bass_boost: 1.0,
                // The same hardware as 1977. Only the amplification differs.
                hammer: 0.6,
                tine: 0.44,
                pole: 0.5,
                twist: 0.12,
                ..default
            },
        ),
        (
            "portable-chime-1980",
            "Portable Chime 1980",
            "The most bell and the least deep bark of the set. Passive tone.",
            Settings {
                law: 2,
                hardness: 0.64,
                sustain: 0.5,
                bell: 0.52,
                distance_mm: 0.78,
                alignment_mm: 0.58,
                dynamics: 0.5,
                preamp: 0.0,
                vibrato: 0.0,
                intensity: 0.0,
                bass_boost: 0.85,
                // The fully plastic hammer at its heaviest against the lightest
                // tine: least deep, most bell.
                hammer: 0.68,
                tine: 0.38,
                pole: 0.42,
                twist: 0.08,
                ..default
            },
        ),
        (
            "wide-dynamics-1984",
            "Wide Dynamics 1984",
            "A later action, reaching further between a soft touch and a hard one.",
            Settings {
                law: 2,
                hardness: 0.66,
                sustain: 0.5,
                bell: 0.46,
                distance_mm: 0.72,
                alignment_mm: 0.55,
                dynamics: 0.68,
                preamp: 0.0,
                vibrato: 0.0,
                intensity: 0.0,
                bass_boost: 0.9,
                // A lighter instrument, which is how the last of them is
                // described: the lightest hammer of the late group on a light
                // tine, and the widest reach between a soft touch and a hard
                // one. Sat between 1977 and 1980 in a first pass and was
                // 1.54 dB from 1977, which the separation test refused.
                hammer: 0.40,
                tine: 0.42,
                pole: 0.46,
                twist: 0.26,
                ..default
            },
        ),
        (
            "tine-bass-1960",
            "Tine Bass 1960",
            "A short bass keyboard, E1 to B3, in a felt-tip voicing. Play it low.",
            Settings {
                law: 2,
                hardness: 0.22,
                sustain: 0.5,
                bell: 0.15,
                distance_mm: 0.88,
                alignment_mm: 0.32,
                dynamics: 0.44,
                preamp: 0.0,
                vibrato: 0.0,
                intensity: 0.0,
                bass_boost: 1.0,
                // The heaviest tine of the set under a modest hammer, far from a wide
                // pole: the short bass keyboard's own hardware.
                hammer: 0.4,
                tine: 0.72,
                pole: 0.8,
                twist: 0.34,
                ..default
            },
        ),
    ]
}

fn unit(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

impl Settings {
    pub fn valid(self) -> bool {
        self.gain.is_finite()
            && (0.0..=2.0).contains(&self.gain)
            && usize::from(self.law) < LAW_NAMES.len()
            && self.distance_mm.is_finite()
            && (0.5..=3.0).contains(&self.distance_mm)
            && self.alignment_mm.is_finite()
            && (-1.0..=1.5).contains(&self.alignment_mm)
            && unit(self.hardness)
            && unit(self.sustain)
            && unit(self.bell)
            && unit(self.dynamics)
            && self.bass_db.is_finite()
            && (-12.0..=12.0).contains(&self.bass_db)
            && self.treble_db.is_finite()
            && (-12.0..=12.0).contains(&self.treble_db)
            && [0.0, 1.0].contains(&self.vibrato)
            && self.speed_hz.is_finite()
            && (0.5..=12.0).contains(&self.speed_hz)
            && unit(self.intensity)
            && [0.0, 1.0].contains(&self.preamp)
            && unit(self.bass_boost)
            && unit(self.hammer)
            && unit(self.tine)
            && unit(self.pole)
            && unit(self.twist)
    }

    /// The mechanical and pickup profile these settings describe.
    pub fn profile(self) -> Profile {
        Profile {
            pickup_law: match self.law {
                1 => PickupLaw::Aperture,
                2 => PickupLaw::RegisterAperture,
                _ => PickupLaw::Production,
            },
            pickup_gap_m: self.distance_mm * 1e-3,
            pickup_offset_m: self.alignment_mm * 1e-3,
            contact_stiffness: 4.0e10 * 25.0_f64.powf(2.0 * self.hardness - 1.0),
            decay_seconds: 5.0 * 16.0_f64.powf(self.sustain),
            bar_partial_decay_seconds: (0.16 * 14.375_f64.powf(2.0 * self.sustain)).min(10.0),
            bar_partial_strike_weight: -0.3 * self.bell * self.bell,
            velocity_exponent: 1.4 * 2.0_f64.powf(2.0 * self.dynamics - 1.0),
            // Geometric about the nominal mass, so a half setting is the
            // nominal value and the two ends are a factor of two either way.
            hammer_mass_kg: 0.004 * 2.0_f64.powf(2.0 * self.hammer - 1.0),
            modal_mass_kg: 0.0015 * 2.0_f64.powf(2.0 * self.tine - 1.0),
            // Linear across the pole radii the flux law accepts; 0.6 is the
            // 2 mm pole every profile before this control used.
            pickup_pole_radius_m: 0.0005 + 0.0025 * self.pole,
            // Both axis numbers come from one physical cause, so one control
            // moves them together, and zero is exactly the single-axis tip.
            tine_boundary_angle_rad: 0.6 * self.twist,
            tine_transverse_frequency_ratio: 1.0 + 0.06 * self.twist,
            ..Profile::default()
        }
    }

    pub fn parameter(self, index: u32) -> Option<f64> {
        Some(match index {
            PARAMETER_GAIN => self.gain,
            PARAMETER_LAW => f64::from(self.law),
            PARAMETER_DISTANCE => self.distance_mm,
            PARAMETER_ALIGNMENT => self.alignment_mm,
            PARAMETER_HARDNESS => self.hardness,
            PARAMETER_SUSTAIN => self.sustain,
            PARAMETER_BELL => self.bell,
            PARAMETER_DYNAMICS => self.dynamics,
            8 => self.bass_db,
            9 => self.treble_db,
            10 => self.vibrato,
            11 => self.speed_hz,
            12 => self.intensity,
            13 => self.preamp,
            14 => self.bass_boost,
            15 => self.hammer,
            16 => self.tine,
            17 => self.pole,
            18 => self.twist,
            _ => return None,
        })
    }

    pub fn with_parameter(mut self, index: u32, value: f64) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        match index {
            PARAMETER_GAIN => self.gain = value,
            PARAMETER_LAW
                if value.fract() == 0.0 && (0.0..LAW_NAMES.len() as f64).contains(&value) =>
            {
                self.law = value as u8
            }
            PARAMETER_DISTANCE => self.distance_mm = value,
            PARAMETER_ALIGNMENT => self.alignment_mm = value,
            PARAMETER_HARDNESS => self.hardness = value,
            PARAMETER_SUSTAIN => self.sustain = value,
            PARAMETER_BELL => self.bell = value,
            PARAMETER_DYNAMICS => self.dynamics = value,
            8 => self.bass_db = value,
            9 => self.treble_db = value,
            10 => self.vibrato = value,
            11 => self.speed_hz = value,
            12 => self.intensity = value,
            13 => self.preamp = value,
            14 => self.bass_boost = value,
            15 => self.hammer = value,
            16 => self.tine = value,
            17 => self.pole = value,
            18 => self.twist = value,
            _ => return None,
        }
        self.valid().then_some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_map_to_the_retained_profile_and_presets_validate() {
        let default = Settings::default().profile();
        let retained = Profile::default();
        assert_eq!(default.pickup_law, retained.pickup_law);
        assert_eq!(default.pickup_gap_m, retained.pickup_gap_m);
        assert_eq!(default.pickup_offset_m, retained.pickup_offset_m);
        assert_eq!(default.contact_stiffness, retained.contact_stiffness);
        assert_eq!(default.decay_seconds, retained.decay_seconds);
        assert_eq!(
            default.bar_partial_decay_seconds,
            retained.bar_partial_decay_seconds
        );
        assert_eq!(
            default.bar_partial_strike_weight,
            retained.bar_partial_strike_weight
        );
        assert_eq!(default.velocity_exponent, retained.velocity_exponent);
        for (id, _, _, settings) in presets() {
            assert!(settings.valid(), "{id}");
            settings.profile().validate(48_000.0).unwrap();
        }
        let calibrated = presets()[3].3.profile();
        let reference = Profile::calibrated();
        assert!((calibrated.decay_seconds - reference.decay_seconds).abs() < 1e-9);
        assert!(
            (calibrated.bar_partial_decay_seconds - reference.bar_partial_decay_seconds).abs()
                < 1e-9
        );
        assert!(
            (calibrated.bar_partial_strike_weight - reference.bar_partial_strike_weight).abs()
                < 1e-4
        );
        assert_eq!(calibrated.pickup_law, PickupLaw::Aperture);
        // Every parameter extreme keeps the profile inside its validated ranges.
        for index in 0..PARAMETERS as u32 {
            for value in [0.0, 1.0] {
                if let Some(settings) = Settings::default().with_parameter(index, value) {
                    settings.profile().validate(48_000.0).unwrap();
                }
            }
        }
        for (index, value) in [(2, 3.0), (2, 0.5), (3, -1.0), (3, 1.5), (4, 1.0), (5, 1.0)] {
            Settings::default()
                .with_parameter(index, value)
                .unwrap()
                .profile()
                .validate(48_000.0)
                .unwrap();
        }
    }
}
