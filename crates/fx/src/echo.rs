//! A tape echo: a delay with feedback through a darkening, slightly saturating loop and a little
//! wow. Turning TIME glides like a changing tape speed.

use crate::delay::{DelayLine, flush};
use std::f32::consts::TAU;

/// TIME follows the metronome when not `Off`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EchoSync {
    #[default]
    Off,
    Quarter,
    Eighth,
    DottedEighth,
}

/// The echo's controls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EchoParams {
    /// 40 to 800 ms when SYNC is off.
    pub time_ms: f32,
    pub sync: EchoSync,
    /// 0 to 0.9.
    pub feedback: f32,
    /// 0 dark to 1 bright.
    pub tone: f32,
}

impl Default for EchoParams {
    fn default() -> Self {
        Self { time_ms: 300.0, sync: EchoSync::Off, feedback: 0.35, tone: 0.5 }
    }
}

const MAX_SECONDS: f32 = 2.0;

impl EchoParams {
    /// The delay in seconds; `quarter` is a quarter note in seconds at the current tempo.
    pub fn seconds(&self, quarter: f32) -> f32 {
        let seconds = match self.sync {
            EchoSync::Off => self.time_ms.clamp(40.0, 800.0) / 1000.0,
            EchoSync::Quarter => quarter,
            EchoSync::Eighth => quarter / 2.0,
            EchoSync::DottedEighth => quarter * 0.75,
        };
        seconds.clamp(0.001, MAX_SECONDS)
    }
}

/// A tape echo: mono in, mono out, only the repeats.
#[derive(Debug, Clone)]
pub struct Echo {
    pub params: EchoParams,
    rate: f32,
    line: DelayLine,
    /// The delay in samples, gliding towards the target.
    delay: f32,
    lowpass: f32,
    /// The highpass's last input and output.
    highpass: [f32; 2],
    phase: f32,
}

impl Echo {
    pub fn new(sample_rate: u32) -> Self {
        let rate = sample_rate as f32;
        Self {
            params: EchoParams::default(),
            rate,
            line: DelayLine::new((MAX_SECONDS * rate) as usize + 64),
            delay: 0.0,
            lowpass: 0.0,
            highpass: [0.0; 2],
            phase: 0.0,
        }
    }

    /// Jump to the target delay at once (a new engine, tests).
    pub fn settle(&mut self, quarter: f32) {
        self.delay = self.params.seconds(quarter) * self.rate;
    }

    /// One sample; `quarter` is a quarter note in seconds at the current tempo.
    pub fn process(&mut self, x: f32, quarter: f32) -> f32 {
        let target = self.params.seconds(quarter) * self.rate;
        // About a third of a second to glide: a tape motor finding its new speed.
        self.delay += (target - self.delay) / (0.3 * self.rate);
        self.phase = (self.phase + 0.4 / self.rate).fract();
        let wow = 0.0003 * self.rate * (TAU * self.phase).sin();
        let delayed = self.line.tap_frac((self.delay + wow).max(1.0) - 1.0);
        // The loop: lowpass (TONE), a fixed highpass at 100 Hz, soft saturation.
        let cutoff = 1_500.0 + 7_500.0 * self.params.tone.clamp(0.0, 1.0);
        let a = 1.0 - (-TAU * cutoff / self.rate).exp();
        self.lowpass = flush(self.lowpass + a * (delayed - self.lowpass));
        let h = (-TAU * 100.0 / self.rate).exp();
        let high = flush(h * (self.highpass[1] + self.lowpass - self.highpass[0]));
        self.highpass = [self.lowpass, high];
        let fed = (high * self.params.feedback.clamp(0.0, 0.9)).tanh();
        self.line.push(flush(x + fed));
        delayed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The loudest sample in `out[from..to]` and where it is.
    fn peak(out: &[f32], from: usize, to: usize) -> (usize, f32) {
        (from..to).fold((0, 0.0f32), |best, i| if out[i].abs() > best.1 { (i, out[i].abs()) } else { best })
    }

    #[test]
    fn repeats_come_back_on_time_and_fade_with_feedback() {
        let mut echo = Echo::new(48_000);
        echo.params = EchoParams { time_ms: 100.0, feedback: 0.5, tone: 1.0, sync: EchoSync::Off };
        echo.settle(0.5);
        let out: Vec<f32> = (0..12_000).map(|i| echo.process(if i == 0 { 0.5 } else { 0.0 }, 0.5)).collect();
        let (at, first) = peak(&out, 0, 7_000);
        assert!((at as i32 - 4_800).abs() < 20, "the first repeat after 100 ms: {at}");
        let (at2, second) = peak(&out, 7_000, 12_000);
        assert!((at2 as i32 - 9_600).abs() < 40, "{at2}");
        assert!(second < first && second > 0.2 * first, "about FEEDBACK times the first: {first} {second}");
    }

    #[test]
    fn sync_follows_the_tempo() {
        let params = EchoParams { sync: EchoSync::Eighth, ..EchoParams::default() };
        assert!((params.seconds(0.5) - 0.25).abs() < 1e-6, "an eighth at 120 BPM");
        assert!((EchoParams { sync: EchoSync::DottedEighth, ..params }.seconds(0.5) - 0.375).abs() < 1e-6);
        assert!((EchoParams { sync: EchoSync::Quarter, ..params }.seconds(3.0) - 2.0).abs() < 1e-6, "capped at 2 s");
        assert!((EchoParams { time_ms: 300.0, sync: EchoSync::Off, ..params }.seconds(0.5) - 0.3).abs() < 1e-6);
    }

    #[test]
    fn full_feedback_stays_bounded() {
        let mut echo = Echo::new(48_000);
        echo.params = EchoParams { time_ms: 40.0, feedback: 0.9, tone: 1.0, sync: EchoSync::Off };
        for i in 0..20 * 48_000 {
            let y = echo.process(if i < 4_800 { 0.9 } else { 0.0 }, 0.5);
            assert!(y.is_finite() && y.abs() < 2.0, "{i}: {y}");
        }
    }
}
