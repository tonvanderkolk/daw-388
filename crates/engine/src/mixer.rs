//! The 388's input channels: a source (MIC, LINE or the tape in the RMX position), TRIM, the
//! MIC preamp, EQ, the channel fader, PAN, the AUX and EFF sends and the taps the buses take
//! from.

use crate::eq::{EQ_DEFAULTS, Eq, EqBand};
use crate::monitor::MonitorMode;
use crate::smooth;
use daw388_fx::{EchoParams, PlateParams};
use std::f32::consts::FRAC_PI_4;

/// Where a channel's signal comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Source {
    /// The channel's input through the microphone preamp, which saturates softly.
    Mic,
    /// The channel's input, clean.
    Line,
    /// The playback of the track with the same number, the 388's remix position.
    #[default]
    Remix,
}

/// One channel's settings, sent whole with [`crate::Command::SetChannel`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelParams {
    pub source: Source,
    /// The input channel for MIC and LINE, counted from 0.
    pub input: Option<u16>,
    /// Linear TRIM gain.
    pub trim: f32,
    /// LOW, MID and HIGH.
    pub eq: [EqBand; 3],
    /// Linear fader gain.
    pub fader: f32,
    /// -1.0 is hard left (the odd buses), 1.0 hard right (the even buses).
    pub pan: f32,
    pub mute: bool,
    /// Linear AUX send. PRE takes it after TRIM and the preamp (before EQ, fader and mute), POST
    /// after fader and mute.
    pub aux: f32,
    pub aux_pre: bool,
    /// Linear EFF send, after fader and mute.
    pub eff: f32,
    /// ASSIGN 1-2, 3-4, 5-6 and 7-8.
    pub assign: [bool; 4],
    /// ASSIGN L-R: to the stereo bus.
    pub lr: bool,
    /// The track with the same number records this channel after TRIM and EQ instead of its bus.
    pub direct: bool,
    /// AUTO: play the tape, and the input while the track hears its input (a simple DAW's input
    /// monitoring). DIRECT then takes the input before the EQ, and TRIM and the preamp work on
    /// the input only.
    pub auto: bool,
}

impl Default for ChannelParams {
    fn default() -> Self {
        Self {
            source: Source::Remix,
            input: None,
            trim: 1.0,
            eq: EQ_DEFAULTS,
            fader: 1.0,
            pan: 0.0,
            mute: false,
            aux: 0.0,
            aux_pre: false,
            eff: 0.0,
            assign: [false; 4],
            lr: false,
            direct: false,
            auto: false,
        }
    }
}

impl ChannelParams {
    /// MIC, LINE or AUTO with an input the device has: a live source.
    pub(crate) fn is_live(&self, input_channels: usize) -> bool {
        (self.source != Source::Remix || self.auto) && self.input.is_some_and(|c| usize::from(c) < input_channels)
    }
}

/// What the main output carries: the stereo master, or AUX or EFF in mono on both sides.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Listen {
    #[default]
    Stereo,
    Aux,
    Eff,
}

/// An EFFECT RTN: linear gain and pan (a balance for the stereo plate).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RtnParams {
    pub gain: f32,
    pub pan: f32,
}

/// The master section's settings, sent whole with [`crate::Command::SetMixer`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixerParams {
    /// Linear gains of the PGM MASTERS: ODD (buses 1, 3, 5, 7) and EVEN (2, 4, 6, 8).
    pub odd: f32,
    pub even: f32,
    pub mon: MonitorMode,
    /// Left and right averaged on both sides after the STEREO MASTER.
    pub mono: bool,
    /// Linear gains of the AUX MASTER and EFF MASTER.
    pub aux_master: f32,
    pub eff_master: f32,
    pub listen: Listen,
    /// Linear MONITOR/PHONES level of the main output; the click comes after it.
    pub phones: f32,
    /// The interface channels (from 0) that carry AUX and EFF, if any.
    pub aux_out: Option<usize>,
    pub eff_out: Option<usize>,
    /// The plate on AUX and the echo on EFF, and their returns (closed by default).
    pub plate: PlateParams,
    pub echo: EchoParams,
    pub rtn: [RtnParams; 2],
}

impl Default for MixerParams {
    fn default() -> Self {
        Self {
            odd: 1.0,
            even: 1.0,
            mon: MonitorMode::Cue,
            mono: false,
            aux_master: 1.0,
            eff_master: 1.0,
            listen: Listen::Stereo,
            phones: 1.0,
            aux_out: None,
            eff_out: None,
            plate: PlateParams::default(),
            echo: EchoParams::default(),
            rtn: [RtnParams::default(); 2],
        }
    }
}

/// Where the MIC preamp starts to saturate (-6 dBFS) and the level it approaches.
const PREAMP_THRESHOLD: f32 = 0.5;
const PREAMP_CEILING: f32 = 0.94;
/// Shifts the curve so the positive side saturates first: a little even-harmonic colour.
const PREAMP_BIAS: f32 = 0.05;

