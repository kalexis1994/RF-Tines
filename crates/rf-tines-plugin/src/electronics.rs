//! Bounded, allocation-free front-panel electronics. Voicing approximation,
//! not a component-level recreation of a historical preamplifier.
use crate::Settings;
use std::f64::consts::TAU;

const OUTPUT_CEILING: f32 = 0.95;

pub struct Electronics {
    rate: f64,
    smoothing: f64,
    output_release: f64,
    output_gain: f64,
    low_coefficient: f64,
    high_coefficient: f64,
    low: f64,
    high: f64,
    stage_low: f64,
    phase: f64,
    current: [f64; 6],
    target: [f64; 6],
}

impl Default for Electronics {
    fn default() -> Self {
        Self::new(48000.0, Settings::default())
    }
}

impl Electronics {
    pub fn new(rate: f64, settings: Settings) -> Self {
        let mut result = Self {
            rate,
            smoothing: 1.0 - (-1.0 / (0.01 * rate)).exp(),
            output_release: 1.0 - (-1.0 / (0.05 * rate)).exp(),
            output_gain: 1.0,
            low_coefficient: 1.0 - (-TAU * 200.0 / rate).exp(),
            high_coefficient: 1.0 - (-TAU * 2500.0 / rate).exp(),
            low: 0.0,
            high: 0.0,
            stage_low: 0.0,
            phase: 0.0,
            current: [0.0; 6],
            target: [0.0; 6],
        };
        result.reset(settings);
        result
    }

    pub fn target(&mut self, s: Settings) {
        self.target = [
            10.0_f64.powf(s.bass_db / 20.0),
            10.0_f64.powf(s.treble_db / 20.0),
            s.vibrato * s.intensity * s.preamp,
            s.speed_hz,
            s.preamp,
            s.bass_boost,
        ];
    }

    pub fn reset(&mut self, settings: Settings) {
        self.target(settings);
        self.current = self.target;
        self.low = 0.0;
        self.high = 0.0;
        self.stage_low = 0.0;
        self.phase = 0.0;
        self.output_gain = 1.0;
    }

    pub fn process(&mut self, sample: f32) -> [f32; 2] {
        for (value, target) in self.current.iter_mut().zip(self.target) {
            *value += self.smoothing * (target - *value);
            if (*value - target).abs() < 1e-12 {
                *value = target;
            }
        }
        let [bass, treble, depth, speed, preamp, boost] = self.current;
        let x = f64::from(sample);
        self.low += self.low_coefficient * (x - self.low);
        let low_shelf = x + (bass - 1.0) * self.low;
        self.high += self.high_coefficient * (low_shelf - self.high);
        let suitcase = low_shelf + (treble - 1.0) * (low_shelf - self.high);
        self.stage_low += self.low_coefficient * (x - self.stage_low);
        // Passive-style bass restoration: full clockwise is flat, never boosted.
        let stage = x - (1.0 - boost) * self.stage_low;
        let tone = stage + preamp * (suitcase - stage);
        let movement = if depth == 0.0 { 0.0 } else { self.phase.sin() };
        self.phase = (self.phase + TAU * speed / self.rate) % TAU;
        // Complementary amplitude modulation: no channel is amplified by the LFO.
        // A mono host gets the left channel, retaining tremolo. Stereo downmix
        // cancels movement and retains the depth-dependent mean attenuation.
        [
            (tone * (1.0 - 0.5 * depth + 0.5 * depth * movement)) as f32,
            (tone * (1.0 - 0.5 * depth - 0.5 * depth * movement)) as f32,
        ]
    }

