use crate::laboratory::{AxialAperture, PlanarAperture, aperture_voltage, planar_voltage};
use crate::{FIRST_NOTE, LAST_NOTE, MagneticPickup, ModelError, OVERSAMPLE, Profile};
use core::f64::consts::TAU;

const MODES: usize = 3;
/// The two principal bending directions of the tine. A circular wire bends the
/// same way in both; its support does not, which is what separates them.
const AXES: usize = 2;
/// Every bending mode of both axes, laid out as `axis * MODES + mode`.
const COORDINATES: usize = MODES * AXES;
const PICKUP_WEIGHTS: [f64; MODES] = [1.0, 0.8, 0.6];

#[derive(Debug, Clone, Copy, Default)]
pub struct Probe {
    pub displacement_m: f64,
    pub velocity_m_s: f64,
    /// Tip motion across the strike direction. Zero unless the profile rotates
    /// the tine's principal axes off it.
    pub transverse_displacement_m: f64,
    pub contact_force_n: f64,
    pub mechanical_energy_j: f64,
    pub pickup_signal: f64,
    pub contact_active: bool,
}

#[derive(Clone, Copy)]
struct Mode {
    q: f64,
    v: f64,
    mass: f64,
    omega: f64,
    gamma: f64,
    free: [[f64; 4]; 2],
    contact_free: [[f64; 4]; 2],
}

impl Mode {
    fn new(frequency: f64, mass: f64, gamma: f64, dt: f64, contact_dt: f64) -> Self {
        let mut mode = Self {
            q: 0.0,
            v: 0.0,
            mass,
            omega: 0.0,
            gamma,
            free: [[0.0; 4]; 2],
            contact_free: [[0.0; 4]; 2],
        };
        mode.set(frequency, mass, gamma, dt, contact_dt);
        mode
    }

    /// Replace frequency, mass and loss while keeping the modal state.
    fn set(&mut self, frequency: f64, mass: f64, gamma: f64, dt: f64, contact_dt: f64) {
        let omega = TAU * frequency;
        let transitions = |dt: f64| {
            [gamma, gamma + 55.0].map(|g| {
                let wd = (omega * omega - g * g).sqrt();
                let (sin, cos) = (wd * dt).sin_cos();
                let envelope = (-g * dt).exp();
                let s = sin / wd;
                [
                    envelope * (cos + g * s),
                    envelope * s,
                    -envelope * omega * omega * s,
                    envelope * (cos - g * s),
                ]
            })
        };
        self.mass = mass;
        self.omega = omega;
        self.gamma = gamma;
        self.free = transitions(dt);
        self.contact_free = transitions(contact_dt);
    }

    fn advance_free(&mut self, damped: bool) {
        self.advance(self.free[usize::from(damped)]);
    }

    fn advance_contact_remainder(&mut self, damped: bool) {
        self.advance(self.contact_free[usize::from(damped)]);
    }

    fn advance(&mut self, [a, b, c, d]: [f64; 4]) {
        let q = a * self.q + b * self.v;
        self.v = c * self.q + d * self.v;
        self.q = q;
    }

    fn energy(&self) -> f64 {
        0.5 * self.mass * (self.v * self.v + self.omega * self.omega * self.q * self.q)
    }
}

/// Research voice: three provisional cantilever modes, nonlinear elastic
/// hammer contact, and an analytic flux surrogate. No measured tonebar fit yet.
pub struct Voice {
    modes: [Mode; COORDINATES],
    /// Each coordinate's displacement at the strike point per unit modal
    /// coordinate, already carrying the projection of its own axis onto the
    /// direction the hammer travels. The second axis's entries are zero when
    /// the principal axes line up with the strike, which is how a profile that
    /// predates the second coordinate keeps it silent.
    hammer_weights: [f64; COORDINATES],
    /// The same for the pickup, resolved into the two laboratory directions:
    /// along the strike, and across it.
    pickup_weights: [[f64; COORDINATES]; 2],
    note: u8,
    profile: Profile,
    pickup: MagneticPickup,
    aperture: Option<AxialAperture>,
    planar: Option<PlanarAperture>,
    pitch_ratio: f64,
    dt: f64,
    contact_steps: usize,
    hammer_mass: f64,
    hammer_x: f64,
    hammer_v: f64,
    contact: bool,
    damped: bool,
    active: bool,
    force: f64,
    signal: f64,
    pub(crate) last_channel: u8,
}