/// The 388's microphone preamp after TRIM: clean up to the threshold, then a soft tanh knee
/// towards the ceiling. Bias in, bias out, so the clean region passes `x` exactly and the
/// output stays below full scale.
pub(crate) fn preamp(x: f32) -> f32 {
    fn curve(x: f32) -> f32 {
        let size = x.abs();
        if size <= PREAMP_THRESHOLD {
            return x;
        }
        let room = PREAMP_CEILING - PREAMP_THRESHOLD;
        (PREAMP_THRESHOLD + room * ((size - PREAMP_THRESHOLD) / room).tanh()).copysign(x)
    }
    curve(x + PREAMP_BIAS) - curve(PREAMP_BIAS)
}

/// Constant-power pan: -3 dB per side in the centre.
pub(crate) fn pan_gains(gain: f32, pan: f32) -> [f32; 2] {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
    [gain * angle.cos(), gain * angle.sin()]
}

/// What a channel hands on for one frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Taps {
    /// After the EQ, before the fader: the PRE meter and OL.
    pub(crate) pre: f32,
    /// What DIRECT records: after TRIM, the preamp and EQ; with AUTO the input before the EQ (and
    /// nothing while it plays the tape).
    pub(crate) direct: f32,
    /// After fader and mute, before PAN: the POST meter.
    pub(crate) post: f32,
    /// The AUX and EFF sends, mono.
    pub(crate) aux: f32,
    pub(crate) eff: f32,
    /// After PAN: left to the odd buses, right to the even ones; both to the stereo bus.
    pub(crate) left: f32,
    pub(crate) right: f32,
}

/// A channel and its smoothed gains.
#[derive(Debug)]
pub(crate) struct Channel {
    pub(crate) params: ChannelParams,
    trim: f32,
    current: [f32; 2],
    /// Smoothed AUX and EFF sends.
    sends: [f32; 2],
    eq: Eq,
}

impl Channel {
    /// TRIM starts at its target so a recording through DIRECT is exact from the first sample;
    /// the fader fades in, as before.
    pub(crate) fn new(sample_rate: u32) -> Self {
        Self { params: ChannelParams::default(), trim: 1.0, current: [0.0; 2], sends: [0.0; 2], eq: Eq::new(sample_rate) }
    }

    /// Run one sample of the channel's source through it. `from_input`: the sample comes from
    /// the interface, not the tape.
    pub(crate) fn process(&mut self, sample: f32, from_input: bool) -> Taps {
        smooth(&mut self.trim, self.params.trim);
        let target = if self.params.mute { [0.0; 2] } else { pan_gains(self.params.fader, self.params.pan) };
        smooth(&mut self.current[0], target[0]);
        smooth(&mut self.current[1], target[1]);
        smooth(&mut self.sends[0], self.params.aux);
        smooth(&mut self.sends[1], self.params.eff);
        // With AUTO the tape skips TRIM and the preamp: they belong to the input.
        let gained = if from_input || !self.params.auto { sample * self.trim } else { sample };
        let shaped = if from_input && self.params.source == Source::Mic { preamp(gained) } else { gained };
        self.eq.target = self.params.eq;
        let pre = self.eq.process(shaped);
        let direct = match (self.params.auto, from_input) {
            (true, true) => shaped,
            (true, false) => 0.0,
            (false, _) => pre,
        };
        // Constant-power pan keeps left² + right² equal to the fader gain squared.
        let post = pre * self.current[0].hypot(self.current[1]);
        let aux = self.sends[0] * if self.params.aux_pre { shaped } else { post };
        let eff = self.sends[1] * post;
        Taps { pre, direct, post, aux, eff, left: pre * self.current[0], right: pre * self.current[1] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(params: ChannelParams) -> Channel {
        let mut channel = Channel { params, ..Channel::new(48_000) };
        for _ in 0..20_000 {
            channel.process(0.0, true);
        }
        channel
    }

    #[test]
    fn the_preamp_is_linear_below_its_threshold() {
        for x in [-0.5, -0.25, 0.0, 0.1, 0.4] {
            assert!((preamp(x) - x).abs() < 1e-6, "{x}");
        }
    }

    #[test]
    fn the_preamp_saturates_and_stays_below_full_scale() {
        assert!(preamp(0.8) < 0.8, "above the threshold it compresses");
        for x in [2.0, 10.0, 100.0, -2.0, -10.0, -100.0] {
            assert!(preamp(x).abs() < 1.0, "{x}");
        }
        assert!((preamp(0.8) + preamp(-0.8)).abs() > 1e-3, "asymmetric: even harmonics");
    }

    #[test]
    fn trim_comes_before_the_fader_and_pan_splits_left_and_right() {
        let mut channel = settled(ChannelParams { trim: 2.0, fader: 0.5, pan: -1.0, ..ChannelParams::default() });
        let taps = channel.process(0.25, false);
        assert!((taps.direct - 0.5).abs() < 1e-4, "DIRECT: after TRIM, not the fader");
        assert!((taps.post - 0.25).abs() < 1e-4, "POST: after the fader");
        assert!((taps.left - 0.25).abs() < 1e-4 && taps.right.abs() < 1e-4, "hard left: all to the odd side");
    }

    #[test]
    fn direct_comes_after_the_eq() {
        let mut eq = EQ_DEFAULTS;
        eq[1].gain_db = 15.0;
        let mut channel = settled(ChannelParams { source: Source::Line, eq, ..ChannelParams::default() });
        let mut peak = 0.0f32;
        for i in 0..48_000 {
            let x = 0.05 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48_000.0).sin();
            let taps = channel.process(x, true);
            if i > 43_200 {
                peak = peak.max(taps.direct.abs());
            }
        }
        assert!((20.0 * (peak / 0.05).log10() - 15.0).abs() < 0.5, "{peak}");
    }

    #[test]
    fn mic_saturates_and_line_stays_clean() {
        let loud = ChannelParams { trim: 100.0, ..ChannelParams::default() };
        let mut mic = settled(ChannelParams { source: Source::Mic, ..loud });
        let mut line = settled(ChannelParams { source: Source::Line, ..loud });
        assert!(mic.process(0.1, true).direct < 1.0, "MIC: +40 dB into the preamp stays below full scale");
        assert!((line.process(0.1, true).direct - 10.0).abs() < 1e-3, "LINE: just gain");
    }

    #[test]
    fn auto_records_before_the_eq_and_meters_after_it() {
        let mut eq = EQ_DEFAULTS;
        eq[1].gain_db = 15.0;
        let auto = ChannelParams { source: Source::Line, auto: true, trim: 2.0, eq, ..ChannelParams::default() };
        let mut channel = settled(auto);
        let mut differs = false;
        for i in 0..4_800 {
            let x = 0.05 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48_000.0).sin();
            let taps = channel.process(x, true);
            assert!((taps.direct - 2.0 * x).abs() < 1e-6, "DIRECT: TRIM only, no EQ");
            differs |= (taps.pre - taps.direct).abs() > 1e-3;
        }
        assert!(differs, "PRE (meter and OL) is after the EQ");
    }

