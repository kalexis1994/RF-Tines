use crate::SpatialPickupProfile;
use crate::laboratory::{APERTURE_PICKUP, PlanarAperture, planar_voltage};

const CELL_COUNT: usize = 512;
const GRID_COUNT: usize = 128;
const LONGITUDINAL_STEPS: f64 = 48.0;
const TRANSVERSE_STEPS: f64 = 1024.0;
const REFILL_INTERVAL: u8 = 32;
const VOLTAGE_SCALE: f64 = -0.015 / APERTURE_PICKUP.flux_scale_wb;
// The near-axis strip out to 4.67 gaps includes the measured attack travel.
const PRIME_COLUMNS: u32 = 224;

#[derive(Clone, Copy)]
struct Cell {
    tag: u64,
    // [degree][base or row slope][longitudinal or transverse]
    gradient: [[[f64; 2]; 2]; 4],
}

impl Cell {
    const EMPTY: Self = Self {
        tag: 0,
        gradient: [[[0.0; 2]; 2]; 4],
    };
}

#[derive(Clone, Copy)]
struct GridPoint {
    tag: u64,
    gradient: [f64; 2],
}

impl GridPoint {
    const EMPTY: Self = Self {
        tag: 0,
        gradient: [0.0; 2],
    };
}

/// Sparse, per-key interpolation of the analytic pickup gradient. Storage is
/// reserved at prepare time, including for a later switch to planar Aperture.
pub(crate) struct PlanarTable {
    cells: Box<[Cell; CELL_COUNT]>,
    grid: Box<[GridPoint; GRID_COUNT]>,
    generation: u32,
    cooldown: u8,
    prime_column: u32,
    geometry: SpatialPickupProfile,
    inverse_gap: f64,
}

impl PlanarTable {
    pub(crate) fn new(note: u8, geometry: SpatialPickupProfile) -> Self {
        Self {
            cells: vec![Cell::EMPTY; CELL_COUNT]
                .into_boxed_slice()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
            grid: vec![GridPoint::EMPTY; GRID_COUNT]
                .into_boxed_slice()
                .try_into()
                .unwrap_or_else(|_| unreachable!()),
            generation: 1,
            cooldown: note % REFILL_INTERVAL,
            prime_column: 0,
            geometry,
            inverse_gap: geometry.gap_m.recip(),
        }
    }

    pub(crate) fn configure(&mut self, note: u8, geometry: SpatialPickupProfile) {
        self.geometry = geometry;
        self.inverse_gap = geometry.gap_m.recip();
        self.clear(note);
    }

    pub(crate) fn clear(&mut self, note: u8) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.cells.fill(Cell::EMPTY);
            self.grid.fill(GridPoint::EMPTY);
            self.generation = 1;
        }
        self.cooldown = note % REFILL_INTERVAL;
        self.prime_column = 0;
    }

    pub(crate) fn prime(&mut self, pickup: &PlanarAperture) -> bool {
        if self.prime_column >= PRIME_COLUMNS {
            return false;
        }
        let column = self.prime_column;
        self.prime_column += 1;
        let (index, tag) = cell_address(self.generation, column, 0);
        if self.cells[index].tag != tag {
            self.cells[index] = self.build_cell(pickup, tag, column, 0);
        }
        true
    }

    pub(crate) fn voltage(
        &mut self,
        pickup: &PlanarAperture,
        position: [f64; 2],
        velocity: [f64; 2],
        allow_refill: bool,
    ) -> f64 {
        self.cooldown = self.cooldown.saturating_sub(u8::from(allow_refill));
        let [offset_x, offset_y] = self.geometry.offset_xy_m;
        let longitudinal = (position[0] + offset_x) * self.inverse_gap;
        let transverse = (position[1] + offset_y) * self.inverse_gap;
        let x = longitudinal.abs() * LONGITUDINAL_STEPS;
        let y = transverse * transverse * TRANSVERSE_STEPS;
        // These bounds also keep the interpolation stencil inside the
        // analytic domain at the largest supported gap and offset.
        if !(x <= 8.0 * LONGITUDINAL_STEPS && y <= TRANSVERSE_STEPS) {
            return planar_voltage(pickup, position, velocity);
        }

        let column = x as u32;
        let row = y as u32;
        let (index, tag) = cell_address(self.generation, column, row);
        if self.cells[index].tag != tag {
            // Cold and colliding cells use the exact field until there is
            // refill capacity. Note-dependent offsets stagger the work.
            if !allow_refill || self.cooldown != 0 {
                return planar_voltage(pickup, position, velocity);
            }
            self.cooldown = REFILL_INTERVAL;
            self.cells[index] = self.build_cell(pickup, tag, column, row);
        }
        let cell = &self.cells[index];
        let x_fraction = x - f64::from(column);
        let y_fraction = y - f64::from(row);
        let [gx, gy] = evaluate(&cell.gradient, x_fraction, y_fraction);
        let gx = if longitudinal < 0.0 { -gx } else { gx };
        let gy = gy * transverse;
        VOLTAGE_SCALE * (gx * velocity[0] + gy * velocity[1])
    }

    #[cold]
    fn build_cell(&mut self, pickup: &PlanarAperture, tag: u64, column: u32, row: u32) -> Cell {
        let samples: [[_; 4]; 2] = core::array::from_fn(|j| {
            core::array::from_fn(|i| self.grid_gradient(pickup, column + i as u32, row + j as u32))
        });
        let [near, far] = samples
            .map(|line| [0, 1].map(|axis| cubic_coefficients(line.map(|sample| sample[axis]))));
        let gradient = core::array::from_fn(|i| {
            let [nx, ny] = [near[0][i], near[1][i]];
            [[nx, ny], [far[0][i] - nx, far[1][i] - ny]]
        });
        Cell { tag, gradient }
    }

    fn grid_gradient(&mut self, pickup: &PlanarAperture, column: u32, row: u32) -> [f64; 2] {
        // Adjacent interpolation cells share grid points. Reuse the same
        // analytic evaluations without changing the interpolation stencil.
        let (hash, tag) = grid_address(self.generation, column, row);
        let point = &mut self.grid[hash & (GRID_COUNT - 1)];
        if point.tag != tag {
            let [offset_x, offset_y] = self.geometry.offset_xy_m;
            let x = (f64::from(column) - 1.0) * (self.geometry.gap_m / LONGITUDINAL_STEPS);
            // The symmetric field makes Gx and Gy/y smooth in y squared.
            // Sample just off-axis for the removable singularity of Gy/y at zero.
            let y = (f64::from(row) / TRANSVERSE_STEPS).sqrt().max(1e-5);
            let [gx, gy] =
                pickup.gradient_wb_per_m([x - offset_x, y * self.geometry.gap_m - offset_y]);
            *point = GridPoint {
                tag,
                gradient: [gx, gy / y],
            };
        }
        point.gradient
    }
}