    pub fn protect_output(&mut self, [left, right]: [f32; 2]) -> [f32; 2] {
        let peak = left.abs().max(right.abs());
        if peak > OUTPUT_CEILING || self.output_gain < 1.0 {
            // Protect both channels with the same gain to preserve the stereo image.
            self.output_gain += self.output_release * (1.0 - self.output_gain);
            if self.output_gain > 0.99999 {
                self.output_gain = 1.0;
            }
            if f64::from(peak) * self.output_gain > f64::from(OUTPUT_CEILING) {
                self.output_gain = f64::from(OUTPUT_CEILING) / f64::from(peak);
            }
            let gain = self.output_gain as f32;
            return [left * gain, right * gain];
        }
        [left, right]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neutral_is_bit_exact_and_stage_is_mono() {
        for rate in [44100.0, 48000.0, 96000.0] {
            let mut fx = Electronics::new(rate, Settings::default());
            for i in 0..4000 {
                let x = (i as f32 * 0.117).sin();
                assert_eq!(fx.process(x), [x, x]);
            }
            fx.reset(Settings {
                preamp: 0.0,
                bass_boost: 0.0,
                vibrato: 1.0,
                intensity: 1.0,
                ..Settings::default()
            });
            for _ in 0..4000 {
                let [l, r] = fx.process(1.0);
                assert_eq!(l, r);
            }
            assert!(fx.process(1.0)[0].abs() < 1e-6);
        }
    }

    #[test]
    fn overload_protection_is_stereo_linked_and_recovers_smoothly() {
        let settings = Settings {
            vibrato: 1.0,
            intensity: 1.0,
            ..Settings::default()
        };
        let mut reference = Electronics::new(48000.0, settings);
        let mut guarded = Electronics::new(48000.0, settings);
        for _ in 0..1500 {
            reference.process(0.0);
            guarded.process(0.0);
        }
        let [left, right] = reference.process(1.0);
        assert!(left > right && right > 0.0);
        let louder = guarded.process(2.0);
        let [limited_left, limited_right] = guarded.protect_output(louder);
        assert!((limited_left - OUTPUT_CEILING).abs() < 1e-6);
        assert!((limited_right - OUTPUT_CEILING * right / left).abs() < 1e-6);
        let gain = guarded.output_gain;
        guarded.protect_output([0.25, 0.25]);
        assert!(guarded.output_gain > gain && guarded.output_gain - gain < 0.001);
        for _ in 0..48000 {
            guarded.protect_output([0.25, 0.25]);
        }
        assert_eq!(guarded.output_gain, 1.0);
    }
    #[test]
    fn stereo_tremolo_has_bounded_complementary_channels_and_requested_period() {
        let mut fx = Electronics::new(
            48000.0,
            Settings {
                vibrato: 1.0,
                intensity: 1.0,
                speed_hz: 4.0,
                ..Settings::default()
            },
        );
        let mut extrema = [1.0_f32, 0.0_f32];
        for i in 0..12000 {
            let [l, r] = fx.process(1.0);
            assert!((l + r - 1.0).abs() < 1e-6);
            extrema[0] = extrema[0].min(l);
            extrema[1] = extrema[1].max(l);
            if i == 3000 {
                assert!((l - 1.0).abs() < 1e-6);
            }
            if i == 9000 {
                assert!(l.abs() < 1e-6);
            }
        }
        assert!(extrema[0] < 1e-6 && extrema[1] > 0.99999);
        fx.target(Settings::default());
        let mut previous = fx.process(1.0)[0];
        for _ in 0..16000 {
            let next = fx.process(1.0)[0];
            assert!((next - previous).abs() < 0.005);
            previous = next;
        }
        assert_eq!(fx.process(0.25), [0.25, 0.25]);
    }
    #[test]
    fn shelf_response_tracks_polarity_and_stays_finite_at_extremes() {
        fn rms(hz: f64, bass: f64, treble: f64, rate: f64) -> f64 {
            let mut fx = Electronics::new(
                rate,
                Settings {
                    bass_db: bass,
                    treble_db: treble,
                    ..Settings::default()
                },
            );
            let mut sum = 0.0;
            for i in 0..24000 {
                let x = (TAU * hz * i as f64 / rate).sin() as f32;
                let y = fx.process(x)[0];
                assert!(y.is_finite());
                if i >= 12000 {
                    sum += f64::from(y).powi(2);
                }
            }
            (sum / 12000.0).sqrt()
        }
        for rate in [44100.0, 48000.0, 96000.0] {
            assert!(rms(40.0, 12.0, 0.0, rate) > 2.5);
            assert!(rms(40.0, -12.0, 0.0, rate) < 0.3);
            assert!(rms(10000.0, 0.0, 12.0, rate) > 1.5);
            assert!(rms(10000.0, 0.0, -12.0, rate) < 0.5);
            assert!(rms(1000.0, 12.0, 12.0, rate) < 16.0);
        }
    }
}
