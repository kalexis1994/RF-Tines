use crate::laboratory::{AxialAperture, PlanarAperture, aperture_voltage, planar_voltage};
use crate::{MagneticPickup, SpatialPickupProfile};
use core::f64::consts::TAU;
use core::fmt;

pub const SAMPLE_RATE_MIN: f64 = 44_100.0;
pub const SAMPLE_RATE_MAX: f64 = 192_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelError(pub &'static str);

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ModelError {}

/// Which transfer law turns tip motion into the pickup voltage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickupLaw {
    /// The original flux surrogate, -0.015 d/dt (1 + z^2)^(-1/2).
    Production,
    /// The laboratory's 16-node finite-aperture flux on the tine axis, with
    /// `pickup_pole_radius_m`; the law of the Close Aperture path.
    Aperture,
    /// Aperture with the frozen upper-register geometry adjustment.
    RegisterAperture,
}

/// Reference tip motion for level compensation: a sine of this amplitude at
/// this frequency, the engine's traced G3 swing at velocity 0.6.
pub const LEVEL_REFERENCE_AMPLITUDE_M: f64 = 0.0004;
pub const LEVEL_REFERENCE_FREQUENCY_HZ: f64 = 196.0;
const LEVEL_REFERENCE_SAMPLES: usize = 256;

/// SI-valued research constants. Defaults are design assumptions, NOT measured
/// Rhodes dimensions. A profile is immutable for the lifetime of a prepared engine.
#[derive(Debug, Clone, Copy)]
pub struct Profile {
    pub hammer_mass_kg: f64,
    pub modal_mass_kg: f64,
    /// F = stiffness * compression^2; units N/m^2.
    pub contact_stiffness: f64,
    pub maximum_hammer_speed_m_s: f64,
    pub pickup_gap_m: f64,
    pub pickup_offset_m: f64,
    /// First-partial T60 at A3; every partial scales with sqrt(220 Hz / f).
    pub decay_seconds: f64,
    /// Second bending partial (the bar partial) T60 at A3.
    pub bar_partial_decay_seconds: f64,
    /// Third bending partial T60 at A3.
    pub third_partial_decay_seconds: f64,
    /// Second bending partial frequency over the fundamental. The uniform
    /// clamped bar gives 6.267; the retained recordings show 6.01.
    pub bar_partial_ratio: f64,
    /// Transfer law of the voice's own pickup.
    pub pickup_law: PickupLaw,
    /// Pole radius of the aperture law; unused by the production law.
    pub pickup_pole_radius_m: f64,
    /// Hammer speed follows normalized velocity to this power.
    pub velocity_exponent: f64,
    /// Second bending partial's displacement at the strike point per unit modal
    /// coordinate, relative to the first partial's 1.0; negative because the
    /// second mode shape is inverted there. Sets how hard the hammer excites it.
    pub bar_partial_strike_weight: f64,
    /// Rotation of the tine's principal bending axes away from the direction the
    /// hammer strikes along.
    ///
    /// A circular tine bends identically in every direction, but its support
    /// does not, so the principal axes belong to the mount rather than to the
    /// wire. Rotated off the strike direction, one vertical blow excites both
    /// of them; because they do not run at quite the same frequency the tip
    /// traces a slowly turning ellipse rather than staying on a line. Zero
    /// keeps the tip on one axis, which is the single-coordinate motion every
    /// profile before this one had.
    pub tine_boundary_angle_rad: f64,
    /// The second principal axis's frequency over the first. The offline
    /// polarized assembly carries the same idea as separate support, rotation
    /// and tonebar ratios; this is the one number a three-mode voice can
    /// express. At exactly one the axes are degenerate, nothing turns, and the
    /// second axis cannot be told from the first however hard it is driven.
    pub tine_transverse_frequency_ratio: f64,
    /// Where the tine rests along the pickup's second coordinate. Only the
    /// two-dimensional flux sees it; on the axis the reduction is exact and
    /// this is zero.
    pub pickup_transverse_offset_m: f64,
    /// Stiffness of the block joining the two prongs of the fork, relative to
    /// the tine's own. Zero leaves the tine alone, which is every voicing
    /// that predates the tonebar; above it the prongs trade energy and the
    /// fundamental stops decaying as one exponential.
    pub tonebar_coupling: f64,
    /// The tonebar's own lowest frequency over the tine's, before coupling.
    /// The prongs of this fork are deliberately unequal: Muenster and Pfeifle
    /// measure their fundamentals several hundred to more than 1400 cents
    /// apart, which is a ratio between about 1.19 and 2.24.
    pub tonebar_frequency_ratio: f64,
    /// The tonebar's effective mass over the tine's. A brass bar against a
    /// steel wire is the heavier prong, so this is above one; how far above
    /// is an assumption, not a measurement.
    pub tonebar_mass_ratio: f64,
    /// T60 of the tonebar alone at A3. The measurements call it the more
    /// heavily damped prong, so this is shorter than `decay_seconds`.
    pub tonebar_decay_seconds: f64,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            hammer_mass_kg: 0.004,
            modal_mass_kg: 0.0015,
            contact_stiffness: 4.0e10,
            maximum_hammer_speed_m_s: 0.8,
            pickup_gap_m: 0.0015,
            pickup_offset_m: 0.0005,
            decay_seconds: 5.0,
            bar_partial_decay_seconds: 0.16,
            third_partial_decay_seconds: 0.055,
            bar_partial_ratio: 6.267,
            bar_partial_strike_weight: -0.3,
            pickup_law: PickupLaw::Production,
            pickup_pole_radius_m: 0.002,
            velocity_exponent: 1.4,
            // The tine stays on one axis unless a profile asks otherwise, so
            // every voicing that predates the second coordinate renders exactly
            // as it did.
            tine_boundary_angle_rad: 0.0,
            tine_transverse_frequency_ratio: 1.0,
            pickup_transverse_offset_m: 0.0,
            // Uncoupled, so the tine rings alone exactly as it did before the
            // second prong existed. The rest describe the prong that is
            // waiting: a heavier, shorter-ringing bar a fifth or so above.
            tonebar_coupling: 0.0,
            tonebar_frequency_ratio: 1.5,
            tonebar_mass_ratio: 8.0,
            tonebar_decay_seconds: 4.0,
        }
    }
}