impl Voice {
    pub fn new(sample_rate: f64, note: u8, profile: Profile) -> Result<Self, ModelError> {
        profile.validate(sample_rate)?;
        if !(FIRST_NOTE..=LAST_NOTE).contains(&note) {
            return Err(ModelError("note must be in the 73-key range, MIDI 28..100"));
        }
        Ok(Self::prepare(sample_rate, note, profile, OVERSAMPLE, true))
    }

    /// Prepare a standalone research voice at a power of two from 4 to 256 internal steps
    /// per output sample, without production contact refinement. The production
    /// pickup/decimator runs at 4x, with finer contact steps where required.
    /// Call `tick` exactly `substeps` times per output frame; this voice does not
    /// perform antialias filtering or decimation itself.
    pub fn new_for_convergence(
        sample_rate: f64,
        note: u8,
        profile: Profile,
        substeps: usize,
    ) -> Result<Self, ModelError> {
        profile.validate(sample_rate)?;
        if !(4..=256).contains(&substeps) || !substeps.is_power_of_two() {
            return Err(ModelError(
                "research substeps must be a power of two from 4 to 256",
            ));
        }
        if !(FIRST_NOTE..=LAST_NOTE).contains(&note) {
            return Err(ModelError("note must be in the 73-key range, MIDI 28..100"));
        }
        Ok(Self::prepare(sample_rate, note, profile, substeps, false))
    }

    pub(crate) fn new_validated(sample_rate: f64, note: u8, profile: Profile) -> Self {
        Self::prepare(sample_rate, note, profile, OVERSAMPLE, true)
    }

