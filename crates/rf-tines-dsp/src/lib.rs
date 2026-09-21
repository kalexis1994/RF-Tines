//! Provisional RF-Tines research engine. Physical plausibility is not calibration.
//! Construction prepares all memory; rendering uses fixed-size state only.
mod assembly;
mod filter;
mod hammer_memory;
mod laboratory;
mod memory_hammer;
pub use memory_hammer::{MemoryFreeStatus, MemoryFreeStep};
mod modal_assembly;
pub use modal_assembly::RestPreparation;
pub use modal_assembly::{
    ActionAssembly, ActionProbe, ActionProfile, DamperDrive, FeltDamperAssembly, FeltDamperProbe,
    FeltDamperProfile, MemoryContactInspection, MemoryContactStatus, MemoryContactStep,
    MemoryModalAssembly, MemoryModalCheckpoint, MemoryModalProbe, MemoryModalRk4Step,
};
pub use modal_assembly::{
    ElectromechanicalAssembly, ElectromechanicalProbe, ElectromechanicalProfile,
    PickupCircuitProfile, SpatialPickup, SpatialPickupProfile,
};
pub use modal_assembly::{PolarizationProfile, PolarizedActionAssembly, PolarizedActionProbe};
mod model;
mod pickup;
mod tine;
mod voice;

pub use assembly::{AssemblyParameters, AssemblyProbe, AssemblyVoice};
pub use filter::Decimator as ProductionDecimator;
pub use hammer_memory::{HammerMemory, HammerMemoryProbe, HammerMemoryProfile};
pub use laboratory::{
    APERTURE_PICKUP, AxialAperture, PICKUP_LEVEL_MATCH, PICKUP_NAMES, PlanarAperture,
    aperture_voltage, planar_voltage,
};
pub use memory_hammer::{
    MemoryHammer, MemoryHammerContactStatus, MemoryHammerContactStep, MemoryHammerProbe,
    MemoryHammerProfile,
};
pub use modal_assembly::{
    ModalAssembly, ModalAssemblyProfile, ModalIntegration, ModalProbe, ModalSpectrum,
    StructuralMode,
};
pub use model::{
    LEVEL_REFERENCE_AMPLITUDE_M, LEVEL_REFERENCE_FREQUENCY_HZ, ModelError, PROFILE_NAMES,
    PickupLaw, Profile, SAMPLE_RATE_MAX, SAMPLE_RATE_MIN,
};
pub use pickup::MagneticPickup;
pub use tine::{TINE_MODE_COUNT, TineGeometry, TineMode, TineModes};
pub use voice::{Probe, Voice};

pub const FIRST_NOTE: u8 = 28;
pub const LAST_NOTE: u8 = 100;
pub const KEY_COUNT: usize = (LAST_NOTE - FIRST_NOTE + 1) as usize;
pub const OVERSAMPLE: usize = 4;

/// One shared mechanical key per pitch, with channel-aware key/pedal ownership.
/// Multiple MIDI channels do not create extra copies of the same physical tine.
pub struct Engine {
    voices: [Voice; KEY_COUNT],
    held: [u16; KEY_COUNT],
    sustained: [u16; KEY_COUNT],
    pedals: u16,
    pitch_bends: [f64; 16],
    decimator: filter::Decimator,
    laboratory: Option<laboratory::Laboratory>,
    sample_rate: f64,
    gain: f64,
    target_gain: f64,
    gain_step: f64,
    profile: Profile,
    compensating: bool,
    compensation: f64,
    target_compensation: f64,
    faults: u64,
}

impl Engine {
    pub fn new(sample_rate: f64, profile: Profile) -> Result<Self, ModelError> {
        profile.validate(sample_rate)?;
        let voices = core::array::from_fn(|i| {
            Voice::new_validated(sample_rate, FIRST_NOTE + i as u8, profile)
        });
        Ok(Self {
            voices,
            held: [0; KEY_COUNT],
            sustained: [0; KEY_COUNT],
            pedals: 0,
            pitch_bends: [0.0; 16],
            decimator: filter::Decimator::new(),
            laboratory: None,
            sample_rate,
            gain: 0.7,
            target_gain: 0.7,
            gain_step: 1.0 - (-1.0 / (0.005 * sample_rate)).exp(),
            profile,
            compensating: false,
            compensation: 1.0,
            target_compensation: 1.0,
            faults: 0,
        })
    }

