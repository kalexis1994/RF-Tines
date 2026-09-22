use crate::{MagneticPickup, ModelError, ProductionDecimator, SpatialPickupProfile};
use core::f64::consts::TAU;

pub const PICKUP_NAMES: [&str; 4] = [
    "Current",
    "Close Original",
    "Close Point Pole",
    "Close Aperture",
];
/// One RMS ratio per complete reference performance, never per note or velocity.
/// Paths 1 and 2: references/pickup-listening-summary.json (44.1 kHz, 2026-09-04).
/// Path 3: references/pickup-aperture-listening-summary.json (44.1 kHz, 2026-09-08),
/// whose first three ratios reproduce the earlier receipt exactly. Frozen at every rate.
pub const PICKUP_LEVEL_MATCH: [f64; 4] = [
    1.0,
    0.2967936920096338,
    0.1361600848962658,
    APERTURE_LEVEL_MATCH,
];
const APERTURE_LEVEL_MATCH: f64 = 2.4996279723549004;
/// Finite-aperture geometry selected by the pickup harmonics study: gap 0.5 mm,
/// lateral offset 0.5 mm, pole radius 2 mm. The flux scale only normalizes the flux.
pub const APERTURE_PICKUP: SpatialPickupProfile = SpatialPickupProfile {
    gap_m: 0.0005,
    offset_xy_m: [0.0005, 0.0],
    pole_radius_m: 0.002,
    pole_wedge: 0.0,
    flux_scale_wb: 0.001,
};

/// The laboratory's 16-node finite-aperture flux reduced to the tine axis. The
/// nodes are the same two-radius, eight-azimuth disk quadrature as `SpatialPickup`;
/// with no vertical offset the mirrored azimuths contribute identically and are
/// merged, so one flux slope costs ten square roots. The slope is the exact
/// analytic derivative of the node flux, not a table or a fit.
pub struct AxialAperture {
    terms: [[f64; 3]; 10],
    offset_m: f64,
    scale: f64,
}

impl AxialAperture {
    pub fn new(p: SpatialPickupProfile) -> Result<Self, ModelError> {
        p.validate()?;
        if p.offset_xy_m[1] != 0.0 {
            return Err(ModelError(
                "axial aperture reduction needs a zero vertical offset",
            ));
        }
        let mut terms = [[0.0; 3]; 10];
        for (half, sign) in [-1.0, 1.0].into_iter().enumerate() {
            let r = p.pole_radius_m * ((1.0 + sign / 3.0_f64.sqrt()) / 2.0).sqrt();
            for k in 0..5 {
                let (sn, cs) = (TAU * k as f64 / 8.0).sin_cos();
                let weight = if k == 0 || k == 4 { 1.0 } else { 2.0 };
                // Grinding the face towards an edge squashes the source along
                // the direction the tine travels; see `pole_wedge`.
                terms[half * 5 + k] = [
                    r * cs * (1.0 - p.pole_wedge),
                    p.gap_m.powi(2) + (r * sn).powi(2),
                    weight,
                ];
            }
        }
        Ok(Self {
            terms,
            offset_m: p.offset_xy_m[0],
            scale: -3.0 * p.flux_scale_wb * p.gap_m.powi(3) / 16.0,
        })
    }

    /// Flux slope along the tine axis, weber per metre; NaN outside the flux law's
    /// +-50 mm domain.
    pub fn slope_wb_per_m(&self, displacement_m: f64) -> f64 {
        if !(-0.05..=0.05).contains(&displacement_m) {
            return f64::NAN;
        }
        let mut sum = 0.0;
        for [node_x, lateral2, weight] in self.terms {
            let u = displacement_m + self.offset_m - node_x;
            let r2 = lateral2 + u * u;
            sum += weight * u / (r2 * r2 * r2.sqrt());
        }
        self.scale * sum
    }
}

/// Finite-aperture law: -0.015 d/dt of the unit-normalized 16-node flux, the same
/// scale the production law applies to its normalized flux. The lateral tip motion
/// is the only coordinate; a displacement outside the flux law's domain yields NaN,
/// which the engine treats as a numerical fault.
pub fn aperture_voltage(pickup: &AxialAperture, displacement_m: f64, velocity_m_s: f64) -> f64 {
    -0.015 * pickup.slope_wb_per_m(displacement_m) * velocity_m_s / APERTURE_PICKUP.flux_scale_wb
}