/// Named profiles the plugin exposes for audition, in parameter order.
pub const PROFILE_NAMES: [&str; 3] = ["Original", "Calibrated Sustain", "Calibrated"];

impl Profile {
    /// Resolve the audition voicing once per voice/profile update, not per sample.
    pub fn for_note(self, note: u8) -> Self {
        if self.pickup_law != PickupLaw::RegisterAperture {
            return self;
        }
        let t = ((f64::from(note) - 55.0) / 17.0).clamp(0.0, 1.0);
        let w = t * t * (3.0 - 2.0 * t);
        Self {
            pickup_law: PickupLaw::Aperture,
            pickup_gap_m: self.pickup_gap_m * (w * 0.135).exp(),
            pickup_offset_m: self.pickup_offset_m * (w * (-0.020)).exp(),
            ..self
        }
    }
    /// The named profile at `index` in `PROFILE_NAMES` order.
    pub fn named(index: usize) -> Option<Self> {
        Some(match index {
            0 => Self::default(),
            1 => Self::calibrated_sustain(),
            2 => Self::calibrated(),
            _ => return None,
        })
    }

    /// The default profile with the first and bar partial T60 set from the
    /// retained D3, G3 and B3 recordings' sustain slopes (docs/PLAYABLE-SUSTAIN.md):
    /// 20 s and 2.3 s at A3 under the shared sqrt(220 Hz / f) pitch scaling.
    /// Mechanics, pickup and the third partial are unchanged.
    pub fn calibrated_sustain() -> Self {
        Self {
            decay_seconds: 20.0,
            bar_partial_decay_seconds: 2.3,
            ..Self::default()
        }
    }