    #[test]
    fn auto_plays_the_tape_without_trim_or_preamp_and_records_nothing_of_it() {
        let mut channel = settled(ChannelParams { source: Source::Mic, auto: true, trim: 100.0, ..ChannelParams::default() });
        let taps = channel.process(0.25, false);
        assert!((taps.pre - 0.25).abs() < 1e-6, "the tape skips TRIM and the preamp");
        assert_eq!(taps.direct, 0.0, "AUTO only records its input");
    }

    #[test]
    fn auto_counts_as_live_with_an_input() {
        let auto = ChannelParams { source: Source::Remix, auto: true, input: Some(0), ..ChannelParams::default() };
        assert!(auto.is_live(1));
        assert!(!ChannelParams { input: None, ..auto }.is_live(1));
    }

    /// The taps after `n` samples of a constant `x` from the input.
    fn steady(channel: &mut Channel, x: f32, n: usize) -> Taps {
        let mut taps = Taps::default();
        for _ in 0..n {
            taps = channel.process(x, true);
        }
        taps
    }

    #[test]
    fn aux_pre_ignores_eq_fader_and_mute_and_post_follows_the_fader_not_the_pan() {
        let mut eq = EQ_DEFAULTS;
        eq[1].gain_db = 15.0;
        let base = ChannelParams { source: Source::Line, aux: 0.5, eff: 0.25, fader: 0.5, pan: -1.0, eq, ..ChannelParams::default() };
        let mut pre = settled(ChannelParams { aux_pre: true, mute: true, ..base });
        let taps = steady(&mut pre, 1.0, 1);
        assert!((taps.aux - 0.5).abs() < 1e-4, "PRE: TRIM only, then the send, at once: {}", taps.aux);
        assert_eq!(taps.eff, 0.0, "EFF is after mute");

        // DC passes a peaking EQ unchanged, so after a while the constant shows the fader.
        let mut post = settled(ChannelParams { aux_pre: false, ..base });
        let taps = steady(&mut post, 1.0, 5_000);
        assert!((taps.aux - 0.25).abs() < 1e-3, "POST: fader 0.5 times send 0.5, PAN does not count: {}", taps.aux);
        assert!((taps.eff - 0.125).abs() < 1e-3, "EFF: fader 0.5 times send 0.25: {}", taps.eff);
    }

    #[test]
    fn mute_silences_everything_after_the_fader_only() {
        let mut channel = settled(ChannelParams { mute: true, ..ChannelParams::default() });
        let taps = channel.process(1.0, false);
        assert!((taps.direct - 1.0).abs() < 1e-6);
        assert!(taps.post.abs() < 1e-4 && taps.left.abs() < 1e-4 && taps.right.abs() < 1e-4);
    }

    #[test]
    fn only_mic_or_line_with_an_existing_input_is_live() {
        let line = ChannelParams { source: Source::Line, input: Some(1), ..ChannelParams::default() };
        assert!(line.is_live(2));
        assert!(!line.is_live(1), "the device has no second input");
        assert!(!ChannelParams { input: None, ..line }.is_live(2));
        assert!(!ChannelParams { source: Source::Remix, ..line }.is_live(2));
    }
}
