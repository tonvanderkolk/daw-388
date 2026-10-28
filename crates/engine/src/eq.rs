//! The 388's channel EQ: three peaking filters (LOW, MID and HIGH), each with a sweepable
//! frequency and ±15 dB. The engine gets frequency and gain, not coefficients, and works them
//! out at its own sample rate.

use crate::smooth;
use std::f64::consts::PI;

/// One band's settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqBand {
    pub freq: f32,
    pub gain_db: f32,
}

/// Flat, at the 388's centre frequencies: LOW, MID, HIGH.
pub const EQ_DEFAULTS: [EqBand; 3] =
    [EqBand { freq: 100.0, gain_db: 0.0 }, EqBand { freq: 1000.0, gain_db: 0.0 }, EqBand { freq: 8000.0, gain_db: 0.0 }];

/// The bands' Q: broad and musical, like the 388's fixed-Q bands.
const Q: f64 = 0.9;
/// Coefficients are recomputed from the smoothed settings every this many samples.
const UPDATE: u32 = 32;

/// A peaking biquad (RBJ cookbook), transposed direct form II.
#[derive(Debug, Default, Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    state: [f64; 2],
}

impl Biquad {
    fn set(&mut self, freq: f64, gain_db: f64, rate: f64) {
        let a = 10f64.powf(gain_db / 40.0);
        let w = 2.0 * PI * freq.clamp(10.0, 0.45 * rate) / rate;
        let alpha = w.sin() / (2.0 * Q);
        let cos = w.cos();
        let a0 = 1.0 + alpha / a;
        self.b = [(1.0 + alpha * a) / a0, -2.0 * cos / a0, (1.0 - alpha * a) / a0];
        self.a = [-2.0 * cos / a0, (1.0 - alpha / a) / a0];
    }

    fn process(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.state[0];
        self.state[0] = self.b[1] * x - self.a[0] * y + self.state[1];
        self.state[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// The three bands with their smoothed settings.
#[derive(Debug)]
pub(crate) struct Eq {
    pub(crate) target: [EqBand; 3],
    current: [EqBand; 3],
    filters: [Biquad; 3],
    rate: f64,
    countdown: u32,
}

impl Eq {
    pub(crate) fn new(sample_rate: u32) -> Self {
        Self { target: EQ_DEFAULTS, current: EQ_DEFAULTS, filters: Default::default(), rate: f64::from(sample_rate), countdown: 0 }
    }

    /// Run one sample through the bands. A band that is flat and has settled is skipped (and
    /// cleared, so it starts afresh), so a flat EQ passes the signal unchanged.
    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let update = self.countdown == 0;
        self.countdown = if update { UPDATE - 1 } else { self.countdown - 1 };
        let mut y = f64::from(x);
        let mut touched = false;
        for ((band, target), filter) in self.current.iter_mut().zip(self.target).zip(&mut self.filters) {
            smooth(&mut band.freq, target.freq);
            smooth(&mut band.gain_db, target.gain_db);
            if band.gain_db == 0.0 && target.gain_db == 0.0 {
                *filter = Biquad::default();
                continue;
            }
            if update || filter.b == [0.0; 3] {
                filter.set(f64::from(band.freq), f64::from(band.gain_db), self.rate);
            }
            y = filter.process(y);
            touched = true;
        }
        if touched { y as f32 } else { x }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn sine(freq: f32, n: usize) -> impl Iterator<Item = f32> {
        (0..n).map(move |i| 0.25 * (2.0 * std::f32::consts::PI * freq * i as f32 / RATE as f32).sin())
    }

    /// Gain in dB over the last tenth of a second, after the settings and filter settled.
    fn gain_at(bands: [EqBand; 3], freq: f32) -> f32 {
        let mut eq = Eq::new(RATE);
        eq.target = bands;
        let samples: Vec<f32> = sine(freq, RATE as usize).map(|x| eq.process(x)).collect();
        let peak = samples[samples.len() - 4800..].iter().fold(0.0f32, |p, s| p.max(s.abs()));
        20.0 * (peak / 0.25).log10()
    }

    #[test]
    fn a_band_boosts_and_cuts_15_db_at_its_frequency() {
        let mut bands = EQ_DEFAULTS;
        bands[1].gain_db = 15.0;
        assert!((gain_at(bands, 1000.0) - 15.0).abs() < 0.5);
        bands[1].gain_db = -15.0;
        assert!((gain_at(bands, 1000.0) + 15.0).abs() < 0.5);
    }

    #[test]
    fn a_flat_eq_passes_the_signal_bit_for_bit() {
        let mut eq = Eq::new(RATE);
        for x in sine(440.0, 4800) {
            assert_eq!(eq.process(x).to_bits(), x.to_bits());
        }
    }

    #[test]
    fn sweeping_a_boosted_band_does_not_click() {
        let mut eq = Eq::new(RATE);
        eq.target[1].gain_db = 15.0;
        let mut previous = 0.0f32;
        let mut biggest = 0.0f32;
        for (i, x) in sine(1000.0, 2 * RATE as usize).enumerate() {
            // Sweep MID from 500 Hz to 5 kHz in jumps, as a fast hand on the knob would.
            if i % 480 == 0 {
                eq.target[1].freq = 500.0 + 4500.0 * (i as f32 / (2 * RATE) as f32);
            }
            let y = eq.process(x);
            assert!(y.is_finite());
            if i > 0 {
                biggest = biggest.max((y - previous).abs());
            }
            previous = y;
        }
        // A 1 kHz sine of 0.25 boosted 15 dB changes at most about 0.18 per sample at 48 kHz.
        assert!(biggest < 0.4, "{biggest}");
    }
}
