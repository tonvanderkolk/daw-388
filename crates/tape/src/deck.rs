//! The mixdown deck: a second, two-track machine after the STEREO MASTER, as the 388's mix went
//! onto a 1/4" deck or a cassette. One tape for both sides (one wow and flutter), hiss and
//! saturation per side. The profiles are fixed: no knobs.
//!
//! Nothing here allocates after [`MixdownDeck::new`].

use crate::filters::{Biquad, Emphasis};
use crate::{FLUTTER, HISS, WOW, saturate};
use std::f32::consts::TAU;

/// The machine the mix goes onto.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Machine {
    /// Straight to digital: the mix passes untouched.
    #[default]
    Direct,
    /// A 1/4" two-track at 15 IPS: warm and fairly transparent.
    Quarter15,
    /// A cassette: more hiss, fewer highs, more wow and flutter.
    Cassette,
}

impl Machine {
    pub const ALL: [Machine; 3] = [Machine::Quarter15, Machine::Cassette, Machine::Direct];

    /// The name on the deck's display.
    pub fn label(self) -> &'static str {
        match self {
            Machine::Direct => "DIRECT",
            Machine::Quarter15 => "1/4\" 15 IPS",
            Machine::Cassette => "CASSETTE",
        }
    }
}

/// The fixed character of a machine.
struct Profile {
    /// Saturation drive (0: none).
    drive: f32,
    /// Head bump: frequency and gain in dB.
    bump: (f64, f64),
    /// High roll-off frequency.
    rolloff: f64,
    /// A peak cut around 8 kHz in dB (0: none).
    shelf: f64,
    /// Wow and flutter as a factor on the 388's at 7½ IPS.
    wobble: f32,
    /// Hiss relative to the 388's, in dB.
    hiss_db: f32,
    /// How much of the hiss above about 4 kHz is left (noise reduction; 1: all of it).
    hiss_highs: f32,
}

fn profile(machine: Machine) -> Profile {
    match machine {
        Machine::Direct => {
            Profile { drive: 0.0, bump: (100.0, 0.0), rolloff: 22_000.0, shelf: 0.0, wobble: 0.0, hiss_db: -200.0, hiss_highs: 1.0 }
        }
        Machine::Quarter15 => {
            Profile { drive: 0.8, bump: (100.0, 1.5), rolloff: 20_000.0, shelf: 0.0, wobble: 0.5, hiss_db: -22.0, hiss_highs: 1.0 }
        }
        Machine::Cassette => {
            Profile { drive: 1.5, bump: (60.0, 1.0), rolloff: 12_000.0, shelf: -2.0, wobble: 2.5, hiss_db: -14.0, hiss_highs: 0.316 }
        }
    }
}

/// The delay line for wow and flutter: long enough for the cassette's deepest wow.
const WOBBLE_LEN: usize = 512;
/// Where the read point sits on average, in samples, so it can move both ways.
const WOBBLE_CENTRE: f32 = 128.0;

#[derive(Debug, Clone, Copy, Default)]
struct Side {
    emphasis: Emphasis,
    bump: Biquad,
    shelf: Biquad,
    rolloff: Biquad,
    /// Two one-pole states for the hiss: its colour and the split at 4 kHz.
    hiss: f32,
    hiss_low: f32,
    seed: u32,
}