    /// Four continuously filtered pickups on one mechanical instrument.
    /// Fixed level matching is tied to the documented default/close geometries.
    pub fn new_laboratory(sample_rate: f64) -> Result<Self, ModelError> {
        Self::new_laboratory_with(sample_rate, Profile::default())
    }

    /// The four matched pickups over a chosen mechanical profile. The level
    /// factors were frozen on the default profile's pickup geometry, so the
    /// profile must keep it; sustain and contact fields are free.
    pub fn new_laboratory_with(sample_rate: f64, profile: Profile) -> Result<Self, ModelError> {
        let default = Profile::default();
        if profile.pickup_gap_m != default.pickup_gap_m
            || profile.pickup_offset_m != default.pickup_offset_m
            || profile.pickup_law != default.pickup_law
        {
            return Err(ModelError(
                "laboratory level matching needs the default pickup geometry and law",
            ));
        }
        let mut engine = Self::new(sample_rate, profile)?;
        engine.laboratory = Some(laboratory::Laboratory::new(sample_rate));
        Ok(engine)
    }

    /// Replace the mechanical profile of every voice while notes keep ringing:
    /// modal and hammer states are kept, only frequencies, losses, strike weights
    /// and contact law change. A laboratory engine keeps its frozen pickup
    /// geometry, so a profile that moves it is rejected.
    pub fn set_profile(&mut self, profile: Profile) -> bool {
        if profile.validate(self.sample_rate).is_err() {
            return false;
        }
        let default = Profile::default();
        if self.laboratory.is_some()
            && (profile.pickup_gap_m != default.pickup_gap_m
                || profile.pickup_offset_m != default.pickup_offset_m
                || profile.pickup_law != default.pickup_law)
        {
            return false;
        }
        for voice in &mut self.voices {
            voice.set_profile(profile);
        }
        self.profile = profile;
        self.target_compensation = if self.compensating {
            profile.level_compensation()
        } else {
            1.0
        };
        true
    }

    /// Enable the pickup level compensation, `Profile::level_compensation` of
    /// the current profile, smoothed like the gain; off by default so the raw
    /// engine keeps its retained output at every geometry. `reset` jumps to it.
    pub fn set_level_compensation(&mut self, enabled: bool) {
        self.compensating = enabled;
        self.target_compensation = if enabled {
            self.profile.level_compensation()
        } else {
            1.0
        };
    }

    /// The pickup level compensation currently applied, smoothed like the gain.
    pub fn level_compensation(&self) -> f64 {
        self.compensation
    }

    /// Select a matched pickup with a 20 ms linear crossfade. No voice resets.
    pub fn set_pickup(&mut self, index: usize) -> bool {
        self.laboratory
            .as_mut()
            .is_some_and(|lab| lab.select(index))
    }

    pub fn set_gain(&mut self, gain: f64) -> bool {
        if !gain.is_finite() || !(0.0..=2.0).contains(&gain) {
            return false;
        }
        self.target_gain = gain;
        true
    }

    pub fn note_on(&mut self, channel: u8, note: u8, velocity: f64) -> bool {
        let Some(i) = key_index(channel, note) else {
            return false;
        };
        if !velocity.is_finite() || !(0.0..=1.0).contains(&velocity) {
            return false;
        }
        if velocity == 0.0 {
            return self.note_off(channel, note);
        }
        let bit = 1 << channel;
        self.held[i] |= bit;
        self.sustained[i] &= !bit;
        self.voices[i].last_channel = channel;
        self.voices[i].set_pitch_ratio(pitch_ratio(self.pitch_bends[channel as usize]));
        self.voices[i].strike(velocity);
        true
    }

    pub fn note_off(&mut self, channel: u8, note: u8) -> bool {
        let Some(i) = key_index(channel, note) else {
            return false;
        };
        let bit = 1 << channel;
        if self.held[i] & bit != 0 && self.pedals & bit != 0 {
            self.sustained[i] |= bit;
        }
        self.held[i] &= !bit;
        self.update_damper(i);
        true
    }