/// The same 16-node flux, kept in both transverse directions.
///
/// `AxialAperture` merges the mirrored azimuths, which is exact only while the
/// tip stays on the axis: off it the two halves of the pole ring no longer see
/// the same distance and their transverse contributions stop cancelling. A tine
/// whose principal bending axes are rotated off the strike direction does leave
/// the axis, so it needs all sixteen nodes and both components of the slope.
/// The gradient here is the analytic derivative of the same node flux
/// `SpatialPickup::flux_wb` sums, so on the axis it returns exactly what the
/// reduction returns, with a transverse component of zero.
pub struct PlanarAperture {
    nodes: [[f64; 2]; 16],
    offset: [f64; 2],
    gap_squared: f64,
    scale: f64,
}

impl PlanarAperture {
    pub fn new(p: SpatialPickupProfile) -> Result<Self, ModelError> {
        p.validate()?;
        // The same two-radius, eight-azimuth disk quadrature as `SpatialPickup`,
        // unmerged because the mirrored pairs no longer agree off the axis.
        let nodes = core::array::from_fn(|i| {
            let sign = if i < 8 { -1.0 } else { 1.0 };
            let r = p.pole_radius_m * ((1.0 + sign / 3.0_f64.sqrt()) / 2.0).sqrt();
            let (sn, cs) = (TAU * (i % 8) as f64 / 8.0).sin_cos();
            [r * cs * (1.0 - p.pole_wedge), r * sn]
        });
        Ok(Self {
            nodes,
            offset: p.offset_xy_m,
            gap_squared: p.gap_m.powi(2),
            scale: -3.0 * p.flux_scale_wb * p.gap_m.powi(3) / 16.0,
        })
    }

    /// Flux slope in both directions, weber per metre; NaN outside the flux
    /// law's +-50 mm domain, which the engine treats as a numerical fault.
    pub fn gradient_wb_per_m(&self, position: [f64; 2]) -> [f64; 2] {
        if position
            .iter()
            .any(|x| !(-0.05..=0.05).contains(x) || !x.is_finite())
        {
            return [f64::NAN; 2];
        }
        let mut sum = [0.0; 2];
        for node in self.nodes {
            let u = [
                position[0] + self.offset[0] - node[0],
                position[1] + self.offset[1] - node[1],
            ];
            let r2 = self.gap_squared + u[0] * u[0] + u[1] * u[1];
            let weight = 1.0 / (r2 * r2 * r2.sqrt());
            sum[0] += weight * u[0];
            sum[1] += weight * u[1];
        }
        [self.scale * sum[0], self.scale * sum[1]]
    }
}

/// The finite-aperture law for a tip that moves in two directions: the same
/// `-0.015 dPhi/dt`, with the flux changing through both coordinates at once.
pub fn planar_voltage(
    pickup: &PlanarAperture,
    position_m: [f64; 2],
    velocity_m_s: [f64; 2],
) -> f64 {
    let gradient = pickup.gradient_wb_per_m(position_m);
    -0.015 * (gradient[0] * velocity_m_s[0] + gradient[1] * velocity_m_s[1])
        / APERTURE_PICKUP.flux_scale_wb
}

pub(crate) struct Laboratory {
    pub pickup: MagneticPickup,
    pub aperture: AxialAperture,
    filters: [ProductionDecimator; 3],
    weights: [f64; 4],
    start: [f64; 4],
    selected: usize,
    position: u32,
    duration: u32,
}

impl Laboratory {
    pub fn new(rate: f64) -> Self {
        Self {
            pickup: MagneticPickup::new(0.0005, 0.00025).expect("validated research geometry"),
            aperture: AxialAperture::new(APERTURE_PICKUP).expect("validated aperture geometry"),
            filters: core::array::from_fn(|_| ProductionDecimator::new()),
            weights: [1.0, 0.0, 0.0, 0.0],
            start: [1.0, 0.0, 0.0, 0.0],
            selected: 0,
            position: (rate * 0.020).ceil() as u32,
            duration: (rate * 0.020).ceil() as u32,
        }
    }

    pub fn select(&mut self, index: usize) -> bool {
        if index >= PICKUP_NAMES.len() {
            return false;
        }
        if index != self.selected {
            self.start = self.weights;
            self.selected = index;
            self.position = 0;
        }
        true
    }

