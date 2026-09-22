use crate::laboratory::{AxialAperture, PlanarAperture, aperture_voltage, planar_voltage};
use crate::{FIRST_NOTE, LAST_NOTE, MagneticPickup, ModelError, OVERSAMPLE, Profile};
use core::f64::consts::TAU;

/// Three bending modes of the tine, and the tonebar's lowest. The first two
/// entries are the coupled pair's normal modes, so index 0 is the
/// tine-dominated one and index 3 the tonebar-dominated one; the bar and
/// third partials keep their places in between.
const MODES: usize = 4;
/// The two principal bending directions of the tine. A circular wire bends the
/// same way in both; its support does not, which is what separates them.
const AXES: usize = 2;
/// Every bending mode of both axes, laid out as `axis * MODES + mode`.
const COORDINATES: usize = MODES * AXES;
/// How much of each mode the pickup sees at the tip, relative to the strike
/// point. The tonebar's entry matches the tine's first mode, because the
/// tonebar reaches the tip through exactly that shape: it moves the root the
/// tine is clamped to and nothing else.
const PICKUP_WEIGHTS: [f64; MODES] = [1.0, 0.8, 0.6, 1.0];

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
        let table = fork_modes(profile, frequency, scale);
        let modes = core::array::from_fn(|c| {
            let (axis, i) = (c / MODES, c % MODES);
            let spec = table[i];
            let spin = axis_frequency_ratio(profile, axis);
            Mode::new(
                spec.frequency * spin,
                spec.mass,
                underdamped(spec.gamma, spec.frequency * spin),
                dt,
                dt / contact_steps as f64,
            )
        });
        let (hammer_weights, pickup_weights) = resolve_axes(profile, &table);
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
        let table = fork_modes(profile, frequency, scale);
        for (c, mode) in self.modes.iter_mut().enumerate() {
            let (axis, i) = (c / MODES, c % MODES);
            let spec = table[i];
            let spin = axis_frequency_ratio(profile, axis);
            mode.set(
                spec.frequency * spin,
                spec.mass,
                underdamped(spec.gamma, spec.frequency * spin),
                self.dt,
                self.dt / self.contact_steps as f64,
            );
        }
        (self.hammer_weights, self.pickup_weights) = resolve_axes(profile, &table);
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
        // Retuning keeps every mass and loss, so the fork is resolved at the
        // note's own frequency and only its frequencies are taken from it.
        let base = 440.0 * 2.0_f64.powf((f64::from(self.note) - 69.0) / 12.0);
        let scale = (220.0 / base).clamp(0.15, 4.0);
        let table = fork_modes(self.profile, frequency, scale);
        for (c, mode) in self.modes.iter_mut().enumerate() {
            let (axis, i) = (c / MODES, c % MODES);
            let spin = axis_frequency_ratio(self.profile, axis);
            mode.set(
                table[i].frequency * spin,
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

/// One mode of the assembly: where it sits, how heavy it is, how fast it
/// dies, and how much of it appears at the tine.
#[derive(Clone, Copy)]
struct ModeSpec {
    frequency: f64,
    mass: f64,
    gamma: f64,
    /// The mode's displacement at the tine, per unit modal coordinate. The
    /// hammer strikes the tine and the pickup watches the tine, so this one
    /// number carries the mode into and out of the instrument.
    tine: f64,
}

/// The four modes of the tine and its tonebar, at one note.
///
/// The tine's first bending mode and the tonebar's lowest are two masses on
/// two springs joined by a third, standing for the aluminium block. That pair
/// has two normal modes, and normal modes are independent oscillators, so the
/// contact solve and the modal advance carry them without knowing anything
/// changed. What the coupling buys -- the prongs trading energy, a decay that
/// is not one exponential -- comes out of superposing two modes that damp at
/// different rates, which is exactly what the fork does.
///
/// The tine's own second and third bending modes are left uncoupled. They
/// couple to the tonebar too, at ratios nobody here has measured; this is the
/// lowest-order step, not the whole assembly.
fn fork_modes(profile: Profile, frequency: f64, scale: f64) -> [ModeSpec; MODES] {
    let decay = |seconds: f64| 1000.0_f64.ln() / (seconds * scale.sqrt());
    let tine_mass = profile.modal_mass_kg * scale;
    let bending = [
        ModeSpec {
            frequency: frequency * profile.bar_partial_ratio,
            mass: tine_mass,
            gamma: decay(profile.bar_partial_decay_seconds),
            tine: 1.0,
        },
        ModeSpec {
            frequency: frequency * 17.55,
            mass: tine_mass,
            gamma: decay(profile.third_partial_decay_seconds),
            tine: 1.0,
        },
    ];
    let tine_gamma = decay(profile.decay_seconds);
    let bar_gamma = decay(profile.tonebar_decay_seconds);
    let bar_frequency = frequency * profile.tonebar_frequency_ratio;
    let bar_mass = tine_mass * profile.tonebar_mass_ratio;

    let (first, tonebar) = if profile.tonebar_coupling <= 0.0 {
        // Uncoupled the pair separates exactly: the tine is the fundamental
        // it always was, and the tonebar has no participation at the tine, so
        // it is neither struck nor heard.
        (
            ModeSpec {
                frequency,
                mass: tine_mass,
                gamma: tine_gamma,
                tine: 1.0,
            },
            ModeSpec {
                frequency: bar_frequency,
                mass: bar_mass,
                gamma: bar_gamma,
                tine: 0.0,
            },
        )
    } else {
        let tine_omega = TAU * frequency;
        let bar_omega = TAU * bar_frequency;
        let tine_k = tine_mass * tine_omega * tine_omega;
        let bar_k = bar_mass * bar_omega * bar_omega;
        let joint = profile.tonebar_coupling * tine_k;
        // det(K - w^2 M) = 0 for the two-mass network, solved in closed form.
        let a = (tine_k + joint) / tine_mass;
        let b = (bar_k + joint) / bar_mass;
        let gap = ((a - b) * (a - b) + 4.0 * joint * joint / (tine_mass * bar_mass)).sqrt();
        let roots = [0.5 * ((a + b) - gap), 0.5 * ((a + b) + gap)];
        // The root nearer the tine's own frequency is the one the tine leads.
        let led_by_tine = usize::from(
            (roots[1] - tine_omega * tine_omega).abs()
                < (roots[0] - tine_omega * tine_omega).abs(),
        );
        let build = |root: f64, lead_is_tine: bool| {
            // Normalise on whichever prong leads, so neither ratio blows up
            // as the joint softens.
            let (tine_amplitude, bar_amplitude) = if lead_is_tine {
                (1.0, tine_mass * (a - root) / joint)
            } else {
                (bar_mass * (b - root) / joint, 1.0)
            };
            let mass = tine_mass * tine_amplitude * tine_amplitude
                + bar_mass * bar_amplitude * bar_amplitude;
            // Proportional damping: each normal mode loses at the rate its
            // own energy is shared at. An assumption, and the usual one.
            let share = tine_mass * tine_amplitude * tine_amplitude / mass;
            ModeSpec {
                frequency: root.max(0.0).sqrt() / TAU,
                mass,
                gamma: share * tine_gamma + (1.0 - share) * bar_gamma,
                tine: tine_amplitude,
            }
        };
        let tine_mode = build(roots[led_by_tine], true);
        let bar_mode = build(roots[1 - led_by_tine], false);
        // Joining a spring to the tine stiffens it, so an assembled fork
        // rings sharp -- 83 cents at a tenth of the tine's own stiffness, an
        // octave at three times it. A real tine is tuned after assembly,
        // with its tuning spring, so the note is the note whatever the block
        // is doing. Both normal modes move together, because the technician
        // moves the tine and not the joint.
        //
        // Tune on whichever mode is actually heard, not on whichever started
        // life as the tine. A strike at the tine drives each mode by its own
        // participation there and over its mass, and the pickup hears it by
        // that participation again, so this ranks them. Joined hard enough,
        // the prongs stop being a tine and a tonebar and the other one leads;
        // the note still has to come out where it was asked for.
        let heard = |mode: &ModeSpec| mode.tine * mode.tine / mode.mass;
        let leader = if heard(&bar_mode) > heard(&tine_mode) {
            &bar_mode
        } else {
            &tine_mode
        };
        let retune = frequency / leader.frequency.max(1e-9);
        (
            ModeSpec {
                frequency: tine_mode.frequency * retune,
                ..tine_mode
            },
            ModeSpec {
                frequency: bar_mode.frequency * retune,
                ..bar_mode
            },
        )
    };
    [first, bending[0], bending[1], tonebar]
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
fn resolve_axes(
    profile: Profile,
    table: &[ModeSpec; MODES],
) -> ([f64; COORDINATES], [[f64; COORDINATES]; 2]) {
    let (sin, cos) = profile.tine_boundary_angle_rad.sin_cos();
    let along = [cos, -sin];
    let across = [sin, cos];
    // Shape of each mode at the strike point along the tine. The tonebar's
    // entry matches the tine's first mode for the same reason its pickup
    // weight does: it arrives through the root, in that shape.
    let strike = [1.0, profile.bar_partial_strike_weight, 0.12, 1.0];
    let mut hammer = [0.0; COORDINATES];
    let mut pickup = [[0.0; COORDINATES]; 2];
    for c in 0..COORDINATES {
        let (axis, i) = (c / MODES, c % MODES);
        // How much of the mode stands at the tine at all, then how much of
        // the tine's motion each direction sees.
        let reach = table[i].tine;
        hammer[c] = strike[i] * reach * along[axis];
        pickup[0][c] = PICKUP_WEIGHTS[i] * reach * along[axis];
        pickup[1][c] = PICKUP_WEIGHTS[i] * reach * across[axis];
    }
    (hammer, pickup)
}

/// Keep a mode oscillating.
///
/// `Mode::set` takes the square root of `omega^2 - gamma^2`, which is real
/// only while the mode is underdamped. Every mode that existed before the
/// tonebar was, because their losses were tied to the tine's. The tonebar can
/// be asked for a short decay at a low frequency, and a mode that stops
/// oscillating would come back as NaN rather than as a thud, so its loss is
/// held below its own frequency.
fn underdamped(gamma: f64, frequency: f64) -> f64 {
    // The damped state adds 55 to whatever is stored, so leave it room.
    let ceiling = 0.9 * TAU * frequency - 55.0;
    gamma.min(ceiling.max(0.0))
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
