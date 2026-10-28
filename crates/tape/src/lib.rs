//! The 388's tape on playback: tape speed, tape kind, saturation, wow and flutter, hiss and
//! crosstalk, for all eight tracks at once. Recordings from an input stay clean; only what plays
//! back from tape gets the character, dbx included: it encodes and decodes around the tape, so
//! the hiss breathes with the music.
//!
//! Nothing here allocates after [`TapeMachine::new`].

mod filters;

use filters::{Biquad, Emphasis};
use std::f32::consts::TAU;

pub const TRACKS: usize = 8;

/// Tape speed in inches per second.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Speed {
    Ips3_75,
    #[default]
    Ips7_5,
    Ips15,
}

/// Tape stock, after the character of 456, 250 and 911 (an approximation, not a copy).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TapeKind {
    #[default]
    T456,
    T250,
    T911,
}

/// The tape's settings. The four amounts run from 0 to 2; 1 is the machine as it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TapeParams {
    /// 388 (true) or Clean (false: playback passes untouched).
    pub on: bool,
    pub speed: Speed,
    pub kind: TapeKind,
    pub saturation: f32,
    pub wow_flutter: f32,
    pub noise: f32,
    pub crosstalk: f32,
    /// dbx on tracks 1-4 and 5-8.
    pub dbx: [bool; 2],
}

impl Default for TapeParams {
    fn default() -> Self {
        Self {
            on: false,
            speed: Speed::Ips7_5,
            kind: TapeKind::T456,
            saturation: 1.0,
            wow_flutter: 1.0,
            noise: 1.0,
            crosstalk: 1.0,
            dbx: [true; 2],
        }
    }
}

/// Bias of the saturation curve: the positive side flattens a little sooner (even harmonics).
const BIAS: f32 = 0.1;
/// Crosstalk at 100%: the 388's 75 dB.
const CROSSTALK: f32 = 1.778_279_4e-4;
/// Hiss at 100% without dbx, RMS (-58 dBFS).
const HISS: f32 = 1.258_925_4e-3;
/// dbx: compression and expansion turn around this level (-20 dBFS) ...
const DBX_REFERENCE: f32 = 0.1;
/// ... and the detector stops at -100 dBFS, so the gain never passes 40 dB.
const DBX_FLOOR: f32 = 1.0e-5;
/// The RMS detectors' time constant in seconds.
const DBX_TIME: f32 = 0.010;
/// Wow (0.5 Hz) and flutter (6 Hz) as peak speed deviations at 7½ IPS: about 0.05% WRMS together.
const WOW: (f32, f32) = (0.5, 0.0006);
const FLUTTER: (f32, f32) = (6.0, 0.0004);

fn speed_factor(speed: Speed) -> (f32, f64, f64, f32) {
    // (wow and flutter, head bump Hz, high roll-off Hz, hiss dB)
    match speed {
        Speed::Ips3_75 => (2.0, 40.0, 10_000.0, 3.0),
        Speed::Ips7_5 => (1.0, 60.0, 15_000.0, 0.0),
        Speed::Ips15 => (0.6, 100.0, 20_000.0, -3.0),
    }
}

fn kind_factor(kind: TapeKind) -> (f32, f64, f32) {
    // (saturation drive, tone dB at 8 kHz, hiss dB)
    match kind {
        TapeKind::T456 => (1.0, 0.0, 0.0),
        TapeKind::T250 => (1.6, -1.0, 2.0),
        TapeKind::T911 => (1.2, 1.0, 0.0),
    }
}

/// The soft, slightly asymmetric curve, with a slope of 1 at 0.
fn saturate(u: f32) -> f32 {
    let slope = 1.0 - BIAS.tanh().powi(2);
    ((u + BIAS).tanh() - BIAS.tanh()) / slope
}

/// dbx for one track: 2:1 on the way onto the tape, 1:2 on the way off, each with an RMS
/// detector and an emphasis half. The hiss in between is expanded with the music: it breathes.
#[derive(Debug, Clone, Copy, Default)]
struct Dbx {
    pre: Emphasis,
    de: Emphasis,
    /// Mean squares before encoding and after the tape.
    encode: f32,
    decode: f32,
}

impl Dbx {
    fn encode(&mut self, x: f32, rate: f32) -> f32 {
        let e = self.pre.pre(x);
        self.encode += rate * (e * e - self.encode);
        let level = self.encode.sqrt().max(DBX_FLOOR);
        e * (level / DBX_REFERENCE).powf(-0.5)
    }

