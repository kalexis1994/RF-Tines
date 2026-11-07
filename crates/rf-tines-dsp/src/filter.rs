use core::f64::consts::PI;

const TAPS: usize = 127;

/// Four-times-rate windowed-sinc FIR; 63 internal samples of group delay.
/// Blackman window, cutoff at 0.105 cycles/internal sample (0.42 output Fs).
pub struct Decimator {
    taps: [f64; TAPS],
    history: [f64; TAPS],
    cursor: usize,
}

impl Default for Decimator {
    fn default() -> Self {
        Self::new()
    }
}

impl Decimator {
    pub fn new() -> Self {
        let mut taps = [0.0; TAPS];
        for (i, tap) in taps.iter_mut().enumerate() {
            let x = i as f64 - (TAPS - 1) as f64 / 2.0;
            let window = 0.42 - 0.5 * (2.0 * PI * i as f64 / (TAPS - 1) as f64).cos()
                + 0.08 * (4.0 * PI * i as f64 / (TAPS - 1) as f64).cos();
            *tap = if x == 0.0 {
                0.21
            } else {
                (2.0 * PI * 0.105 * x).sin() / (PI * x)
            } * window;
        }
        let sum: f64 = taps.iter().sum();
        for tap in &mut taps {
            *tap /= sum;
        }
        Self {
            taps,
            history: [0.0; TAPS],
            cursor: 0,
        }
    }

    pub fn push(&mut self, sample: f64) {
        self.history[self.cursor] = sample;
        self.cursor = (self.cursor + 1) % TAPS;
    }

    pub fn output(&self) -> f64 {
        let (recent, wrapped) = self.history.split_at(self.cursor);
        // Read newest to oldest across the wrap, in the original sum order.
        self.taps
            .iter()
            .zip(recent.iter().rev().chain(wrapped.iter().rev()))
            .map(|(tap, sample)| tap * sample)
            .sum()
    }

    pub fn clear(&mut self) {
        self.history.fill(0.0);
        self.cursor = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_passes_dc_and_rejects_above_output_nyquist() {
        let filter = Decimator::new();
        assert!((filter.taps.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        for frequency in [0.15, 0.2, 0.3, 0.4, 0.5] {
            let re: f64 = filter
                .taps
                .iter()
                .enumerate()
                .map(|(i, t)| t * (2.0 * PI * frequency * i as f64).cos())
                .sum();
            let im: f64 = filter
                .taps
                .iter()
                .enumerate()
                .map(|(i, t)| t * (2.0 * PI * frequency * i as f64).sin())
                .sum();
            assert!(re.hypot(im) < 0.0001, "stopband at {frequency}");
        }
    }
}