impl Side {
    /// White noise from -1 to 1 (xorshift32).
    fn white(&mut self) -> f32 {
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// A two-track machine for the mix.
#[derive(Debug, Clone)]
pub struct MixdownDeck {
    machine: Machine,
    rate: f32,
    sides: [Side; 2],
    /// The delay line per side and where the next sample goes.
    delay: [[f32; WOBBLE_LEN]; 2],
    write: usize,
    /// Wow and flutter phases, 0 to 1.
    phases: [f32; 2],
    hiss_filter: f32,
    split_filter: f32,
    hiss_scale: f32,
    noise: bool,
}

impl MixdownDeck {
    pub fn new(sample_rate: u32) -> Self {
        let rate = sample_rate as f32;
        let hiss_filter = 1.0 - (-TAU * 8_000.0 / rate).exp();
        let split_filter = 1.0 - (-TAU * 4_000.0 / rate).exp();
        let white_rms = (1.0f32 / 3.0).sqrt() * (hiss_filter / (2.0 - hiss_filter)).sqrt();
        let mut sides = [Side::default(); 2];
        for (i, side) in sides.iter_mut().enumerate() {
            side.seed = 0x2545_F491 ^ (i as u32 + 7).wrapping_mul(0x9E37_79B9);
        }
        let mut deck = Self {
            machine: Machine::Direct,
            rate,
            sides,
            delay: [[0.0; WOBBLE_LEN]; 2],
            write: 0,
            phases: [0.0; 2],
            hiss_filter,
            split_filter,
            hiss_scale: HISS / white_rms,
            noise: true,
        };
        deck.tune();
        deck
    }

    pub fn machine(&self) -> Machine {
        self.machine
    }

    /// Switch machines. The filters keep their memory and the delay line keeps running, so a
    /// switch does not click.
    pub fn set(&mut self, machine: Machine) {
        if machine != self.machine {
            self.machine = machine;
            self.tune();
        }
    }

    /// Without hiss, for measuring the rest.
    #[cfg(test)]
    fn without_noise(mut self) -> Self {
        self.noise = false;
        self
    }

    fn tune(&mut self) {
        let rate = f64::from(self.rate);
        let p = profile(self.machine);
        for side in &mut self.sides {
            side.bump = side.bump.retuned(Biquad::peak(p.bump.0, p.bump.1, 1.0, rate));
            side.shelf = side.shelf.retuned(Biquad::peak(8_000.0, p.shelf, 0.7, rate));
            side.rolloff = side.rolloff.retuned(Biquad::lowpass(p.rolloff, 0.707, rate));
        }
    }

    /// How far the read point is off its centre this frame, in samples.
    fn wobble(&mut self, depth: f32) -> f32 {
        let mut offset = 0.0;
        for (phase, (freq, deviation)) in self.phases.iter_mut().zip([WOW, FLUTTER]) {
            *phase = (*phase + freq / self.rate).fract();
            offset += deviation / (TAU * freq) * (TAU * *phase).sin();
        }
        offset * depth * self.rate
    }

    /// One frame through the deck.
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        if self.machine == Machine::Direct {
            return (left, right);
        }
        let p = profile(self.machine);
        let hiss = if self.noise { self.hiss_scale * 10f32.powf(p.hiss_db / 20.0) } else { 0.0 };
        let read = WOBBLE_CENTRE + self.wobble(p.wobble);
        let mut out = [0.0; 2];
        for (index, x) in [left, right].into_iter().enumerate() {
            let side = &mut self.sides[index];
            let pre = side.emphasis.pre(x);
            let mut y = side.emphasis.de(saturate(pre * p.drive) / p.drive);
            y = side.rolloff.process(side.shelf.process(side.bump.process(y)));
            if hiss > 0.0 {
                let white = side.white();
                side.hiss += self.hiss_filter * (white - side.hiss);
                side.hiss_low += self.split_filter * (side.hiss - side.hiss_low);
                let shaped = side.hiss_low + p.hiss_highs * (side.hiss - side.hiss_low);
                y += shaped * hiss;
            }
            // Into the delay line, and read back where the wobbling tape is.
            let line = &mut self.delay[index];
            line[self.write] = y;
            let back = self.write as f32 + WOBBLE_LEN as f32 - read;
            let at = back.floor();
            let frac = back - at;
            let i0 = at as usize % WOBBLE_LEN;
            let i1 = (i0 + 1) % WOBBLE_LEN;
            out[index] = line[i0] + frac * (line[i1] - line[i0]);
        }
        self.write = (self.write + 1) % WOBBLE_LEN;
        (out[0], out[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    const RATE: u32 = 48_000;

    fn deck(machine: Machine) -> MixdownDeck {
        let mut deck = MixdownDeck::new(RATE);
        deck.set(machine);
        deck
    }

    /// The RMS of the left side over the last half second of a second of sine.
    fn sine_rms(deck: &mut MixdownDeck, freq: f32, level: f32) -> f32 {
        let (mut sum, mut n) = (0.0f64, 0);
        for i in 0..RATE as usize {
            let x = level * (2.0 * PI * freq * i as f32 / RATE as f32).sin();
            let (l, _) = deck.process(x, x);
            if i >= RATE as usize / 2 {
                sum += f64::from(l * l);
                n += 1;
            }
        }
        (sum / f64::from(n)).sqrt() as f32
    }

    #[test]
    fn direct_is_bit_exact() {
        let mut deck = deck(Machine::Direct);
        for i in 0..1_000 {
            let (l, r) = ((i as f32 * 0.37).sin() * 1.3, (i as f32 * 0.11).cos());
            let (a, b) = deck.process(l, r);
            assert_eq!((a.to_bits(), b.to_bits()), (l.to_bits(), r.to_bits()));
        }
    }

    #[test]
    fn cassette_is_darker_than_the_quarter_inch() {
        let level = |machine| sine_rms(&mut deck(machine).without_noise(), 10_000.0, 0.1);
        let (cassette, quarter, direct) = (level(Machine::Cassette), level(Machine::Quarter15), level(Machine::Direct));
        assert!(cassette < quarter && quarter < direct, "{cassette} {quarter} {direct}");
    }

    #[test]
    fn cassette_hisses_more() {
        let hiss = |machine| sine_rms(&mut deck(machine), 1_000.0, 0.0);
        let (cassette, quarter) = (hiss(Machine::Cassette), hiss(Machine::Quarter15));
        assert!(cassette > quarter && quarter > 0.0, "{cassette} {quarter}");
    }

    #[test]
    fn both_sides_wobble_together() {
        let mut deck = deck(Machine::Cassette).without_noise();
        for i in 0..RATE as usize {
            let x = 0.3 * (2.0 * PI * 1_000.0 * i as f32 / RATE as f32).sin();
            let (l, r) = deck.process(x, x);
            assert_eq!(l.to_bits(), r.to_bits(), "frame {i}");
        }
    }

    #[test]
    fn a_loud_mix_saturates_softly() {
        let mut deck = deck(Machine::Cassette).without_noise();
        let mut peak = 0.0f32;
        for i in 0..RATE as usize {
            let x = 2.0 * (2.0 * PI * 200.0 * i as f32 / RATE as f32).sin();
            let (l, _) = deck.process(x, x);
            peak = peak.max(l.abs());
        }
        assert!(peak < 1.2, "{peak}");
        let quiet = sine_rms(&mut self::deck(Machine::Cassette).without_noise(), 200.0, 0.01) / 0.01;
        let loud = sine_rms(&mut self::deck(Machine::Cassette).without_noise(), 200.0, 2.0) / 2.0;
        assert!(loud < 0.8 * quiet, "loud {loud} quiet {quiet}");
    }
}