#[inline]
fn cell_address(generation: u32, column: u32, row: u32) -> (usize, u64) {
    let (hash, tag) = grid_address(generation, column, row);
    (hash & (CELL_COUNT - 1), tag)
}

#[inline]
fn grid_address(generation: u32, column: u32, row: u32) -> (usize, u64) {
    let tag = (u64::from(generation) << 32) | (u64::from(column) << 16) | u64::from(row);
    let hash = column.wrapping_mul(0x5e4f_3419) ^ row.wrapping_mul(0x31d7_6c4d);
    (hash as usize, tag)
}

fn cubic_coefficients([a, b, c, d]: [f64; 4]) -> [f64; 4] {
    // Four-point Lagrange polynomial at nodes -1, 0, 1 and 2.
    let left = b - a;
    let right = c - b;
    let far = d - c;
    [
        b,
        (2.0 * left + 5.0 * right - far) / 6.0,
        (right - left) * 0.5,
        (far - 2.0 * right + left) / 6.0,
    ]
}

#[inline]
fn evaluate(coefficients: &[[[f64; 2]; 2]; 4], x: f64, y: f64) -> [f64; 2] {
    let [a, b, c, d] =
        coefficients.map(|[base, slope]| [base[0] + y * slope[0], base[1] + y * slope[1]]);
    [
        ((d[0] * x + c[0]) * x + b[0]) * x + a[0],
        ((d[1] * x + c[1]) * x + b[1]) * x + a[1],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolated_voltage_tracks_the_analytic_field_across_geometry_corners() {
        for gap_m in [0.0005, 0.0015, 0.005 * 0.135_f64.exp()] {
            for pole_radius_m in [0.0002, 0.00175, 0.003] {
                for pole_wedge in [0.0, 0.35, 1.0] {
                    let geometry = SpatialPickupProfile {
                        gap_m,
                        offset_xy_m: [-0.003, 0.003],
                        pole_radius_m,
                        pole_wedge,
                        ..APERTURE_PICKUP
                    };
                    let pickup = PlanarAperture::new(geometry).unwrap();
                    let mut table = PlanarTable::new(60, geometry);
                    for x in [
                        -7.999, -4.31, -2.23, -1.02, -0.137, 0.0, 0.673, 2.149, 7.999,
                    ] {
                        for y in [-0.999, -0.53, -0.078, 0.0, 0.027, 0.149, 0.772, 0.999] {
                            let position = [x * gap_m + 0.003, y * gap_m - 0.003];
                            for velocity in [[1.0, 0.0], [0.0, 1.0], [0.8, -0.6]] {
                                // Force a refill or hit so the analytic fallback
                                // cannot mask an inaccurate interpolant.
                                table.cooldown = 0;
                                let actual = table.voltage(&pickup, position, velocity, true);
                                let exact = planar_voltage(&pickup, position, velocity);
                                assert!(
                                    (actual - exact).abs() * gap_m / 0.015 < 2e-6,
                                    "gap={gap_m} radius={pole_radius_m} wedge={pole_wedge} x={x} y={y}"
                                );
                            }
                        }
                    }
                    let position = [0.04, 0.03];
                    let velocity = [1.0, 1.0];
                    assert_eq!(
                        table.voltage(&pickup, position, velocity, true),
                        planar_voltage(&pickup, position, velocity)
                    );
                }
            }
        }
    }

    #[test]
    fn contact_reads_primed_cells_and_leaves_cold_cells_untouched() {
        let geometry = APERTURE_PICKUP;
        let pickup = PlanarAperture::new(geometry).unwrap();
        let mut table = PlanarTable::new(64, geometry);
        let position = [
            0.25 * geometry.gap_m / LONGITUDINAL_STEPS - geometry.offset_xy_m[0],
            0.0,
        ];
        let velocity = [1.0, 0.0];
        let exact = planar_voltage(&pickup, position, velocity);
        let (index, tag) = cell_address(table.generation, 0, 0);
        assert_eq!(table.voltage(&pickup, position, velocity, false), exact);
        assert_ne!(table.cells[index].tag, tag);
        assert!(table.prime(&pickup));
        assert_eq!(table.cells[index].tag, tag);
        let warm = table.voltage(&pickup, position, velocity, false);
        assert!((warm - exact).abs() * geometry.gap_m / 0.015 < 2e-6);
        assert_eq!(table.cooldown, 0);
        table.clear(64);
        assert_eq!(table.voltage(&pickup, position, velocity, false), exact);
    }
}
