//! A plate reverb after Dattorro's topology ("Effect Design, Part 1", 1997): pre-delay, input
//! diffusion and a figure-eight tank whose taps give a decorrelated stereo output.

use crate::delay::{Allpass, DelayLine, flush};

/// The plate's controls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlateParams {
    /// Reverb time in seconds, 0.5 to 6.
    pub decay_s: f32,
    /// 0 to 100 ms.
    pub predelay_ms: f32,
    /// 0 dark to 1 bright.
    pub tone: f32,
}

impl Default for PlateParams {
    fn default() -> Self {
        Self { decay_s: 2.0, predelay_ms: 20.0, tone: 0.5 }
    }
}

/// Dattorro's lengths are at 29 761 Hz.
const REFERENCE_RATE: f32 = 29_761.0;
/// Seconds one trip round half the tank takes, for turning a reverb time into a gain.
const HALF_LOOP_S: f32 = (672.0 + 4453.0 + 1800.0 + 3720.0) / REFERENCE_RATE;

/// One half of the tank: allpass, delay, damping, decay, allpass, delay.
#[derive(Debug, Clone)]
struct Half {
    diffuse: Allpass,
    first: DelayLine,
    damp: f32,
    second_allpass: Allpass,
    second: DelayLine,
}

/// A plate reverb: mono in, stereo out.
#[derive(Debug, Clone)]
pub struct Plate {
    pub params: PlateParams,
    rate: f32,
    predelay: DelayLine,
    bandwidth: f32,
    inputs: [Allpass; 4],
    halves: [Half; 2],
    /// The end of each half, fed into the other on the next sample.
    ends: [f32; 2],
    /// Output taps, in samples at this rate: left from each half, then right.
    taps: [[usize; 7]; 2],
}

impl Plate {
    pub fn new(sample_rate: u32) -> Self {
        let rate = sample_rate as f32;
        let n = |samples: f32| ((samples * rate / REFERENCE_RATE).round() as usize).max(1);
        let half = |a: f32, d1: f32, b: f32, d2: f32| Half {
            diffuse: Allpass::new(n(a), -0.7),
            first: DelayLine::new(n(d1)),
            damp: 0.0,
            second_allpass: Allpass::new(n(b), 0.5),
            second: DelayLine::new(n(d2)),
        };
        let taps =
            [[266.0, 2974.0, 1913.0, 1996.0, 1990.0, 187.0, 1066.0].map(n), [353.0, 3627.0, 1228.0, 2673.0, 2111.0, 335.0, 121.0].map(n)];
        Self {
            params: PlateParams::default(),
            rate,
            predelay: DelayLine::new((0.1 * rate) as usize + 2),
            bandwidth: 0.0,
            inputs: [
                Allpass::new(n(142.0), 0.75),
                Allpass::new(n(107.0), 0.75),
                Allpass::new(n(379.0), 0.625),
                Allpass::new(n(277.0), 0.625),
            ],
            halves: [half(672.0, 4453.0, 1800.0, 3720.0), half(908.0, 4217.0, 2656.0, 3163.0)],
            ends: [0.0; 2],
            taps,
        }
    }

    pub fn process(&mut self, x: f32) -> [f32; 2] {
        let p = self.params;
        let decay = 10f32.powf(-3.0 * HALF_LOOP_S / p.decay_s.clamp(0.5, 6.0)).min(0.97);
        let damping = 0.7 - 0.65 * p.tone.clamp(0.0, 1.0);
        let wait = (p.predelay_ms.clamp(0.0, 100.0) * 0.001 * self.rate) as usize;
        self.predelay.push(x);
        let delayed = self.predelay.tap(wait);
        self.bandwidth = flush(self.bandwidth + 0.9995 * (delayed - self.bandwidth));
        let mut diffused = self.bandwidth;
        for allpass in &mut self.inputs {
            diffused = allpass.process(diffused);
        }
        let ends = self.ends;
        for (side, half) in self.halves.iter_mut().enumerate() {
            let fed = diffused + ends[1 - side] * decay;
            let a = half.diffuse.process(fed);
            let b = half.first.shift(a);
            half.damp = flush(b + damping * (half.damp - b));
            let c = half.second_allpass.process(half.damp * decay);
            self.ends[side] = half.second.shift(c);
        }
        // Each side mostly hears the other half, which decorrelates left and right.
        let [l, r] = &self.halves;
        let out = |own: &Half, other: &Half, t: &[usize; 7]| {
            other.first.tap(t[0]) + other.first.tap(t[1]) - other.second_allpass.line().tap(t[2]) + other.second.tap(t[3])
                - own.first.tap(t[4])
                - own.second_allpass.line().tap(t[5])
                - own.second.tap(t[6])
        };
        [0.6 * out(l, r, &self.taps[0]), 0.6 * out(r, l, &self.taps[1])]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse_response(params: PlateParams, n: usize) -> Vec<[f32; 2]> {
        let mut plate = Plate::new(48_000);
        plate.params = params;
        (0..n).map(|i| plate.process(if i == 0 { 1.0 } else { 0.0 })).collect()
    }

    fn energy(frames: &[[f32; 2]]) -> f32 {
        frames.iter().map(|[l, r]| l * l + r * r).sum()
    }

    #[test]
    fn an_impulse_gives_a_decaying_stereo_tail() {
        let out = impulse_response(PlateParams::default(), 4 * 48_000);
        assert!(energy(&out[..48_000]) > 0.0);
        assert!(out[..48_000].iter().any(|[l, r]| (l - r).abs() > 1e-4), "left and right differ");
        assert!(energy(&out[3 * 48_000..]) < 0.01 * energy(&out[..48_000]), "the tail dies away");
    }

    #[test]
    fn longer_decay_rings_longer() {
        let short = impulse_response(PlateParams { decay_s: 0.8, ..PlateParams::default() }, 2 * 48_000);
        let long = impulse_response(PlateParams { decay_s: 5.0, ..PlateParams::default() }, 2 * 48_000);
        let (s, l) = (energy(&short[48_000..]), energy(&long[48_000..]));
        assert!(l > 4.0 * s, "{s} {l}");
    }

    #[test]
    fn the_longest_brightest_plate_stays_bounded_and_falls_silent() {
        let mut plate = Plate::new(48_000);
        plate.params = PlateParams { decay_s: 6.0, tone: 1.0, predelay_ms: 100.0 };
        for i in 0..48_000 {
            let [l, r] = plate.process(if i % 100 == 0 { 1.0 } else { 0.0 });
            assert!(l.abs() < 4.0 && r.abs() < 4.0 && l.is_finite(), "{i}: {l} {r}");
        }
        let mut last = [1.0f32; 2];
        for _ in 0..60 * 48_000 {
            last = plate.process(0.0);
        }
        assert_eq!(last, [0.0, 0.0], "silence in, silence out");
    }

    #[test]
    fn predelay_holds_back_the_first_sound() {
        let out = impulse_response(PlateParams { predelay_ms: 50.0, ..PlateParams::default() }, 48_000);
        assert!(out[..2_300].iter().all(|[l, r]| *l == 0.0 && *r == 0.0), "nothing in the first 48 ms");
    }
}