    fn prepare(
        sample_rate: f64,
        note: u8,
        profile: Profile,
        substeps: usize,
        refine_contact: bool,
    ) -> Self {
        let profile = profile.for_note(note);
        let frequency = 440.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0);
        let dt = 1.0 / (sample_rate * substeps as f64);
        let scale = (220.0 / frequency).clamp(0.15, 4.0);
        // Uniform cantilever ratios by default; the second is a profile field.
        let ratios = [1.0, profile.bar_partial_ratio, 17.55];
        // Bound the fastest mode's phase advance during midpoint contact.
        // At supported rates/notes this requires at most 16 bounded microsteps.
        let contact_steps = if refine_contact {
            ((TAU * frequency * ratios[2] * dt / 0.2).ceil() as usize)
                .max(1)
                .next_power_of_two()
                .min(16)
        } else {
            1
        };
        let t60 = [
            profile.decay_seconds * scale.sqrt(),
            profile.bar_partial_decay_seconds * scale.sqrt(),
            profile.third_partial_decay_seconds * scale.sqrt(),
        ];
        let modes = core::array::from_fn(|c| {
            let (axis, i) = (c / MODES, c % MODES);
            Mode::new(
                frequency * ratios[i] * axis_frequency_ratio(profile, axis),
                profile.modal_mass_kg * scale,
                1000.0_f64.ln() / t60[i],
                dt,
                dt / contact_steps as f64,
            )
        });
        let (hammer_weights, pickup_weights) = resolve_axes(profile);
        Self {
            modes,
            hammer_weights,
            pickup_weights,
            note,
            profile,
            pickup: MagneticPickup::from_validated_profile(profile),
            aperture: profile.aperture(),
            planar: profile.planar_aperture(),
            pitch_ratio: 1.0,
            dt,
            contact_steps,
            hammer_mass: profile.hammer_mass_kg * scale.sqrt(),
            hammer_x: 0.0,
            hammer_v: 0.0,
            contact: false,
            damped: true,
            active: false,
            force: 0.0,
            signal: 0.0,
            last_channel: 0,
        }
    }

    /// Replace the mechanical profile while every modal and hammer state is kept.
    /// The contact subdivision stays as prepared; it depends on the fixed third
    /// ratio only. The caller validates the profile for its sample rate.
    pub(crate) fn set_profile(&mut self, profile: Profile) {
        let profile = profile.for_note(self.note);
        let base_frequency = 440.0 * 2.0_f64.powf((self.note as f64 - 69.0) / 12.0);
        let frequency = base_frequency * self.pitch_ratio;
        let scale = (220.0 / base_frequency).clamp(0.15, 4.0);
        let ratios = [1.0, profile.bar_partial_ratio, 17.55];
        let t60 = [
            profile.decay_seconds * scale.sqrt(),
            profile.bar_partial_decay_seconds * scale.sqrt(),
            profile.third_partial_decay_seconds * scale.sqrt(),
        ];
        for (c, mode) in self.modes.iter_mut().enumerate() {
            let (axis, i) = (c / MODES, c % MODES);
            mode.set(
                frequency * ratios[i] * axis_frequency_ratio(profile, axis),
                profile.modal_mass_kg * scale,
                1000.0_f64.ln() / t60[i],
                self.dt,
                self.dt / self.contact_steps as f64,
            );
        }
        (self.hammer_weights, self.pickup_weights) = resolve_axes(profile);
        self.hammer_mass = profile.hammer_mass_kg * scale.sqrt();
        self.pickup = MagneticPickup::from_validated_profile(profile);
        self.aperture = profile.aperture();
        self.planar = profile.planar_aperture();
        self.profile = profile;
    }

    /// Retune every structural mode while retaining displacement, velocity,
    /// mass, loss, hammer state and pickup geometry. This models a performer
    /// pitch gesture as a temporary stiffness/tuning change rather than moving
    /// the hammer or pickup.
    pub fn set_pitch_ratio(&mut self, ratio: f64) -> bool {
        if !ratio.is_finite() || !(0.5..=2.0).contains(&ratio) {
            return false;
        }
        self.pitch_ratio = ratio;
        let frequency = 440.0 * 2.0_f64.powf((self.note as f64 - 69.0) / 12.0) * ratio;
        let ratios = [1.0, self.profile.bar_partial_ratio, 17.55];
        for (c, mode) in self.modes.iter_mut().enumerate() {
            let (axis, i) = (c / MODES, c % MODES);
            mode.set(
                frequency * ratios[i] * axis_frequency_ratio(self.profile, axis),
                mode.mass,
                mode.gamma,
                self.dt,
                self.dt / self.contact_steps as f64,
            );
        }
        true
    }

    /// Retriggering retains every resonator coordinate and its velocity.
    pub fn strike(&mut self, velocity: f64) -> bool {
        if !velocity.is_finite() || !(0.0..=1.0).contains(&velocity) || velocity == 0.0 {
            return false;
        }
        self.hammer_x = self.contact_position();
        self.hammer_v =
            self.profile.maximum_hammer_speed_m_s * velocity.powf(self.profile.velocity_exponent);
        self.contact = true;
        self.damped = false;
        self.active = true;
        true
    }

    pub fn set_damped(&mut self, damped: bool) {
        self.damped = damped;
    }

    /// Prepared contact-only subdivision count; free motion keeps the base rate.
    pub fn contact_substeps(&self) -> usize {
        self.contact_steps
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    /// Advance ONE internal sample (4x by default, or the research substep rate).
    pub fn tick(&mut self) -> f64 {
        if !self.active {
            return 0.0;
        }
        self.force = 0.0;
        if self.contact {
            let mut force_sum = 0.0;
            for _ in 0..self.contact_steps {
                if self.contact {
                    self.advance_contact(self.dt / self.contact_steps as f64);
                    force_sum += self.force;
                } else {
                    // Complete the same physical interval after a separation
                    // inside this tick; never advance a full extra base step.
                    for mode in &mut self.modes {
                        mode.advance_contact_remainder(self.damped);
                    }
                }
            }
            self.force = force_sum / self.contact_steps as f64;
        } else {
            for mode in &mut self.modes {
                mode.advance_free(self.damped);
            }
        }
        let (position, velocity) = self.tip();
        // Smooth, bounded flux linkage surrogate. Gap never reaches zero.
        // Phi = 1 / sqrt(1 + ((offset + position) / gap)^2).
        // Output follows -dPhi/dt, not displacement and not a post-mix clipper.
        // A tip that has left the axis changes the flux through both of its
        // coordinates, so both terms of the chain rule are kept.
        self.signal = match (&self.aperture, &self.planar) {
            (Some(aperture), _) => aperture_voltage(aperture, position[0], velocity[0]),
            (None, Some(planar)) => planar_voltage(planar, position, velocity),
            (None, None) => self.pickup.voltage(position[0], velocity[0]),
        };
        if !self.contact && self.modes.iter().map(Mode::energy).sum::<f64>() < 1e-18 {
            self.reset();
        }
        self.signal
    }

    fn advance_contact(&mut self, h: f64) {
        let delta0 = self.hammer_x - self.contact_position();
        let mut free_q = [0.0; COORDINATES];
        let mut free_v = [0.0; COORDINATES];
        let mut response_v = [0.0; COORDINATES];
        let mut delta_free = self.hammer_x + h * self.hammer_v;
        let mut compliance = h * h / (2.0 * self.hammer_mass);
        for (i, mode) in self.modes.iter().enumerate() {
            let gamma = mode.gamma + if self.damped { 55.0 } else { 0.0 };
            let denominator = 1.0 + h * gamma + 0.25 * h * h * mode.omega * mode.omega;
            free_v[i] = ((1.0 - h * gamma - 0.25 * h * h * mode.omega * mode.omega) * mode.v
                - h * mode.omega * mode.omega * mode.q)
                / denominator;
            free_q[i] = mode.q + 0.5 * h * (mode.v + free_v[i]);
            let weight = self.hammer_weights[i];
            response_v[i] = h * weight / (mode.mass * denominator);
            delta_free -= weight * free_q[i];
            compliance += 0.5 * h * weight * response_v[i];
        }
        let stiffness = self.profile.contact_stiffness;
        let maximum_delta = delta0.max(delta_free).max(0.0);
        let mut low = 0.0;
        let mut high = stiffness * maximum_delta * maximum_delta;
        // Monotone scalar solve with a proven bracket, bounded work, no Newton
        // divergence. The discrete gradient preserves the contact potential.
        for _ in 0..40 {
            let force = 0.5 * (low + high);
            let delta1 = delta_free - compliance * force;
            if force > contact_gradient(stiffness, delta0, delta1) {
                high = force;
            } else {
                low = force;
            }
        }
        self.force = 0.5 * (low + high);
        for (i, mode) in self.modes.iter_mut().enumerate() {
            mode.v = free_v[i] + response_v[i] * self.force;
            mode.q = free_q[i] + 0.5 * h * response_v[i] * self.force;
        }
        self.hammer_x += h * self.hammer_v - 0.5 * h * h * self.force / self.hammer_mass;
        self.hammer_v -= h * self.force / self.hammer_mass;
        let delta1 = self.hammer_x - self.contact_position();
        let contact_v: f64 = self
            .modes
            .iter()
            .zip(self.hammer_weights)
            .map(|(m, b)| m.v * b)
            .sum();
        if delta1 <= 0.0 && self.hammer_v <= contact_v {
            self.contact = false;
            self.hammer_x = 0.0;
            self.hammer_v = 0.0;
        }
    }

    fn contact_position(&self) -> f64 {
        self.modes
            .iter()
            .zip(self.hammer_weights)
            .map(|(m, b)| m.q * b)
            .sum()
    }

    /// Tip position and velocity in laboratory coordinates: along the strike
    /// direction first, then across it.
    pub(crate) fn tip(&self) -> ([f64; 2], [f64; 2]) {
        let mut q = [0.0; 2];
        let mut v = [0.0; 2];
        for (c, mode) in self.modes.iter().enumerate() {
            for direction in 0..2 {
                q[direction] += mode.q * self.pickup_weights[direction][c];
                v[direction] += mode.v * self.pickup_weights[direction][c];
            }
        }
        (q, v)
    }

    pub fn probe(&self) -> Probe {
        let (position, velocity) = self.tip();
        let (displacement_m, velocity_m_s) = (position[0], velocity[0]);
        let delta = (self.hammer_x - self.contact_position()).max(0.0);
        let hammer_energy = if self.contact {
            0.5 * self.hammer_mass * self.hammer_v * self.hammer_v
                + self.profile.contact_stiffness * delta * delta * delta / 3.0
        } else {
            0.0
        };
        Probe {
            displacement_m,
            velocity_m_s,
            transverse_displacement_m: position[1],
            contact_force_n: self.force,
            mechanical_energy_j: self.modes.iter().map(Mode::energy).sum::<f64>() + hammer_energy,
            pickup_signal: self.signal,
            contact_active: self.contact,
        }
    }

    pub fn reset(&mut self) {
        for mode in &mut self.modes {
            mode.q = 0.0;
            mode.v = 0.0;
        }
        self.hammer_x = 0.0;
        self.hammer_v = 0.0;
        self.contact = false;
        self.active = false;
        self.force = 0.0;
        self.signal = 0.0;
    }
}

