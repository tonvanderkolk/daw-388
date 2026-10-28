//! The MONITOR section: per track a GAIN and PAN, and the 388's switching logic that decides
//! whether a track's monitor hears its input (the PGM bus or the DIRECT tap), the tape or
//! nothing. The table is in the 388's manual under "Monitor Source with MONITOR/PHONES Selector
//! in STEREO".

use crate::mixer::pan_gains;
use crate::smooth;

/// The MON switch: CUE follows the switching logic, PGM always hears the buses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MonitorMode {
    Pgm,
    #[default]
    Cue,
}

/// One track's MONITOR controls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorParams {
    /// Linear gain.
    pub gain: f32,
    /// -1.0 is hard left, 1.0 is hard right.
    pub pan: f32,
    pub mute: bool,
}

impl Default for MonitorParams {
    fn default() -> Self {
        Self { gain: 1.0, pan: 0.0, mute: false }
    }
}

/// What a track's monitor hears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MonitorSource {
    Input,
    Tape,
    Silent,
}

/// The track hears its input rather than the tape: it is recording, or it is armed with INSERT
/// off. A RMX channel of such a track stays silent, so bus, track and channel never form a loop.
pub(crate) fn hears_input(armed: bool, insert: bool, recording: bool) -> bool {
    armed && (recording || !insert)
}

/// The 388's table: PGM always hears the input; CUE hears the input while the track hears its
/// input, else the tape while it plays, else nothing.
pub(crate) fn monitor_source(mode: MonitorMode, armed: bool, insert: bool, recording: bool, tape_playing: bool) -> MonitorSource {
    match mode {
        MonitorMode::Pgm => MonitorSource::Input,
        MonitorMode::Cue if hears_input(armed, insert, recording) => MonitorSource::Input,
        MonitorMode::Cue if tape_playing => MonitorSource::Tape,
        MonitorMode::Cue => MonitorSource::Silent,
    }
}

/// A track's monitor with its smoothed left and right gains.
#[derive(Debug, Default)]
pub(crate) struct Monitor {
    pub(crate) params: MonitorParams,
    current: [f32; 2],
}

impl Monitor {
    /// This frame's left and right gains, moved one step towards their target.
    pub(crate) fn gains(&mut self) -> [f32; 2] {
        let target = if self.params.mute { [0.0; 2] } else { pan_gains(self.params.gain, self.params.pan) };
        smooth(&mut self.current[0], target[0]);
        smooth(&mut self.current[1], target[1]);
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::MonitorSource::{Input, Silent, Tape};
    use super::*;

    #[test]
    fn the_388_switching_table() {
        let cue = MonitorMode::Cue;
        // (armed, insert, recording, tape playing) -> source
        let rows = [
            ((true, true, true, true), Input),    // armed, INSERT, recording
            ((true, true, false, true), Tape),    // armed, INSERT, playing before the punch-in
            ((true, true, false, false), Silent), // armed, INSERT, tape stopped
            ((true, false, false, false), Input), // armed, INSERT off: always the input
            ((true, false, false, true), Input),
            ((false, false, false, true), Tape),   // safe, playing
            ((false, true, false, false), Silent), // safe, stopped
        ];
        for ((armed, insert, recording, playing), expected) in rows {
            assert_eq!(monitor_source(cue, armed, insert, recording, playing), expected, "{armed} {insert} {recording} {playing}");
        }
        for playing in [false, true] {
            assert_eq!(monitor_source(MonitorMode::Pgm, false, false, false, playing), Input, "PGM always hears the buses");
        }
    }

    #[test]
    fn hears_input_follows_insert() {
        assert!(hears_input(true, false, false));
        assert!(!hears_input(true, true, false), "INSERT: the tape until the punch-in");
        assert!(hears_input(true, true, true));
        assert!(!hears_input(false, false, false));
    }
}
