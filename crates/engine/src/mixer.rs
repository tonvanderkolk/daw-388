//! The 388's input channels: a source (MIC, LINE or the tape in the RMX position), TRIM, the
//! channel fader, PAN and the taps the buses take from. EQ and the sends follow in 0.9 and 0.10.

use crate::monitor::MonitorMode;
use crate::smooth;
use std::f32::consts::FRAC_PI_4;

/// Where a channel's signal comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Source {
    /// The channel's input through the microphone preamp (its character follows in 0.9).
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
    /// Linear fader gain.
    pub fader: f32,
    /// -1.0 is hard left (the odd buses), 1.0 hard right (the even buses).
    pub pan: f32,
    pub mute: bool,
    /// ASSIGN 1-2, 3-4, 5-6 and 7-8.
    pub assign: [bool; 4],
    /// ASSIGN L-R: to the stereo bus.
    pub lr: bool,
    /// The track with the same number records this channel after TRIM instead of its bus.
    pub direct: bool,
}

impl Default for ChannelParams {
    fn default() -> Self {
        Self {
            source: Source::Remix,
            input: None,
            trim: 1.0,
            fader: 1.0,
            pan: 0.0,
            mute: false,
            assign: [false; 4],
            lr: false,
            direct: false,
        }
    }
}

impl ChannelParams {
    /// MIC or LINE with an input the device has: a live source.
    pub(crate) fn is_live(&self, input_channels: usize) -> bool {
        self.source != Source::Remix && self.input.is_some_and(|c| usize::from(c) < input_channels)
    }
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
}

impl Default for MixerParams {
    fn default() -> Self {
        Self { odd: 1.0, even: 1.0, mon: MonitorMode::Cue, mono: false }
    }
}

/// Constant-power pan: -3 dB per side in the centre.
pub(crate) fn pan_gains(gain: f32, pan: f32) -> [f32; 2] {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
    [gain * angle.cos(), gain * angle.sin()]
}

/// What a channel hands on for one frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Taps {
    /// After TRIM, before the fader: DIRECT and the PRE meter.
    pub(crate) direct: f32,
    /// After fader and mute, before PAN: the POST meter.
    pub(crate) post: f32,
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
}

impl Default for Channel {
    /// TRIM starts at its target so a recording through DIRECT is exact from the first sample;
    /// the fader fades in, as before.
    fn default() -> Self {
        Self { params: ChannelParams::default(), trim: 1.0, current: [0.0; 2] }
    }
}

impl Channel {
    /// Run one sample of the channel's source through it.
    pub(crate) fn process(&mut self, sample: f32) -> Taps {
        smooth(&mut self.trim, self.params.trim);
        let target = if self.params.mute { [0.0; 2] } else { pan_gains(self.params.fader, self.params.pan) };
        smooth(&mut self.current[0], target[0]);
        smooth(&mut self.current[1], target[1]);
        let direct = sample * self.trim;
        // Constant-power pan keeps left² + right² equal to the fader gain squared.
        let post = direct * self.current[0].hypot(self.current[1]);
        Taps { direct, post, left: direct * self.current[0], right: direct * self.current[1] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(params: ChannelParams) -> Channel {
        let mut channel = Channel { params, ..Channel::default() };
        for _ in 0..20_000 {
            channel.process(0.0);
        }
        channel
    }

    #[test]
    fn trim_comes_before_the_fader_and_pan_splits_left_and_right() {
        let mut channel = settled(ChannelParams { trim: 2.0, fader: 0.5, pan: -1.0, ..ChannelParams::default() });
        let taps = channel.process(0.25);
        assert!((taps.direct - 0.5).abs() < 1e-4, "DIRECT: after TRIM, not the fader");
        assert!((taps.post - 0.25).abs() < 1e-4, "POST: after the fader");
        assert!((taps.left - 0.25).abs() < 1e-4 && taps.right.abs() < 1e-4, "hard left: all to the odd side");
    }

    #[test]
    fn mute_silences_everything_after_the_fader_only() {
        let mut channel = settled(ChannelParams { mute: true, ..ChannelParams::default() });
        let taps = channel.process(1.0);
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
