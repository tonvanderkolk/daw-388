//! Small filters for the tape's colour: an RBJ biquad and a first-order emphasis pair.

use std::f64::consts::PI;

/// A biquad (RBJ cookbook), transposed direct form II.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    state: [f64; 2],
}

impl Biquad {
    /// A peaking filter; `gain_db` at `freq`.
    pub(crate) fn peak(freq: f64, gain_db: f64, q: f64, rate: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w = 2.0 * PI * freq.clamp(10.0, 0.45 * rate) / rate;
        let alpha = w.sin() / (2.0 * q);
        let cos = w.cos();
        let a0 = 1.0 + alpha / a;
        Self {
            b: [(1.0 + alpha * a) / a0, -2.0 * cos / a0, (1.0 - alpha * a) / a0],
            a: [-2.0 * cos / a0, (1.0 - alpha / a) / a0],
            state: [0.0; 2],
        }
    }

    /// A second-order lowpass, clamped below the Nyquist frequency.
    pub(crate) fn lowpass(freq: f64, q: f64, rate: f64) -> Self {
        let w = 2.0 * PI * freq.clamp(10.0, 0.45 * rate) / rate;
        let alpha = w.sin() / (2.0 * q);
        let cos = w.cos();
        let a0 = 1.0 + alpha;
        let b = (1.0 - cos) / 2.0 / a0;
        Self { b: [b, 2.0 * b, b], a: [-2.0 * cos / a0, (1.0 - alpha) / a0], state: [0.0; 2] }
    }

    /// The same filter with new coefficients, keeping its memory (no click on a change).
    pub(crate) fn retuned(self, other: Self) -> Self {
        Self { state: self.state, ..other }
    }

    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let x = f64::from(x);
        let y = self.b[0] * x + self.state[0];
        self.state[0] = self.b[1] * x - self.a[0] * y + self.state[1];
        self.state[1] = self.b[2] * x - self.a[1] * y;
        y as f32
    }
}

/// First-order pre-emphasis and its exact inverse: the highs reach the saturation sooner.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Emphasis {
    last_in: f32,
    last_out: f32,
}

const K: f32 = 0.5;

impl Emphasis {
    pub(crate) fn pre(&mut self, x: f32) -> f32 {
        let y = (1.0 + K) * x - K * self.last_in;
        self.last_in = x;
        y
    }

    pub(crate) fn de(&mut self, x: f32) -> f32 {
        let y = (x + K * self.last_out) / (1.0 + K);
        self.last_out = y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_gain_db(mut filter: Biquad, freq: f64) -> f64 {
        let rate = 48_000.0;
        let mut peak = 0.0f64;
        for i in 0..48_000 {
            let x = (2.0 * PI * freq * f64::from(i) / rate).sin() as f32;
            let y = filter.process(x);
            if i > 40_000 {
                peak = peak.max(f64::from(y.abs()));
            }
        }
        20.0 * peak.log10()
    }

    #[test]
    fn a_peak_lifts_its_frequency_and_a_lowpass_cuts_far_above() {
        assert!((sine_gain_db(Biquad::peak(60.0, 1.5, 1.0, 48_000.0), 60.0) - 1.5).abs() < 0.1);
        assert!(sine_gain_db(Biquad::lowpass(5_000.0, 0.707, 48_000.0), 18_000.0) < -10.0);
    }

    #[test]
    fn de_emphasis_undoes_pre_emphasis() {
        let (mut pre, mut de) = (Emphasis::default(), Emphasis::default());
        for i in 0..1_000 {
            let x = ((i * 7919) % 1000) as f32 / 1000.0 - 0.5;
            let y = de.de(pre.pre(x));
            assert!((y - x).abs() < 1e-5, "{i}: {x} {y}");
        }
    }
}