/// How much faster the second principal axis runs than the first.
///
/// The tine is a circular wire, so its bending rigidity is the same in both
/// directions; the split belongs to the support, which is stiffer one way than
/// the other. The offline polarized assembly spends three ratios on that
/// (support, rotation and tonebar); a three-mode voice has room for one, and
/// it multiplies every bending partial of the second axis alike.
fn axis_frequency_ratio(profile: Profile, axis: usize) -> f64 {
    if axis == 0 {
        1.0
    } else {
        profile.tine_transverse_frequency_ratio
    }
}

/// Resolve each axis onto the laboratory directions once, at voice
/// preparation, so that nothing per sample has to know about angles.
///
/// With the principal axes rotated by `theta` from the strike direction, the
/// first axis points along `(cos, sin)` and the second along `(-sin, cos)`. The
/// hammer travels along the strike direction and feels only what each axis
/// contributes there, which is also how hard its force drives that axis back.
/// At `theta = 0` the second axis contributes nothing to the strike, so it is
/// never driven and never moves: the pair collapses to the single coordinate
/// this voice had before, exactly and not approximately.
fn resolve_axes(profile: Profile) -> ([f64; COORDINATES], [[f64; COORDINATES]; 2]) {
    let (sin, cos) = profile.tine_boundary_angle_rad.sin_cos();
    let along = [cos, -sin];
    let across = [sin, cos];
    let shape = [1.0, profile.bar_partial_strike_weight, 0.12];
    let mut hammer = [0.0; COORDINATES];
    let mut pickup = [[0.0; COORDINATES]; 2];
    for c in 0..COORDINATES {
        let (axis, i) = (c / MODES, c % MODES);
        hammer[c] = shape[i] * along[axis];
        pickup[0][c] = PICKUP_WEIGHTS[i] * along[axis];
        pickup[1][c] = PICKUP_WEIGHTS[i] * across[axis];
    }
    (hammer, pickup)
}

/// Difference quotient of V(d) = k * max(d, 0)^3 / 3.
/// Piecewise algebra avoids catastrophic cancellation at nearby compressions.
pub(crate) fn contact_gradient(k: f64, a: f64, b: f64) -> f64 {
    if a >= 0.0 && b >= 0.0 {
        k * (a * a + a * b + b * b) / 3.0
    } else if a <= 0.0 && b <= 0.0 {
        0.0
    } else {
        let positive = a.max(b);
        k * positive * positive * positive / (3.0 * (b - a).abs())
    }
}
