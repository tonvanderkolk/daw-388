//! Building blocks: a fixed delay line and a Schroeder allpass.

/// Values this small in a feedback path are set to 0, so the CPU never meets denormals.
pub(crate) fn flush(x: f32) -> f32 {
    if x.abs() < 1e-15 { 0.0 } else { x }
}

/// A ring buffer of fixed length, allocated once.
#[derive(Debug, Clone)]
pub(crate) struct DelayLine {
    buffer: Vec<f32>,
    write: usize,
}

impl DelayLine {
    pub(crate) fn new(length: usize) -> Self {
        Self { buffer: vec![0.0; length.max(1)], write: 0 }
    }

    pub(crate) fn len(&self) -> usize {
        self.buffer.len()
    }

    /// The sample written `delay` samples ago (0: the last one), clamped to the length.
    pub(crate) fn tap(&self, delay: usize) -> f32 {
        let len = self.buffer.len();
        let delay = delay.min(len - 1);
        self.buffer[(self.write + len - 1 - delay) % len]
    }

    /// Linear interpolation between two taps, for a delay in fractional samples.
    pub(crate) fn tap_frac(&self, delay: f32) -> f32 {
        let whole = delay.max(0.0).floor();
        let frac = delay.max(0.0) - whole;
        let a = self.tap(whole as usize);
        let b = self.tap(whole as usize + 1);
        a + (b - a) * frac
    }

    pub(crate) fn push(&mut self, x: f32) {
        self.buffer[self.write] = x;
        self.write = (self.write + 1) % self.buffer.len();
    }

    /// The oldest sample, then `x` in its place: a delay of the full length.
    pub(crate) fn shift(&mut self, x: f32) -> f32 {
        let out = self.buffer[self.write];
        self.push(x);
        out
    }
}

/// A Schroeder allpass of a fixed delay: flat in level, smearing in time.
#[derive(Debug, Clone)]
pub(crate) struct Allpass {
    line: DelayLine,
    gain: f32,
}

impl Allpass {
    pub(crate) fn new(length: usize, gain: f32) -> Self {
        Self { line: DelayLine::new(length), gain }
    }

    pub(crate) fn line(&self) -> &DelayLine {
        &self.line
    }

    pub(crate) fn process(&mut self, x: f32) -> f32 {
        let delayed = self.line.tap(self.line.len() - 1);
        let v = flush(x - self.gain * delayed);
        self.line.push(v);
        delayed + self.gain * v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_delay_line_gives_an_impulse_back_after_its_length() {
        let mut line = DelayLine::new(10);
        let out: Vec<f32> = (0..20).map(|i| line.shift(if i == 0 { 1.0 } else { 0.0 })).collect();
        assert_eq!(out.iter().position(|s| *s == 1.0), Some(10));
    }

    #[test]
    fn a_fractional_tap_lies_between_two_samples() {
        let mut line = DelayLine::new(8);
        for x in [0.0, 0.0, 2.0, 4.0] {
            line.push(x);
        }
        assert_eq!(line.tap(0), 4.0);
        assert!((line.tap_frac(0.5) - 3.0).abs() < 1e-6);
    }

    #[test]
    fn an_allpass_keeps_the_energy() {
        let mut allpass = Allpass::new(37, 0.7);
        let energy: f32 = (0..10_000).map(|i| allpass.process(if i == 0 { 1.0 } else { 0.0 })).map(|y| y * y).sum();
        assert!((energy - 1.0).abs() < 0.01, "{energy}");
    }
}