    pub fn control_change(&mut self, channel: u8, controller: u8, value: f64) -> bool {
        if channel >= 16 || !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return false;
        }
        let bit = 1 << channel;
        match controller {
            64 => {
                if value >= 0.5 {
                    self.pedals |= bit;
                    // A late pedal catches a still-vibrating released tine.
                    for i in 0..KEY_COUNT {
                        if self.voices[i].is_active() && self.voices[i].last_channel == channel {
                            self.sustained[i] |= bit;
                            self.update_damper(i);
                        }
                    }
                } else {
                    self.pedals &= !bit;
                    for i in 0..KEY_COUNT {
                        self.sustained[i] &= !bit;
                        self.update_damper(i);
                    }
                }
            }
            123 => {
                for note in FIRST_NOTE..=LAST_NOTE {
                    self.note_off(channel, note);
                }
            }
            120 => {
                for i in 0..KEY_COUNT {
                    let owned = (self.held[i] | self.sustained[i]) & bit != 0;
                    self.held[i] &= !bit;
                    self.sustained[i] &= !bit;
                    // Released tails have no ownership, but still belong to this
                    // channel's last strike unless another channel holds the key.
                    if (owned || self.voices[i].last_channel == channel)
                        && (self.held[i] | self.sustained[i]) == 0
                    {
                        self.voices[i].reset();
                    }
                    self.update_damper(i);
                }
            }
            121 => {
                self.pedals &= !bit;
                for i in 0..KEY_COUNT {
                    self.sustained[i] &= !bit;
                    self.update_damper(i);
                }
                self.pitch_bend(channel, 0.0);
            }
            _ => return false,
        }
        true
    }

    /// Apply the channel wheel to ringing tails and future strikes. The wheel
    /// spans two semitones in each direction; zero is exact center.
    pub fn pitch_bend(&mut self, channel: u8, normalized: f64) -> bool {
        if channel >= 16 || !normalized.is_finite() || !(-1.0..=1.0).contains(&normalized) {
            return false;
        }
        self.pitch_bends[channel as usize] = normalized;
        let ratio = pitch_ratio(normalized);
        for voice in &mut self.voices {
            if voice.is_active() && voice.last_channel == channel {
                voice.set_pitch_ratio(ratio);
            }
        }
        true
    }

    fn update_damper(&mut self, i: usize) {
        self.voices[i].set_damped((self.held[i] | self.sustained[i]) == 0);
    }

    /// A mono direct signal, after a 127-tap antialias filter. No limiter,
    /// normalization, reverb or amplifier coloration is hidden here.
    pub fn next_sample(&mut self) -> f32 {
        for _ in 0..OVERSAMPLE {
            let mut sum = 0.0;
            let mut close = [0.0; 3];
            for voice in &mut self.voices {
                sum += voice.tick();
                if let Some(lab) = &self.laboratory
                    && voice.is_active()
                {
                    // The laboratory's comparison paths are one-coordinate
                    // laws, so they see the motion along the strike direction.
                    let (q, v) = voice.tip();
                    close[0] += lab.pickup.voltage(q[0], v[0]);
                    close[1] += lab.pickup.research_point_pole_voltage(q[0], v[0]);
                    close[2] += laboratory::aperture_voltage(&lab.aperture, q[0], v[0]);
                }
            }
            if !sum.is_finite() || close.iter().any(|value| !value.is_finite()) {
                self.reset();
                self.faults = self.faults.saturating_add(1);
                return 0.0;
            }
            self.decimator.push(sum);
            if let Some(lab) = &mut self.laboratory {
                lab.push(close);
            }
        }
        self.gain += self.gain_step * (self.target_gain - self.gain);
        self.compensation += self.gain_step * (self.target_compensation - self.compensation);
        let mut signal = self.decimator.output();
        if let Some(lab) = &mut self.laboratory {
            signal = lab.mix(signal);
        }
        (signal * self.gain * self.compensation * 0.12) as f32
    }

    pub fn probe(&self, note: u8) -> Option<Probe> {
        let i = key_index(0, note)?;
        Some(self.voices[i].probe())
    }

    pub fn faults(&self) -> u64 {
        self.faults
    }

    pub fn reset(&mut self) {
        for voice in &mut self.voices {
            voice.reset();
        }
        self.held.fill(0);
        self.sustained.fill(0);
        self.pedals = 0;
        self.pitch_bends.fill(0.0);
        for voice in &mut self.voices {
            voice.set_pitch_ratio(1.0);
        }
        self.decimator.clear();
        if let Some(lab) = &mut self.laboratory {
            lab.reset();
        }
        self.gain = self.target_gain;
        self.compensation = self.target_compensation;
    }
}

fn pitch_ratio(normalized: f64) -> f64 {
    2.0_f64.powf((2.0 * normalized) / 12.0)
}

fn key_index(channel: u8, note: u8) -> Option<usize> {
    (channel < 16 && (FIRST_NOTE..=LAST_NOTE).contains(&note)).then(|| (note - FIRST_NOTE) as usize)
}
