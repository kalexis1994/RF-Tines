//! Reciprocal two-axis flux transducer and passive coil/load circuit.
//! Offline reduced model: finite-aperture flux proxy, constant coil inductance.
use super::*;

#[derive(Clone, Copy, Debug)]
pub struct SpatialPickupProfile {
    pub gap_m: f64,
    pub offset_xy_m: [f64; 2],
    pub pole_radius_m: f64,
    /// How far the pole face is ground from a disc towards an edge.
    ///
    /// Zero is the circular face every geometry used before this, and one
    /// collapses the source onto a line across the direction the tine
    /// travels. The sources describe the real tip as wedge shaped and
    /// pointing at the tine; which way the edge runs they do not say, and
    /// across the travel is the orientation that removes the flux slope's
    /// sign inversion. See docs/WEDGE-POLE.md.
    pub pole_wedge: f64,
    /// Flux linkage scale (weber-turn), not a voltage gain or fitted magnetic field.
    pub flux_scale_wb: f64,
}
impl Default for SpatialPickupProfile {
    fn default() -> Self {
        Self {
            gap_m: 0.0015,
            offset_xy_m: [0.0005, 0.0002],
            pole_radius_m: 0.0005,
            pole_wedge: 0.0,
            flux_scale_wb: 0.001,
        }
    }
}
impl SpatialPickupProfile {
    pub fn validate(self) -> Result<(), ModelError> {
        bounded(self.gap_m, 0.0002, 0.01)?;
        for x in self.offset_xy_m {
            bounded(x, -0.01, 0.01)?;
        }
        bounded(self.pole_radius_m, 0.0, 0.003)?;
        bounded(self.pole_wedge, 0.0, 1.0)?;
        bounded(self.flux_scale_wb, 0.0, 0.02)
    }
}
pub struct SpatialPickup {
    p: SpatialPickupProfile,
    nodes: [[f64; 2]; 16],
}
impl SpatialPickup {
    pub fn new(p: SpatialPickupProfile) -> Result<Self, ModelError> {
        p.validate()?;
        // Uniform disk-area average: two-point Gauss in squared radius, eight azimuths.
        let nodes = core::array::from_fn(|i| {
            let sign = if i < 8 { -1.0 } else { 1.0 };
            let r = p.pole_radius_m * ((1.0 + sign / 3.0_f64.sqrt()) / 2.0).sqrt();
            let (sn, cs) = (TAU * (i % 8) as f64 / 8.0).sin_cos();
            [r * cs * (1.0 - p.pole_wedge), r * sn]
        });
        Ok(Self { p, nodes })
    }
    pub fn flux_wb(&self, xy: [f64; 2]) -> Result<f64, ModelError> {
        for x in xy {
            bounded(x, -0.05, 0.05)?;
        }
        Ok(self
            .nodes
            .iter()
            .map(|n| {
                let r2 = self.p.gap_m.powi(2)
                    + (xy[0] + self.p.offset_xy_m[0] - n[0]).powi(2)
                    + (xy[1] + self.p.offset_xy_m[1] - n[1]).powi(2);
                self.p.flux_scale_wb * self.p.gap_m.powi(3) / (16.0 * r2 * r2.sqrt())
            })
            .sum())
    }
    /// Symmetric discrete gradient, including coincident endpoints. Its dot
    /// product with displacement equals flux change without subtracting close fluxes.
    pub fn discrete_gradient(&self, a: [f64; 2], b: [f64; 2]) -> Result<[f64; 2], ModelError> {
        for x in a.into_iter().chain(b) {
            bounded(x, -0.05, 0.05)?;
        }
        let mut gradient = [0.0; 2];
        for node in self.nodes {
            let u: [f64; 2] = core::array::from_fn(|i| a[i] + self.p.offset_xy_m[i] - node[i]);
            let v: [f64; 2] = core::array::from_fn(|i| b[i] + self.p.offset_xy_m[i] - node[i]);
            let ra = (self.p.gap_m.powi(2) + dot(u, u)).sqrt();
            let rb = (self.p.gap_m.powi(2) + dot(v, v)).sqrt();
            let coefficient =
                -self.p.flux_scale_wb * self.p.gap_m.powi(3) * (ra * ra + ra * rb + rb * rb)
                    / (16.0 * ra.powi(3) * rb.powi(3) * (ra + rb));
            for i in 0..2 {
                gradient[i] += coefficient * (u[i] + v[i]);
            }
        }
        Ok(gradient)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PickupCircuitProfile {
    pub inductance_h: f64,
    pub coil_resistance_ohm: f64,
    pub shunt_capacitance_f: f64,
    /// None removes the resistive load; shunt capacitance remains connected.
    pub load_resistance_ohm: Option<f64>,
}
impl Default for PickupCircuitProfile {
    fn default() -> Self {
        Self {
            inductance_h: 0.2,
            coil_resistance_ohm: 200.0,
            shunt_capacitance_f: 4.7e-9,
            load_resistance_ohm: Some(10000.0),
        }
    }
}
impl PickupCircuitProfile {
    pub fn validate(self) -> Result<(), ModelError> {
        bounded(self.inductance_h, 1e-6, 100.0)?;
        bounded(self.coil_resistance_ohm, 0.0, 1e6)?;
        bounded(self.shunt_capacitance_f, 1e-12, 1e-3)?;
        if let Some(r) = self.load_resistance_ohm {
            bounded(r, 1.0, 1e9)?;
        }
        Ok(())
    }
    fn conductance(self) -> f64 {
        self.load_resistance_ohm.map_or(0.0, |r| 1.0 / r)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Default)]
struct CircuitState {
    current: f64,
    voltage: f64,
    coil_heat: f64,
    load_heat: f64,
    input_work: f64,
}
fn circuit_energy(p: PickupCircuitProfile, s: CircuitState) -> f64 {
    0.5 * (p.inductance_h * s.current * s.current + p.shunt_capacitance_f * s.voltage * s.voltage)
}
fn circuit_step(
    p: PickupCircuitProfile,
    a: CircuitState,
    h: f64,
    emf: f64,
) -> Result<CircuitState, ModelError> {
    let g = p.conductance();
    let ai = p.inductance_h / h + p.coil_resistance_ohm / 2.0;
    let av = p.shunt_capacitance_f / h + g / 2.0;
    let determinant = ai * av + 0.25;
    let ri = emf - p.coil_resistance_ohm * a.current - a.voltage;
    let rv = a.current - g * a.voltage;
    let di = (ri * av - 0.5 * rv) / determinant;
    let dv = (ai * rv + 0.5 * ri) / determinant;
    let im = a.current + di / 2.0;
    let vm = a.voltage + dv / 2.0;
    let b = CircuitState {
        current: a.current + di,
        voltage: a.voltage + dv,
        coil_heat: a.coil_heat + h * p.coil_resistance_ohm * im * im,
        load_heat: a.load_heat + h * g * vm * vm,
        input_work: a.input_work + h * emf * im,
    };
    if [b.current, b.voltage, b.coil_heat, b.load_heat, b.input_work]
        .iter()
        .any(|x| !x.is_finite())
    {
        return Err(ModelError("non-finite pickup circuit state"));
    }
    Ok(b)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElectromechanicalProbe {
    pub mechanical: PolarizedActionProbe,
    pub current_a: f64,
    pub output_voltage_v: f64,
    pub emf_v: f64,
    pub reaction_force_xy_n: [f64; 2],
    pub electrical_energy_j: f64,
    pub coil_heat_j: f64,
    pub load_heat_j: f64,
    pub circuit_input_work_j: f64,
    pub circuit_balance_residual_j: f64,
    pub exchange_residual_j: f64,
    pub total_balance_residual_j: f64,
    pub iterations: usize,
    pub maximum_iterations: usize,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ElectromechanicalProfile {
    pub geometry: TineGeometry,
    pub assembly: ModalAssemblyProfile,
    pub felt: FeltDamperProfile,
    pub action: ActionProfile,
    pub polarization: PolarizationProfile,
    pub pickup: SpatialPickupProfile,
    pub circuit: PickupCircuitProfile,
}
pub struct ElectromechanicalAssembly {
    mechanics: PolarizedActionAssembly,
    pickup: SpatialPickup,
    circuit: PickupCircuitProfile,
    state: CircuitState,
    h: f64,
    force: [f64; 2],
    emf: f64,
    iterations: usize,
    max_iterations: usize,
    rest: Option<RestPreparation>,
}
impl ElectromechanicalAssembly {
    pub fn new(h: f64, p: ElectromechanicalProfile) -> Result<Self, ModelError> {
        p.circuit.validate()?;
        Ok(Self {
            mechanics: PolarizedActionAssembly::new_polarized(
                h,
                p.geometry,
                p.assembly,
                p.felt,
                p.action,
                p.polarization,
            )?,
            pickup: SpatialPickup::new(p.pickup)?,
            circuit: p.circuit,
            state: CircuitState::default(),
            h,
            force: [0.0; 2],
            emf: 0.0,
            iterations: 0,
            max_iterations: 0,
            rest: None,
        })
    }
    /// Prepare the mechanical preload in force equilibrium with zero current
    /// and voltage. Does not advance time, mute samples or erase stored energy.
    /// Requires positive anchored stiffness; new() retains the historical start.
    pub fn new_at_rest(h: f64, p: ElectromechanicalProfile) -> Result<Self, ModelError> {
        let mut result = Self::new(h, p)?;
        result.rest = Some(result.mechanics.initialize_rest()?);
        Ok(result)
    }
    pub fn rest_preparation(&self) -> Option<RestPreparation> {
        self.rest
    }
    /// Read-only damping operator for independent offline loss observers.
    pub fn structural_damping_matrix(&self) -> [[f64; 18]; 18] {
        self.mechanics.structural_damping_matrix()
    }
    pub fn advance(&mut self, pedestal: f64, pedal: f64) -> Result<(), ModelError> {
        self.advance_budget(pedestal, pedal, 16)
    }
    fn advance_budget(
        &mut self,
        pedestal: f64,
        pedal: f64,
        budget: usize,
    ) -> Result<(), ModelError> {
        let checkpoint = self.mechanics.checkpoint();
        let a = self.mechanics.probe();
        let electrical = self.state;
        let result = (|| {
            let mut force = self.force;
            for iteration in 1..=budget {
                self.mechanics.restore(checkpoint);
                self.mechanics
                    .advance_with_pickup_force(pedestal, pedal, force)?;
                let b = self.mechanics.probe();
                let gradient = self
                    .pickup
                    .discrete_gradient(a.pickup_displacement_xy_m, b.pickup_displacement_xy_m)?;
                let delta = core::array::from_fn(|i| {
                    b.pickup_displacement_xy_m[i] - a.pickup_displacement_xy_m[i]
                });
                let emf = -dot(gradient, delta) / self.h;
                let next = circuit_step(self.circuit, electrical, self.h, emf)?;
                let current_mid = 0.5 * (electrical.current + next.current);
                let expected = gradient.map(|g| g * current_mid);
                let residual = (force[0] - expected[0]).hypot(force[1] - expected[1]);
                let scale = expected[0].hypot(expected[1]).max(1e-9);
                if residual <= 1e-10 * scale {
                    self.state = next;
                    self.force = force;
                    self.emf = emf;
                    self.iterations = iteration;
                    self.max_iterations = self.max_iterations.max(iteration);
                    return Ok(());
                }
                force = expected;
            }
            Err(ModelError(
                "electromechanical coupling iteration budget exhausted",
            ))
        })();
        if result.is_err() {
            self.mechanics.restore(checkpoint);
        }
        result
    }
    pub fn probe(&self) -> ElectromechanicalProbe {
        let m = self.mechanics.probe();
        let s = self.state;
        let energy = circuit_energy(self.circuit, s);
        let circuit_balance = energy + s.coil_heat + s.load_heat - s.input_work;
        let exchange = m.pickup_force_work_j + s.input_work;
        ElectromechanicalProbe {
            mechanical: m,
            current_a: s.current,
            output_voltage_v: s.voltage,
            emf_v: self.emf,
            reaction_force_xy_n: self.force,
            electrical_energy_j: energy,
            coil_heat_j: s.coil_heat,
            load_heat_j: s.load_heat,
            circuit_input_work_j: s.input_work,
            circuit_balance_residual_j: circuit_balance,
            exchange_residual_j: exchange,
            total_balance_residual_j: m.balance_residual_j + circuit_balance + exchange,
            iterations: self.iterations,
            maximum_iterations: self.max_iterations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stationary_rest_is_time_step_independent_quiet_and_rejects_free_returns() {
        for length in [0.07, 0.12] {
            let mut p = ElectromechanicalProfile::default();
            p.geometry.length_m = length;
            p.polarization.felt_angle_rad = 0.4;
            let mut a = ElectromechanicalAssembly::new_at_rest(1e-6, p).unwrap();
            let b = ElectromechanicalAssembly::new_at_rest(0.5e-6, p).unwrap();
            assert_eq!(a.probe(), b.probe());
            assert_eq!(a.rest_preparation(), b.rest_preparation());
            let start = a.probe();
            assert!(start.mechanical.contact_force_n[1] > 0.0);
            assert!(start.mechanical.initial_energy_j > 0.0);
            for _ in 0..50000 {
                a.advance(p.action.hammer_rest_m, p.action.damper_closed_m)
                    .unwrap();
                let v = a.probe();
                assert!(v.output_voltage_v.abs() < 1e-9);
                assert!(
                    v.total_balance_residual_j.abs() / start.mechanical.initial_energy_j < 1e-8
                );
            }
            assert_eq!(a.probe().mechanical.absolute_drive_work_j, 0.0);
            let before = a.probe();
            assert!(a.advance(f64::NAN, p.action.damper_closed_m).is_err());
            assert_eq!(before, a.probe());
        }
        let mut p = ElectromechanicalProfile::default();
        p.action.hammer_return_n_m = 0.0;
        assert!(ElectromechanicalAssembly::new(1e-6, p).is_ok());
        assert!(ElectromechanicalAssembly::new_at_rest(1e-6, p).is_err());
        let mut p = ElectromechanicalProfile::default();
        p.assembly.translation_stiffness_n_m = 0.0;
        p.assembly.rotation_stiffness_n_m_rad = 0.0;
        assert!(ElectromechanicalAssembly::new(1e-6, p).is_ok());
        assert!(ElectromechanicalAssembly::new_at_rest(1e-6, p).is_err());
    }
    #[test]
    fn open_felt_rest_preserves_zero_energy_without_artificial_contact() {
        let mut p = ElectromechanicalProfile::default();
        p.action.damper_closed_m = -0.001;
        let mut v = ElectromechanicalAssembly::new_at_rest(1e-6, p).unwrap();
        assert_eq!(v.probe().mechanical.initial_energy_j, 0.0);
        assert_eq!(v.probe().mechanical.contact_force_n, [0.0; 4]);
        for _ in 0..1000 {
            v.advance(p.action.hammer_rest_m, p.action.damper_closed_m)
                .unwrap();
        }
        assert_eq!(v.probe().mechanical.mechanical_energy_j, 0.0);
        assert_eq!(v.probe().output_voltage_v, 0.0);
    }
    #[test]
    fn spatial_flux_gradient_closes_work_and_matches_analytic_derivative_limit() {
        let pickup = SpatialPickup::new(SpatialPickupProfile::default()).unwrap();
        for (a, b) in [
            ([0.0, 0.0], [0.0002, -0.0001]),
            ([0.002, -0.001], [-0.001, 0.003]),
            ([0.0001, 0.0002], [0.0001, 0.0002]),
        ] {
            let g = pickup.discrete_gradient(a, b).unwrap();
            let expected = pickup.flux_wb(b).unwrap() - pickup.flux_wb(a).unwrap();
            assert!((dot(g, core::array::from_fn(|i| b[i] - a[i])) - expected).abs() < 1e-17);
            assert_eq!(g, pickup.discrete_gradient(b, a).unwrap());
            let tangent = pickup.discrete_gradient(a, a).unwrap();
            for i in 0..2 {
                let mut plus = a;
                let mut minus = a;
                plus[i] += 1e-8;
                minus[i] -= 1e-8;
                let numerical =
                    (pickup.flux_wb(plus).unwrap() - pickup.flux_wb(minus).unwrap()) / 2e-8;
                assert!((numerical - tangent[i]).abs() < 1e-8);
            }
        }
        assert!(pickup.discrete_gradient([f64::NAN, 0.0], [0.0; 2]).is_err());
    }
    #[test]
    fn finite_aperture_agrees_with_independent_dense_disk_integration_at_default_geometry() {
        let p = SpatialPickupProfile::default();
        let pickup = SpatialPickup::new(p).unwrap();
        for xy in [[0.0; 2], [0.001, -0.0005], [-0.0005, 0.001]] {
            let mut exact = 0.0;
            for radial in 0..128 {
                for angular in 0..256 {
                    let r = p.pole_radius_m * ((radial as f64 + 0.5) / 128.0).sqrt();
                    let (sn, cs) = (TAU * (angular as f64 + 0.5) / 256.0).sin_cos();
                    let r2 = p.gap_m.powi(2)
                        + (xy[0] + p.offset_xy_m[0] - r * cs).powi(2)
                        + (xy[1] + p.offset_xy_m[1] - r * sn).powi(2);
                    exact += p.flux_scale_wb * p.gap_m.powi(3) / (r2 * r2.sqrt() * 128.0 * 256.0);
                }
            }
            assert!((pickup.flux_wb(xy).unwrap() - exact).abs() / exact < 1e-4);
        }
    }
    #[test]
    fn passive_circuit_matches_complex_transfer_and_lossless_lc_energy() {
        let p = PickupCircuitProfile::default();
        let h = 1e-6;
        let w = TAU * 1000.0;
        let mut s = CircuitState::default();
        let mut sin = 0.0;
        let mut cos = 0.0;
        for i in 0..100000 {
            s = circuit_step(p, s, h, (w * (i as f64 + 0.5) * h).sin()).unwrap();
            assert!(
                (circuit_energy(p, s) + s.coil_heat + s.load_heat - s.input_work).abs() < 1e-12
            );
            if i >= 50000 {
                let phase = w * (i as f64 + 1.0) * h;
                sin += s.voltage * phase.sin() / 25000.0;
                cos += s.voltage * phase.cos() / 25000.0;
            }
        }
        let re = 1.0 + p.coil_resistance_ohm * p.conductance()
            - w * w * p.inductance_h * p.shunt_capacitance_f;
        let im =
            w * (p.coil_resistance_ohm * p.shunt_capacitance_f + p.inductance_h * p.conductance());
        assert!((sin - re / (re * re + im * im)).abs() < 1e-5);
        assert!((cos + im / (re * re + im * im)).abs() < 1e-5);
        let p = PickupCircuitProfile {
            coil_resistance_ohm: 0.0,
            load_resistance_ohm: None,
            ..p
        };
        let mut s = CircuitState {
            current: 0.001,
            voltage: 0.1,
            ..CircuitState::default()
        };
        let initial = circuit_energy(p, s);
        for _ in 0..100000 {
            s = circuit_step(p, s, h, 0.0).unwrap();
        }
        assert!((circuit_energy(p, s) - initial).abs() / initial < 1e-10);
        assert_eq!(s.coil_heat + s.load_heat, 0.0);
    }
    #[test]
    fn coupled_motion_exchanges_energy_with_load_and_rolls_back_failed_iterations() {
        let h = 1e-6;
        let p = ElectromechanicalProfile::default();
        let mut v = ElectromechanicalAssembly::new(h, p).unwrap();
        let initial = v.probe();
        assert!(
            v.advance_budget(p.action.hammer_rest_m, p.action.damper_closed_m, 1)
                .is_err()
        );
        assert_eq!(initial, v.probe());
        assert!(v.advance(f64::NAN, p.action.damper_closed_m).is_err());
        assert_eq!(initial, v.probe());
        let mut x = p.action.hammer_rest_m;
        let mut peak = 0.0_f64;
        let mut force = 0.0_f64;
        for i in 0..60000 {
            let target = if (10000..35000).contains(&i) {
                -p.action.escapement_m
            } else {
                p.action.hammer_rest_m
            };
            x += (target - x).clamp(-1.5 * h, 1.5 * h);
            v.advance(x, p.action.damper_closed_m).unwrap();
            let b = v.probe();
            let scale =
                (b.mechanical.initial_energy_j + b.mechanical.absolute_drive_work_j).max(1e-20);
            assert!(b.total_balance_residual_j.abs() / scale < 1e-8);
            assert!(b.exchange_residual_j.abs() / scale < 1e-10);
            assert!(b.circuit_balance_residual_j.abs() < 1e-12);
            peak = peak.max(b.output_voltage_v.abs());
            force = force.max(b.reaction_force_xy_n[0].abs());
        }
        assert!(peak > 0.001 && force > 1e-8);
        assert!(v.probe().coil_heat_j > 0.0 && v.probe().load_heat_j > 0.0);
        assert!(v.probe().mechanical.pickup_force_work_j < 0.0);
    }
    #[test]
    fn zero_flux_is_exactly_silent_and_preserves_the_uncoupled_mechanics() {
        let mut p = ElectromechanicalProfile::default();
        p.pickup.flux_scale_wb = 0.0;
        let mut v = ElectromechanicalAssembly::new(1e-6, p).unwrap();
        let mut reference = PolarizedActionAssembly::new_polarized(
            1e-6,
            p.geometry,
            p.assembly,
            p.felt,
            p.action,
            p.polarization,
        )
        .unwrap();
        let mut x = p.action.hammer_rest_m;
        for _ in 0..25000 {
            x = (-p.action.escapement_m).min(x + 1.5e-6);
            v.advance(x, p.action.damper_closed_m).unwrap();
            reference.advance(x, p.action.damper_closed_m).unwrap();
            let b = v.probe();
            assert_eq!(b.mechanical, reference.probe());
            assert_eq!(b.output_voltage_v, 0.0);
            assert_eq!(b.reaction_force_xy_n, [0.0; 2]);
        }
    }
}