    fn decode(&mut self, y: f32, rate: f32) -> f32 {
        self.decode += rate * (y * y - self.decode);
        // The encoder's floor, as it looks after encoding.
        let floor = (DBX_REFERENCE * DBX_FLOOR).sqrt();
        let level = self.decode.sqrt().max(floor);
        self.de.de(y * level / DBX_REFERENCE)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Track {
    emphasis: Emphasis,
    dbx: Dbx,
    bump: Biquad,
    tone: Biquad,
    rolloff: Biquad,
    hiss: f32,
    seed: u32,
}

impl Track {
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

/// The tape for eight tracks.
#[derive(Debug, Clone)]
pub struct TapeMachine {
    params: TapeParams,
    rate: f32,
    tracks: [Track; TRACKS],
    /// Wow and flutter phases, 0 to 1.
    phases: [f32; 2],
    /// The hiss filter's coefficient and the scale that brings white noise to `HISS`.
    hiss_filter: f32,
    hiss_scale: f32,
    /// The dbx detectors' one-pole coefficient.
    dbx_rate: f32,
}

impl TapeMachine {
    pub fn new(sample_rate: u32) -> Self {
        let rate = sample_rate as f32;
        let hiss_filter = 1.0 - (-TAU * 8_000.0 / rate).exp();
        // White noise from -1 to 1 has an RMS of 1/√3; a one-pole lowpass keeps a/(2-a) of its power.
        let white_rms = (1.0f32 / 3.0).sqrt() * (hiss_filter / (2.0 - hiss_filter)).sqrt();
        let mut tracks = [Track::default(); TRACKS];
        for (i, track) in tracks.iter_mut().enumerate() {
            track.seed = 0x9E37_79B9 ^ (i as u32 + 1).wrapping_mul(0x85EB_CA6B);
        }
        let dbx_rate = 1.0 - (-1.0 / (DBX_TIME * rate)).exp();
        let mut machine =
            Self { params: TapeParams::default(), rate, tracks, phases: [0.0; 2], hiss_filter, hiss_scale: HISS / white_rms, dbx_rate };
        machine.tune();
        machine
    }

    pub fn params(&self) -> TapeParams {
        self.params
    }

    /// New settings; the filters are retuned only when the speed or kind changes.
    pub fn set(&mut self, params: TapeParams) {
        let retune = params.speed != self.params.speed || params.kind != self.params.kind;
        self.params = params;
        if retune {
            self.tune();
        }
    }

    fn tune(&mut self) {
        let rate = f64::from(self.rate);
        let (_, bump, rolloff, _) = speed_factor(self.params.speed);
        let (_, tone, _) = kind_factor(self.params.kind);
        for track in &mut self.tracks {
            track.bump = track.bump.retuned(Biquad::peak(bump, 1.5, 1.0, rate));
            track.tone = track.tone.retuned(Biquad::peak(8_000.0, tone, 0.7, rate));
            track.rolloff = track.rolloff.retuned(Biquad::lowpass(rolloff, 0.707, rate));
        }
    }

    /// One frame of wow and flutter: how many samples the playback position is off. 0 on Clean.
    pub fn wow(&mut self) -> f64 {
        if !self.params.on {
            return 0.0;
        }
        let (speed, ..) = speed_factor(self.params.speed);
        let depth = speed * self.params.wow_flutter.clamp(0.0, 2.0);
        let mut offset = 0.0;
        for (phase, (freq, deviation)) in self.phases.iter_mut().zip([WOW, FLUTTER]) {
            *phase = (*phase + freq / self.rate).fract();
            // The position is the integral of the speed's deviation.
            offset += deviation / (TAU * freq) * (TAU * *phase).sin();
        }
        f64::from(offset * depth * self.rate)
    }

    /// The tape's character on one frame of all eight tracks. `has_audio`: the track has a
    /// recording (hiss only then); `playing`: the tape is moving.
    pub fn process(&mut self, tracks: &mut [f32; TRACKS], has_audio: [bool; TRACKS], playing: bool) {
        if !self.params.on {
            return;
        }
        let p = self.params;
        let (_, _, _, speed_hiss) = speed_factor(p.speed);
        let (drive, _, kind_hiss) = kind_factor(p.kind);
        let drive = drive * p.saturation.clamp(0.0, 2.0);
        let leak = CROSSTALK * p.crosstalk.clamp(0.0, 2.0);
        let hiss = self.hiss_scale * 10f32.powf((speed_hiss + kind_hiss) / 20.0) * p.noise.clamp(0.0, 2.0);
        let dry = *tracks;
        for (i, (out, track)) in tracks.iter_mut().zip(&mut self.tracks).enumerate() {
            let neighbours = if i > 0 { dry[i - 1] } else { 0.0 } + if i + 1 < TRACKS { dry[i + 1] } else { 0.0 };
            let mut x = dry[i] + leak * neighbours;
            let dbx = p.dbx[i / 4];
            if dbx {
                x = track.dbx.encode(x, self.dbx_rate);
            }
            if drive > 0.0 {
                let pre = track.emphasis.pre(x);
                x = track.emphasis.de(saturate(pre * drive) / drive);
            }
            x = track.rolloff.process(track.tone.process(track.bump.process(x)));
            if playing && has_audio[i] && hiss > 0.0 {
                let white = track.white();
                track.hiss += self.hiss_filter * (white - track.hiss);
                x += track.hiss * hiss;
            }
            if dbx {
                x = track.dbx.decode(x, self.dbx_rate);
            }
            *out = x;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    const RATE: u32 = 48_000;

    fn machine(params: TapeParams) -> TapeMachine {
        let mut machine = TapeMachine::new(RATE);
        machine.set(params);
        machine
    }

    fn on() -> TapeParams {
        TapeParams { on: true, ..TapeParams::default() }
    }

    /// Peak in dB on `track` over the last tenth of a second of a sine on that track.
    fn sine_db(machine: &mut TapeMachine, freq: f32, level: f32) -> f32 {
        let mut peak = 0.0f32;
        for i in 0..RATE as usize {
            let mut frame = [0.0; TRACKS];
            frame[0] = level * (2.0 * PI * freq * i as f32 / RATE as f32).sin();
            machine.process(&mut frame, [false; TRACKS], true);
            if i > 43_200 {
                peak = peak.max(frame[0].abs());
            }
        }
        20.0 * (peak / level).log10()
    }

    #[test]
    fn clean_passes_the_tape_bit_for_bit() {
        let mut machine = machine(TapeParams { wow_flutter: 2.0, noise: 2.0, ..TapeParams::default() });
        for i in 0..1_000 {
            assert_eq!(machine.wow(), 0.0);
            let mut frame = [i as f32 * 1e-3; TRACKS];
            let before = frame;
            machine.process(&mut frame, [true; TRACKS], true);
            assert_eq!(frame, before);
        }
    }

    #[test]
    fn faster_tape_keeps_more_highs() {
        let quiet = TapeParams { saturation: 0.0, noise: 0.0, ..on() };
        let slow = sine_db(&mut machine(TapeParams { speed: Speed::Ips3_75, ..quiet }), 12_000.0, 0.1);
        let fast = sine_db(&mut machine(TapeParams { speed: Speed::Ips15, ..quiet }), 12_000.0, 0.1);
        assert!(fast > slow + 3.0, "{slow} {fast}");
    }

    #[test]
    fn saturation_leaves_soft_signals_and_flattens_loud_ones() {
        // Without dbx: with it, a loud signal is compressed before the tape and saturates less.
        let quiet = TapeParams { noise: 0.0, dbx: [false; 2], ..on() };
        let soft =
            sine_db(&mut machine(quiet), 1_000.0, 0.03) - sine_db(&mut machine(TapeParams { saturation: 0.0, ..quiet }), 1_000.0, 0.03);
        assert!(soft.abs() < 0.1, "-30 dBFS stays as it is: {soft}");
        let loud_456 = sine_db(&mut machine(quiet), 100.0, 0.9);
        let loud_250 = sine_db(&mut machine(TapeParams { kind: TapeKind::T250, ..quiet }), 100.0, 0.9);
        assert!(loud_456 < -0.3, "a hard signal is flattened: {loud_456}");
        assert!(loud_250 < loud_456, "250 saturates sooner: {loud_250} {loud_456}");
        let mut hot = machine(TapeParams { saturation: 2.0, kind: TapeKind::T250, ..quiet });
        for i in 0..RATE as usize {
            let mut frame = [10.0 * (i as f32 * 0.05).sin(); TRACKS];
            hot.process(&mut frame, [false; TRACKS], true);
            assert!(frame.iter().all(|s| s.abs() < 1.5 && s.is_finite()), "{i}: {frame:?}");
        }
    }

    #[test]
    fn wow_and_flutter_stay_small_and_vanish_at_zero() {
        let mut most = machine(TapeParams { wow_flutter: 2.0, ..on() });
        let biggest = (0..4 * RATE).map(|_| most.wow().abs()).fold(0.0, f64::max);
        assert!(biggest > 5.0 && biggest < 25.0, "{biggest}");
        let mut none = machine(TapeParams { wow_flutter: 0.0, ..on() });
        assert!((0..RATE).all(|_| none.wow() == 0.0));
    }

    /// RMS of what `track` gives over a second, with `level` as a 1 kHz sine on every track.
    fn rms(machine: &mut TapeMachine, track: usize, level: f32, has_audio: bool, playing: bool) -> f32 {
        let mut sum = 0.0f32;
        for i in 0..RATE as usize {
            let mut frame = [level * (2.0 * PI * 1_000.0 * i as f32 / RATE as f32).sin(); TRACKS];
            machine.process(&mut frame, [has_audio; TRACKS], playing);
            if i >= RATE as usize / 2 {
                sum += frame[track] * frame[track];
            }
        }
        (sum / (RATE / 2) as f32).sqrt()
    }

    #[test]
    fn hiss_only_on_recorded_tracks_while_the_tape_plays() {
        let mut tape = machine(TapeParams { saturation: 0.0, dbx: [false; 2], ..on() });
        let hiss = rms(&mut tape, 0, 0.0, true, true);
        assert!(hiss > 0.5 * HISS && hiss < 2.0 * HISS, "about -58 dBFS without dbx: {hiss}");
        assert_eq!(rms(&mut tape, 0, 0.0, false, true), 0.0, "an empty track has no tape to hiss");
        assert_eq!(rms(&mut tape, 0, 0.0, true, false), 0.0, "a stopped tape is silent");
        assert_eq!(rms(&mut machine(TapeParams { noise: 0.0, ..on() }), 0, 0.0, true, true), 0.0);
    }

    #[test]
    fn dbx_passes_the_signal_and_buries_the_hiss_in_silence() {
        let quiet = TapeParams { saturation: 0.0, noise: 0.0, ..on() };
        let with = rms(&mut machine(quiet), 0, 0.1, true, true);
        let without = rms(&mut machine(TapeParams { dbx: [false; 2], ..quiet }), 0, 0.1, true, true);
        assert!((20.0 * (with / without).log10()).abs() < 0.5, "{with} {without}");

        let hiss = TapeParams { saturation: 0.0, ..on() };
        let plain = rms(&mut machine(TapeParams { dbx: [false; 2], ..hiss }), 0, 0.0, true, true);
        let reduced = rms(&mut machine(hiss), 0, 0.0, true, true);
        assert!(20.0 * (plain / reduced).log10() > 20.0, "at least 20 dB less hiss: {plain} {reduced}");
    }

    #[test]
    fn with_dbx_the_hiss_breathes_with_the_music() {
        let hiss = TapeParams { saturation: 0.0, ..on() };
        // The hiss alone: the same tape with and without noise, sample by sample.
        let under = |level: f32| {
            let (mut noisy, mut clean) = (machine(hiss), machine(TapeParams { noise: 0.0, ..hiss }));
            let mut sum = 0.0f32;
            for i in 0..RATE as usize {
                let x = level * (2.0 * PI * 1_000.0 * i as f32 / RATE as f32).sin();
                let (mut a, mut b) = ([x; TRACKS], [x; TRACKS]);
                noisy.process(&mut a, [true; TRACKS], true);
                clean.process(&mut b, [true; TRACKS], true);
                if i >= RATE as usize / 2 {
                    sum += (a[0] - b[0]).powi(2);
                }
            }
            (sum / (RATE / 2) as f32).sqrt()
        };
        let (silent, playing) = (under(0.0), under(0.1));
        assert!(playing > 4.0 * silent, "the hiss comes up with the music: {silent} {playing}");
    }

    #[test]
    fn each_group_has_its_own_dbx() {
        let mut tape = machine(TapeParams { saturation: 0.0, dbx: [false, true], ..on() });
        let (first, fifth) = (rms(&mut tape, 0, 0.0, true, true), rms(&mut tape, 4, 0.0, true, true));
        assert!(first > 10.0 * fifth, "tracks 1-4 without dbx hiss more: {first} {fifth}");
    }

    #[test]
    fn crosstalk_leaks_into_the_neighbours_only() {
        let mut tape = machine(TapeParams { saturation: 0.0, noise: 0.0, ..on() });
        let mut peaks = [0.0f32; TRACKS];
        for i in 0..RATE as usize {
            let mut frame = [0.0; TRACKS];
            frame[1] = 0.5 * (2.0 * PI * 1_000.0 * i as f32 / RATE as f32).sin();
            tape.process(&mut frame, [false; TRACKS], true);
            if i > 43_200 {
                for (peak, s) in peaks.iter_mut().zip(frame) {
                    *peak = peak.max(s.abs());
                }
            }
        }
        let db = |peak: f32| 20.0 * (peak / 0.5).log10();
        assert!((db(peaks[0]) + 75.0).abs() < 2.0 && (db(peaks[2]) + 75.0).abs() < 2.0, "{peaks:?}");
        assert_eq!(peaks[3], 0.0);
    }
}