    /// The calibrated sustain with the second bending partial's strike weight
    /// reduced to -0.02: the retained recordings bound that partial 20 to 35 dB
    /// below the default's excitation at medium and soft dynamics, and their
    /// line at six times the fundamental is the pickup's sixth harmonic
    /// (docs/PLAYABLE-BAR-PARTIAL.md). Ratio, contact and pickup are unchanged.
    pub fn calibrated() -> Self {
        Self {
            bar_partial_strike_weight: -0.02,
            ..Self::calibrated_sustain()
        }
    }

    /// Whether the tine leaves the pickup's axis at all.
    ///
    /// It does not when the principal axes line up with the strike, because
    /// then the second axis is never driven, and it does not when the two axes
    /// are degenerate, because then the pair moves as one line. Either way the
    /// transverse coordinate stays at zero for the life of the note and the
    /// axial reduction is not an approximation but the same number, computed in
    /// ten square roots instead of sixteen.
    pub(crate) fn leaves_the_axis(&self) -> bool {
        self.pickup_transverse_offset_m != 0.0
            || (self.tine_boundary_angle_rad.sin() != 0.0
                && self.tine_transverse_frequency_ratio != 1.0)
    }

    pub(crate) fn spatial_pickup(&self) -> SpatialPickupProfile {
        SpatialPickupProfile {
            gap_m: self.pickup_gap_m,
            offset_xy_m: [self.pickup_offset_m, self.pickup_transverse_offset_m],
            pole_radius_m: self.pickup_pole_radius_m,
            flux_scale_wb: 0.001,
        }
    }

    /// The aperture pickup of this profile's geometry, when its law is Aperture
    /// and the tip stays on the axis that reduction assumes.
    pub(crate) fn aperture(&self) -> Option<AxialAperture> {
        (matches!(
            self.pickup_law,
            PickupLaw::Aperture | PickupLaw::RegisterAperture
        ) && !self.leaves_the_axis())
        .then(|| {
            AxialAperture::new(SpatialPickupProfile {
                offset_xy_m: [self.pickup_offset_m, 0.0],
                ..self.spatial_pickup()
            })
            .expect("validated aperture geometry")
        })
    }

    /// The same flux kept in two dimensions, for a tip that traces an ellipse.
    pub(crate) fn planar_aperture(&self) -> Option<PlanarAperture> {
        (matches!(
            self.pickup_law,
            PickupLaw::Aperture | PickupLaw::RegisterAperture
        ) && self.leaves_the_axis())
        .then(|| PlanarAperture::new(self.spatial_pickup()).expect("validated aperture geometry"))
    }

    /// RMS pickup voltage for the reference tip motion, a sine of
    /// `LEVEL_REFERENCE_AMPLITUDE_M` at `LEVEL_REFERENCE_FREQUENCY_HZ`, sampled
    /// over one period. A pure function of the pickup law and geometry.
    pub fn pickup_sensitivity(&self) -> f64 {
        let pickup = MagneticPickup::from_validated_profile(*self);
        // The reference motion is a sine along the strike direction, but it is
        // measured through whichever law the voice will run, at the geometry
        // the voice will run it at. A two-plane profile that fell back to the
        // production law here would be compensated to the wrong level.
        let aperture = self.aperture();
        let planar = self.planar_aperture();
        let omega = TAU * LEVEL_REFERENCE_FREQUENCY_HZ;
        let mut sum = 0.0;
        for i in 0..LEVEL_REFERENCE_SAMPLES {
            let phase = TAU * i as f64 / LEVEL_REFERENCE_SAMPLES as f64;
            let x = LEVEL_REFERENCE_AMPLITUDE_M * phase.sin();
            let v = LEVEL_REFERENCE_AMPLITUDE_M * omega * phase.cos();
            let voltage = match (&aperture, &planar) {
                (Some(aperture), _) => aperture_voltage(aperture, x, v),
                (None, Some(planar)) => planar_voltage(planar, [x, 0.0], [v, 0.0]),
                (None, None) => pickup.voltage(x, v),
            };
            sum += voltage * voltage;
        }
        (sum / LEVEL_REFERENCE_SAMPLES as f64).sqrt()
    }

