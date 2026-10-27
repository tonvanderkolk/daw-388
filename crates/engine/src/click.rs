//! The metronome: a synthesised click on the beats of a bar grid that starts at the beginning of
//! the tape (so RESET does not move the rhythm). It goes to the output only, never into a
//! recording or the mix meters.

use std::f32::consts::TAU;

/// Length of one click in seconds.
const CLICK_SECONDS: f32 = 0.035;
/// Decay time constant of the click envelope.
const DECAY_SECONDS: f32 = 0.008;
/// Pitch of the first beat of a bar and of the other beats.
const ACCENT_HZ: f32 = 1_760.0;
const BEAT_HZ: f32 = 1_175.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter {
    /// Quarter notes per minute, as in Logic (also in 6/8).
    pub bpm: f64,
    /// Beats per bar (the upper number of the time signature).
    pub beats: u8,
    /// The note value of a beat (the lower number): 4 or 8.
    pub unit: u8,
}

impl Default for Meter {
    fn default() -> Self {
        Self { bpm: 120.0, beats: 4, unit: 4 }
    }
}

impl Meter {
    /// Length of one beat in seconds.
    pub fn beat_seconds(&self) -> f64 {
        60.0 / self.bpm.clamp(20.0, 400.0) * 4.0 / f64::from(self.unit.max(1))
    }

    /// Bar and beat (both from 1) at a tape position in seconds.
    pub fn bar_and_beat(&self, seconds: f64) -> (i64, u8) {
        let beat = (seconds / self.beat_seconds()).floor() as i64;
        let beats = i64::from(self.beats.max(1));
        (beat.div_euclid(beats) + 1, (beat.rem_euclid(beats) + 1) as u8)
    }
}

/// The click generator on the audio thread.
#[derive(Debug)]
pub(crate) struct Click {
    pub(crate) enabled: bool,
    /// Also click during plain playback, not only when recording or counting in.
    pub(crate) always: bool,
    pub(crate) meter: Meter,
    /// Linear level.
    pub(crate) volume: f32,
    /// Bars to count in before a recording from stop.
    pub(crate) count_in: u8,
    rate: f32,
    /// The beat (counted from the start of the tape) that last clicked.
    last_beat: Option<i64>,
    /// Samples left of the sounding click, its phase and its pitch.
    left: usize,
    phase: f32,
    hz: f32,
    age: usize,
}

impl Click {
    pub(crate) fn new(sample_rate: u32) -> Self {
        Self {
            enabled: false,
            always: false,
            meter: Meter::default(),
            volume: 0.5,
            count_in: 0,
            rate: sample_rate as f32,
            last_beat: None,
            left: 0,
            phase: 0.0,
            hz: BEAT_HZ,
            age: 0,
        }
    }

    pub(crate) fn beat_samples(&self) -> f64 {
        self.meter.beat_seconds() * f64::from(self.rate)
    }

    /// Samples in the count-in.
    pub(crate) fn count_in_samples(&self) -> f64 {
        f64::from(self.count_in) * f64::from(self.meter.beats.max(1)) * self.beat_samples()
    }

    /// The tape is at `position` (samples) and the click should sound: start a click on every
    /// new beat. A start in the middle of a beat waits for the next one.
    pub(crate) fn follow(&mut self, position: f64, sounding: bool) {
        if !sounding {
            self.last_beat = None;
            return;
        }
        let beat_length = self.beat_samples();
        let beat = (position / beat_length).floor() as i64;
        let new = match self.last_beat {
            Some(last) => beat != last,
            // Just started: click only when starting on the beat itself.
            None => position - beat as f64 * beat_length < 1.0,
        };
        self.last_beat = Some(beat);
        if new {
            let accent = beat.rem_euclid(i64::from(self.meter.beats.max(1))) == 0;
            self.hz = if accent { ACCENT_HZ } else { BEAT_HZ };
            self.left = (CLICK_SECONDS * self.rate) as usize;
            self.phase = 0.0;
            self.age = 0;
        }
    }

    /// The next sample of the click (silence between clicks).
    pub(crate) fn next(&mut self) -> f32 {
        if self.left == 0 {
            return 0.0;
        }
        self.left -= 1;
        let t = self.age as f32 / self.rate;
        self.age += 1;
        self.phase = (self.phase + TAU * self.hz / self.rate) % TAU;
        self.phase.sin() * (-t / DECAY_SECONDS).exp() * self.volume
    }

    #[cfg(test)]
    pub(crate) fn sounding(&self) -> bool {
        self.left > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_and_beats_count_from_the_start_of_the_tape() {
        let meter = Meter::default();
        assert_eq!(meter.beat_seconds(), 0.5);
        assert_eq!(meter.bar_and_beat(0.0), (1, 1));
        assert_eq!(meter.bar_and_beat(0.6), (1, 2));
        assert_eq!(meter.bar_and_beat(2.0), (2, 1));
        assert_eq!(meter.bar_and_beat(-0.1), (0, 4), "before the tape starts: the bar before");
        let six_eight = Meter { bpm: 120.0, beats: 6, unit: 8 };
        assert_eq!(six_eight.beat_seconds(), 0.25, "eighths at a quarter-note tempo");
        assert_eq!(six_eight.bar_and_beat(1.5), (2, 1));
    }

    #[test]
    fn clicks_on_every_beat_with_an_accent_on_the_one() {
        let mut click = Click::new(1_000);
        click.meter = Meter { bpm: 120.0, beats: 3, unit: 4 };
        let mut starts = Vec::new();
        for n in 0..2_000 {
            let before = click.sounding();
            click.follow(f64::from(n), true);
            if click.sounding() && (!before || click.age == 0) {
                starts.push((n, click.hz));
            }
            click.next();
        }
        assert_eq!(starts.iter().map(|(n, _)| *n).collect::<Vec<_>>(), vec![0, 500, 1_000, 1_500]);
        assert_eq!(starts[0].1, ACCENT_HZ);
        assert_eq!(starts[1].1, BEAT_HZ);
        assert_eq!(starts[3].1, ACCENT_HZ, "a new bar of three");
    }

    #[test]
    fn starting_mid_beat_waits_for_the_next_beat() {
        let mut click = Click::new(1_000);
        click.follow(250.0, true);
        assert!(!click.sounding());
        click.follow(499.0, true);
        assert!(!click.sounding());
        click.follow(500.0, true);
        assert!(click.sounding());
    }

    #[test]
    fn the_click_is_short_and_fades() {
        let mut click = Click::new(48_000);
        click.follow(0.0, true);
        let samples: Vec<f32> = (0..2_000).map(|_| click.next()).collect();
        let peak_start = samples[..100].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let peak_end = samples[1_500..1_680].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak_start > 0.2 && peak_end < 0.01);
        assert!(samples[1_700..].iter().all(|s| *s == 0.0), "over after 35 ms");
    }
}