    pub fn push(&mut self, signals: [f64; 3]) {
        for (filter, signal) in self.filters.iter_mut().zip(signals) {
            filter.push(signal);
        }
    }

    pub fn mix(&mut self, current: f64) -> f64 {
        if self.position < self.duration {
            self.position += 1;
            let t = f64::from(self.position) / f64::from(self.duration);
            for (i, weight) in self.weights.iter_mut().enumerate() {
                *weight = self.start[i] * (1.0 - t) + if i == self.selected { t } else { 0.0 };
            }
        }
        [
            current,
            self.filters[0].output(),
            self.filters[1].output(),
            self.filters[2].output(),
        ]
        .into_iter()
        .zip(self.weights)
        .zip(PICKUP_LEVEL_MATCH)
        .map(|((signal, weight), gain)| signal * weight * gain)
        .sum()
    }

    pub fn reset(&mut self) {
        for filter in &mut self.filters {
            filter.clear();
        }
        self.weights = core::array::from_fn(|i| if i == self.selected { 1.0 } else { 0.0 });
        self.start = self.weights;
        self.position = self.duration;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SpatialPickup;

    #[test]
    fn axial_reduction_equals_the_two_axis_flux_gradient_on_the_tine_axis() {
        let full = SpatialPickup::new(APERTURE_PICKUP).unwrap();
        let axial = AxialAperture::new(APERTURE_PICKUP).unwrap();
        // The node terms cancel by up to three orders near the slope's zero
        // crossings, so agreement is held relative to the largest slope.
        let largest = (0..=400)
            .map(|i| {
                axial
                    .slope_wb_per_m(-0.006 + 0.012 * f64::from(i) / 400.0)
                    .abs()
            })
            .fold(0.0, f64::max);
        assert!(largest > 0.0);
        for i in 0..=400 {
            let x = -0.006 + 0.012 * f64::from(i) / 400.0;
            let gradient = full.discrete_gradient([x, 0.0], [x, 0.0]).unwrap()[0];
            let slope = axial.slope_wb_per_m(x);
            assert!(
                (slope - gradient).abs() <= 1e-12 * largest,
                "{x}: {slope} vs {gradient}"
            );
        }
        assert!(axial.slope_wb_per_m(0.0501).is_nan());
        assert!(axial.slope_wb_per_m(f64::NAN).is_nan());
        let tilted = SpatialPickupProfile {
            offset_xy_m: [0.0005, 0.0002],
            ..APERTURE_PICKUP
        };
        assert!(AxialAperture::new(tilted).is_err());
        assert!(SpatialPickup::new(tilted).is_ok());
    }

    #[test]
    fn aperture_voltage_matches_the_flux_time_derivative_and_faults_outside_its_domain() {
        let pickup = AxialAperture::new(APERTURE_PICKUP).unwrap();
        let full = SpatialPickup::new(APERTURE_PICKUP).unwrap();
        let flux = |x: f64| full.flux_wb([x, 0.0]).unwrap() / APERTURE_PICKUP.flux_scale_wb;
        for x in [-0.0012, -0.0004, 0.0, 0.0003, 0.0009] {
            let h = 1e-7;
            let slope = (flux(x + h) - flux(x - h)) / (2.0 * h);
            let v = 0.37;
            let expected = -0.015 * slope * v;
            let actual = aperture_voltage(&pickup, x, v);
            assert!(
                (actual - expected).abs() < 1e-6 * expected.abs().max(1e-3),
                "{x}"
            );
            assert_eq!(aperture_voltage(&pickup, x, 0.0), 0.0);
            assert_eq!(aperture_voltage(&pickup, x, -v), -actual);
        }
        // Outside the pole face the flux falls with distance and the sign follows the
        // production law. Inside it the node ring makes the flux rise toward the
        // ring, so near the centre the slope, and the voltage sign, reverse.
        let production = MagneticPickup::new(0.0005, 0.0005).unwrap();
        assert_eq!(
            aperture_voltage(&pickup, 0.003, 1.0).signum(),
            production.voltage(0.003, 1.0).signum()
        );
        assert_eq!(
            aperture_voltage(&pickup, 0.0002, 1.0).signum(),
            -production.voltage(0.0002, 1.0).signum()
        );
        assert!(aperture_voltage(&pickup, 0.06, 1.0).is_nan());
    }
}