    /// Gain that brings this profile's reference-motion RMS to the default
    /// profile's. Exactly 1 for the default pickup, so retained renders are
    /// unchanged; it equalizes a medium note, not the timbre.
    pub fn level_compensation(&self) -> f64 {
        Self::default().pickup_sensitivity() / self.pickup_sensitivity()
    }

    pub fn validate(self, sample_rate: f64) -> Result<(), ModelError> {
        if !sample_rate.is_finite() || !(SAMPLE_RATE_MIN..=SAMPLE_RATE_MAX).contains(&sample_rate) {
            return Err(ModelError(
                "sample rate must be finite and between 44100 and 192000 Hz",
            ));
        }
        for (value, minimum, maximum, error) in [
            (
                self.hammer_mass_kg,
                0.001,
                0.02,
                "hammer mass outside 0.001..0.02 kg",
            ),
            (
                self.modal_mass_kg,
                0.0001,
                0.01,
                "modal mass outside 0.0001..0.01 kg",
            ),
            (
                self.contact_stiffness,
                1.0e8,
                1.0e12,
                "contact stiffness outside 1e8..1e12 N/m^2",
            ),
            (
                self.maximum_hammer_speed_m_s,
                0.1,
                3.0,
                "hammer speed outside 0.1..3 m/s",
            ),
            (
                self.pickup_gap_m,
                0.0005,
                0.005,
                "pickup gap outside 0.0005..0.005 m",
            ),
            (
                self.pickup_offset_m,
                -0.003,
                0.003,
                "pickup offset outside -0.003..0.003 m",
            ),
            (
                self.decay_seconds,
                0.25,
                80.0,
                "decay outside 0.25..80 seconds",
            ),
            (
                self.bar_partial_decay_seconds,
                0.01,
                10.0,
                "bar partial decay outside 0.01..10 seconds",
            ),
            (
                self.third_partial_decay_seconds,
                0.005,
                5.0,
                "third partial decay outside 0.005..5 seconds",
            ),
            (
                self.bar_partial_ratio,
                2.0,
                12.0,
                "bar partial ratio outside 2..12",
            ),
            (
                self.bar_partial_strike_weight,
                -1.0,
                1.0,
                "bar partial strike weight outside -1..1",
            ),
            (
                self.pickup_pole_radius_m,
                0.0002,
                0.003,
                "pickup pole radius outside 0.0002..0.003 m",
            ),
            (
                self.velocity_exponent,
                0.5,
                3.0,
                "velocity exponent outside 0.5..3",
            ),
            (
                self.tine_boundary_angle_rad,
                -TAU,
                TAU,
                "tine boundary angle outside -TAU..TAU",
            ),
            (
                self.tine_transverse_frequency_ratio,
                0.25,
                4.0,
                "tine transverse frequency ratio outside 0.25..4",
            ),
            (
                self.pickup_transverse_offset_m,
                -0.003,
                0.003,
                "pickup transverse offset outside -0.003..0.003 m",
            ),
            (
                self.tonebar_coupling,
                0.0,
                4.0,
                "tonebar coupling outside 0..4",
            ),
            (
                self.tonebar_frequency_ratio,
                0.25,
                4.0,
                "tonebar frequency ratio outside 0.25..4",
            ),
            (
                self.tonebar_mass_ratio,
                0.1,
                100.0,
                "tonebar mass ratio outside 0.1..100",
            ),
            (
                self.tonebar_decay_seconds,
                0.05,
                80.0,
                "tonebar decay outside 0.05..80 seconds",
            ),
        ] {
            if !value.is_finite() || !(minimum..=maximum).contains(&value) {
                return Err(ModelError(error));
            }
        }
        if self.pickup_law == PickupLaw::RegisterAperture {
            self.for_note(72).validate(sample_rate)?;
        }
        Ok(())
    }
}
