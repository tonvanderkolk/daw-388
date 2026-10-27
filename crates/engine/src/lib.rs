//! Platform-independent audio engine of DAW 388.
//!
//! The UI thread owns an [`EngineHandle`]; the audio callback owns the [`Engine`]. Commands travel
//! to the callback through a lock-free ring buffer, position and meters come back through atomics,
//! and track buffers the engine lets go of travel back through a second ring buffer so they are
//! freed on the UI thread. [`Engine::render`] never allocates, locks or does I/O.
//!
//! The engine knows nothing about audio devices: a platform crate (desktop: cpal, later the
//! browser: WebAudio) calls [`Engine::render`] from its callback.
//!
//! The transport follows the 388's: a tape position (in samples at the engine rate, fractional
//! so the pitch control can vary the speed), a zero point set with RESET, one CUE point in
//! memory, RTZ and STC searches, fast winding and REPEAT between zero and CUE. In
//! [`TransportStyle::Tape`] searches wind through the tape in time, as on the machine; in
//! [`TransportStyle::Cycle`] they jump straight there, as in a modern DAW.

mod record;

pub use record::{RecItem, RecMode, Recorder, input_ring};

use record::{Input, RecordOut};
use std::f32::consts::FRAC_PI_4;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

/// Number of tape tracks, as on the 388.
pub const TRACKS: usize = 8;
/// Winding speed (REW, F FWD and searches) as a multiple of play speed.
pub const WIND_SPEED: f64 = 30.0;
/// Range of the pitch control: plus or minus 15%, as on the 388.
pub const PITCH_RANGE: f64 = 0.15;

const COMMANDS: usize = 256;
const GARBAGE: usize = 64;
/// One-pole smoothing for gain changes (about 5 ms at 48 kHz), so moving a fader does not click.
const SMOOTH: f32 = 0.004;

/// Mono audio of one track, already at the engine's sample rate.
#[derive(Debug, Default)]
pub struct TrackAudio {
    pub name: String,
    pub samples: Vec<f32>,
}

/// How RTZ, STC and REPEAT reach their point.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TransportStyle {
    /// Wind there in time, as the 388 does.
    #[default]
    Tape,
    /// Jump there at once, as a DAW's cycle mode does.
    Cycle,
}

/// What the transport is doing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Motion {
    #[default]
    Stopped,
    Playing,
    Rewinding,
    Forwarding,
}

/// A request from the UI to the audio thread.
#[derive(Debug)]
pub enum Command {
    /// Play; during an RTZ or STC search, play on arrival instead (as on the 388).
    Play,
    Stop,
    /// Wind backwards until STOP or the start.
    Rewind,
    /// Wind forwards until STOP or the end of the longest track.
    FastForward,
    /// Return to the zero point.
    ReturnToZero,
    /// Search to the CUE point (to the start when there is none).
    SearchToCue,
    /// Move the tape to a position in samples at once.
    Locate(f64),
    /// Make the current position the zero point (RESET).
    ResetZero,
    SetZero(f64),
    /// Store the current position as the CUE point.
    MarkCue,
    SetCue(Option<f64>),
    /// Loop from zero to CUE while playing.
    SetRepeat(bool),
    SetStyle(TransportStyle),
    /// Speed as a ratio: 1.0 is normal, 1.15 is 15% faster.
    SetPitch(f64),
    SetTrack(usize, Option<Arc<TrackAudio>>),
    /// Linear gain.
    SetGain(usize, f32),
    /// -1.0 is hard left, 1.0 is hard right.
    SetPan(usize, f32),
    SetMute(usize, bool),
    /// Linear gain of the stereo master.
    SetMaster(f32),
    /// REC FUNCTION: make a track ready to record (or safe).
    SetArmed(usize, bool),
    /// The input channel a track records from (counted from 0), or none.
    SetInputChannel(usize, Option<u16>),
    /// REC: start recording the armed tracks (from stop it also starts the tape, as REC and
    /// PLAY together do on the 388). Without armed tracks the machine goes record-ready.
    Record,
    /// INSERT: armed tracks play the tape until the punch-in, then the input.
    SetInsert(bool),
    /// Hear armed and recording tracks' input through the app (off: use the interface's own
    /// direct monitoring).
    SetMonitoring(bool),
    /// AUTO PUNCH: after REC, record only between the zero point and the CUE point.
    SetAutoPunch(bool),
    /// Extra latency compensation in seconds on top of what the device reports.
    SetLatencyOffset(f64),
}

const NO_CUE: u64 = u64::MAX;

/// State the audio thread publishes for the UI.
#[derive(Debug)]
struct Shared {
    /// Tape position, zero point and CUE point as `f64` bits ([`NO_CUE`]: none).
    position: AtomicU64,
    zero: AtomicU64,
    cue: AtomicU64,
    motion: AtomicU8,
    /// Highest peak per track since the UI last read it, as `f32` bits: before the fader (the
    /// track's own level, as the 388's track meters show it) and after fader and mute.
    track_peaks: [AtomicU32; TRACKS],
    track_peaks_post: [AtomicU32; TRACKS],
    master_peaks: [AtomicU32; 2],
    rec_mode: AtomicU8,
    /// Bit per track that is recording.
    recording: AtomicU8,
    /// Input channels attached (0: no input).
    input_channels: AtomicU32,
    /// Recorded audio was lost because the writer fell behind.
    overflow: AtomicBool,
    /// Latency compensation in use, in seconds, as `f64` bits.
    compensation: AtomicU64,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            position: AtomicU64::new(0f64.to_bits()),
            zero: AtomicU64::new(0f64.to_bits()),
            cue: AtomicU64::new(NO_CUE),
            motion: AtomicU8::new(0),
            track_peaks: Default::default(),
            track_peaks_post: Default::default(),
            master_peaks: Default::default(),
            rec_mode: AtomicU8::new(0),
            recording: AtomicU8::new(0),
            input_channels: AtomicU32::new(0),
            overflow: AtomicBool::new(false),
            compensation: AtomicU64::new(0f64.to_bits()),
        }
    }
}

fn rec_mode_code(mode: RecMode) -> u8 {
    match mode {
        RecMode::Off => 0,
        RecMode::Ready => 1,
        RecMode::Recording => 2,
    }
}

fn rec_mode_from(code: u8) -> RecMode {
    match code {
        1 => RecMode::Ready,
        2 => RecMode::Recording,
        _ => RecMode::Off,
    }
}

fn motion_code(motion: Motion) -> u8 {
    match motion {
        Motion::Stopped => 0,
        Motion::Playing => 1,
        Motion::Rewinding => 2,
        Motion::Forwarding => 3,
    }
}

fn motion_from(code: u8) -> Motion {
    match code {
        1 => Motion::Playing,
        2 => Motion::Rewinding,
        3 => Motion::Forwarding,
        _ => Motion::Stopped,
    }
}

/// Raise the stored peak to `value` when it is higher.
fn store_peak(cell: &AtomicU32, value: f32) {
    let mut current = cell.load(Ordering::Relaxed);
    while value > f32::from_bits(current) {
        match cell.compare_exchange_weak(current, value.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(actual) => current = actual,
        }
    }
}

/// Move a gain one step towards its target, landing on it exactly once it is close (so a closed
/// fader is truly silent and no denormals build up).
fn smooth(current: &mut f32, target: f32) {
    let difference = target - *current;
    if difference.abs() < 1e-6 { *current = target } else { *current += difference * SMOOTH }
}

fn take_peak(cell: &AtomicU32) -> f32 {
    f32::from_bits(cell.swap(0, Ordering::Relaxed))
}

/// The sample at a fractional position, with 4-point Hermite interpolation (exact on whole
/// positions); silence outside the track.
fn sample_at(samples: &[f32], position: f64) -> f32 {
    if position < -1.0 || position >= samples.len() as f64 + 1.0 {
        return 0.0;
    }
    let index = position.floor();
    let t = (position - index) as f32;
    let i = index as i64;
    let at = |k: i64| usize::try_from(k).ok().and_then(|k| samples.get(k)).copied().unwrap_or(0.0);
    let (y0, y1, y2, y3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
    if t == 0.0 {
        return y1;
    }
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * t + c2) * t + c1) * t + y1
}

/// A track's peak level before the fader and after fader and mute (pan does not count: the
/// track is mono).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TrackPeaks {
    pub pre: f32,
    pub post: f32,
}

/// Build a connected handle and engine for a device running at `sample_rate`.
pub fn create(sample_rate: u32) -> (EngineHandle, Engine) {
    let (commands, command_rx) = rtrb::RingBuffer::new(COMMANDS);
    let (garbage_tx, garbage) = rtrb::RingBuffer::new(GARBAGE);
    let shared = Arc::new(Shared::default());
    let (rec_out, recorder) = record::recorder(sample_rate);
    let handle = EngineHandle { sample_rate, commands, garbage, shared: Arc::clone(&shared), recorder: Some(recorder) };
    let engine = Engine {
        commands: command_rx,
        garbage: garbage_tx,
        shared,
        tracks: Default::default(),
        master: 1.0,
        master_current: 1.0,
        position: 0.0,
        state: State::Stopped,
        zero: 0.0,
        cue: None,
        repeat: false,
        style: TransportStyle::Tape,
        pitch: 1.0,
        end: 0.0,
        sample_rate,
        input: None,
        latency_source: None,
        latency_offset: 0.0,
        armed: [false; TRACKS],
        input_channel: [None; TRACKS],
        recording: [false; TRACKS],
        rec_mode: RecMode::Off,
        auto_record: false,
        insert: false,
        monitoring: true,
        auto_punch: false,
        rec_out,
        compensation: 0.0,
    };
    (handle, engine)
}

/// The UI side of the engine.
pub struct EngineHandle {
    sample_rate: u32,
    commands: rtrb::Producer<Command>,
    garbage: rtrb::Consumer<Arc<TrackAudio>>,
    shared: Arc<Shared>,
    recorder: Option<Recorder>,
}

impl EngineHandle {
    /// The recorded audio's way out, for a writer on another thread. Can be taken once.
    pub fn take_recorder(&mut self) -> Option<Recorder> {
        self.recorder.take()
    }

    pub fn rec_mode(&self) -> RecMode {
        rec_mode_from(self.shared.rec_mode.load(Ordering::Relaxed))
    }

    pub fn is_recording(&self, track: usize) -> bool {
        track < TRACKS && self.shared.recording.load(Ordering::Relaxed) & (1 << track) != 0
    }

    /// Number of input channels attached; 0 when there is no input.
    pub fn input_channels(&self) -> usize {
        self.shared.input_channels.load(Ordering::Relaxed) as usize
    }

    /// Whether recorded audio was lost since the last call (the writer fell behind).
    pub fn take_overflow(&self) -> bool {
        self.shared.overflow.swap(false, Ordering::Relaxed)
    }

    /// The latency compensation in use, in seconds.
    pub fn compensation_seconds(&self) -> f64 {
        f64::from_bits(self.shared.compensation.load(Ordering::Relaxed))
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Queue a command. Returns `false` when the queue is full (the audio thread has stalled).
    pub fn send(&mut self, command: Command) -> bool {
        self.commands.push(command).is_ok()
    }

    pub fn play(&mut self) {
        self.send(Command::Play);
    }

    pub fn stop(&mut self) {
        self.send(Command::Stop);
    }

    /// Return to zero.
    pub fn rtz(&mut self) {
        self.send(Command::ReturnToZero);
    }

    fn samples(&self, seconds: f64) -> f64 {
        seconds.max(0.0) * f64::from(self.sample_rate)
    }

    fn seconds(&self, samples: f64) -> f64 {
        samples / f64::from(self.sample_rate)
    }

    pub fn locate_seconds(&mut self, seconds: f64) {
        let samples = self.samples(seconds);
        self.send(Command::Locate(samples));
    }

    pub fn set_zero_seconds(&mut self, seconds: f64) {
        let samples = self.samples(seconds);
        self.send(Command::SetZero(samples));
    }

    pub fn set_cue_seconds(&mut self, seconds: Option<f64>) {
        let samples = seconds.map(|s| self.samples(s));
        self.send(Command::SetCue(samples));
    }

    /// Tape position in samples from the start of the tape.
    pub fn position_samples(&self) -> f64 {
        f64::from_bits(self.shared.position.load(Ordering::Relaxed))
    }

    /// Tape position in seconds from the start of the tape.
    pub fn position_seconds(&self) -> f64 {
        self.seconds(self.position_samples())
    }

    /// The zero point in seconds from the start of the tape.
    pub fn zero_seconds(&self) -> f64 {
        self.seconds(f64::from_bits(self.shared.zero.load(Ordering::Relaxed)))
    }

    /// The CUE point in seconds from the start of the tape, if one is stored.
    pub fn cue_seconds(&self) -> Option<f64> {
        let bits = self.shared.cue.load(Ordering::Relaxed);
        (bits != NO_CUE).then(|| self.seconds(f64::from_bits(bits)))
    }

    /// The counter: seconds from the zero point, negative before it.
    pub fn counter_seconds(&self) -> f64 {
        self.position_seconds() - self.zero_seconds()
    }

    pub fn motion(&self) -> Motion {
        motion_from(self.shared.motion.load(Ordering::Relaxed))
    }

    pub fn is_playing(&self) -> bool {
        self.motion() == Motion::Playing
    }

    /// Peaks of a track since the last call, linear.
    pub fn take_track_peaks(&self, track: usize) -> TrackPeaks {
        TrackPeaks {
            pre: self.shared.track_peaks.get(track).map_or(0.0, take_peak),
            post: self.shared.track_peaks_post.get(track).map_or(0.0, take_peak),
        }
    }

    /// Peaks of the stereo master since the last call, linear.
    pub fn take_master_peaks(&self) -> [f32; 2] {
        [take_peak(&self.shared.master_peaks[0]), take_peak(&self.shared.master_peaks[1])]
    }

    /// Free track buffers the audio thread no longer uses. Call regularly from the UI thread.
    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
    }
}

#[derive(Debug)]
struct Track {
    audio: Option<Arc<TrackAudio>>,
    gain: f32,
    pan: f32,
    mute: bool,
    /// Smoothed left and right gains.
    current: [f32; 2],
}

impl Default for Track {
    fn default() -> Self {
        Self { audio: None, gain: 1.0, pan: 0.0, mute: false, current: [0.0; 2] }
    }
}

impl Track {
    /// Constant-power pan: -3 dB per side in the centre.
    fn target(&self) -> [f32; 2] {
        if self.mute {
            return [0.0; 2];
        }
        let angle = (self.pan.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
        [self.gain * angle.cos(), self.gain * angle.sin()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Stopped,
    Playing,
    /// Moving at winding speed; `speed` is negative when rewinding. With a `target` it is a
    /// search (RTZ, STC, REPEAT) that stops there, or plays on when `then_play` is set.
    Winding {
        speed: f64,
        target: Option<f64>,
        then_play: bool,
    },
}

/// The audio-thread side of the engine.
pub struct Engine {
    commands: rtrb::Consumer<Command>,
    garbage: rtrb::Producer<Arc<TrackAudio>>,
    shared: Arc<Shared>,
    tracks: [Track; TRACKS],
    master: f32,
    /// Smoothed master gain.
    master_current: f32,
    position: f64,
    state: State,
    zero: f64,
    cue: Option<f64>,
    repeat: bool,
    style: TransportStyle,
    pitch: f64,
    /// Length of the longest track in samples; playback and winding stop there.
    end: f64,
    sample_rate: u32,
    input: Option<Input>,
    /// Device latency (input plus output) in seconds as `f64` bits, measured by the platform.
    latency_source: Option<Arc<AtomicU64>>,
    latency_offset: f64,
    armed: [bool; TRACKS],
    input_channel: [Option<usize>; TRACKS],
    recording: [bool; TRACKS],
    rec_mode: RecMode,
    /// REC was pressed with AUTO PUNCH on: recording follows the zero and CUE points.
    auto_record: bool,
    insert: bool,
    monitoring: bool,
    auto_punch: bool,
    rec_out: RecordOut,
    /// Latency compensation in samples for this block.
    compensation: f64,
}

impl Engine {
    /// Connect the input: the consumer of [`input_ring`], fed by the platform's input callback
    /// with interleaved frames of `channels` channels. `fill` is how many frames to keep in the
    /// ring (at least one input and one output buffer).
    pub fn attach_input(&mut self, rx: rtrb::Consumer<f32>, channels: usize, fill: usize) {
        self.shared.input_channels.store(channels as u32, Ordering::Relaxed);
        self.input = Some(Input::new(rx, channels, fill));
    }

    /// Where the platform publishes the device latency (input plus output, seconds as `f64`
    /// bits), used to place recordings exactly under the tape.
    pub fn set_latency_source(&mut self, latency: Arc<AtomicU64>) {
        self.latency_source = Some(latency);
    }

    fn update_compensation(&mut self) {
        let device = self.latency_source.as_ref().map_or(0.0, |l| f64::from_bits(l.load(Ordering::Relaxed)));
        let fill = self.input.as_ref().map_or(0, |i| i.target) as f64;
        self.compensation = ((device + self.latency_offset) * f64::from(self.sample_rate) + fill).max(0.0);
    }

    fn can_record(&self, track: usize) -> bool {
        let channels = self.input.as_ref().map_or(0, |i| i.channels);
        self.armed[track] && self.input_channel[track].is_some_and(|c| c < channels)
    }

    fn punch_in(&mut self, track: usize) {
        if self.recording[track] || !self.can_record(track) {
            return;
        }
        let start = self.position - self.compensation;
        if self.rec_out.tracks[track].push(RecItem::Start(start)).is_err() {
            self.shared.overflow.store(true, Ordering::Relaxed);
            return;
        }
        self.recording[track] = true;
        self.rec_mode = RecMode::Recording;
    }

    fn punch_in_armed(&mut self) {
        for track in 0..TRACKS {
            self.punch_in(track);
        }
    }

    fn punch_out(&mut self, track: usize) {
        if !self.recording[track] {
            return;
        }
        self.recording[track] = false;
        // The writer drains continuously; a full ring here means it is gone, and the take with it.
        if self.rec_out.tracks[track].push(RecItem::Stop).is_err() {
            self.shared.overflow.store(true, Ordering::Relaxed);
        }
        if self.rec_mode == RecMode::Recording && !self.recording.iter().any(|r| *r) {
            self.rec_mode = RecMode::Ready;
        }
    }

    fn punch_out_all(&mut self) {
        for track in 0..TRACKS {
            self.punch_out(track);
        }
    }

    /// Leave record mode: punch out everything (STOP, PLAY, REW and the other transport keys).
    fn end_recording(&mut self) {
        self.punch_out_all();
        self.rec_mode = RecMode::Off;
        self.auto_record = false;
    }

    /// The span AUTO PUNCH records: zero point to CUE point.
    fn punch_span(&self) -> Option<(f64, f64)> {
        self.cue.filter(|&cue| cue > self.zero + 1.0).map(|cue| (self.zero, cue))
    }

    fn record(&mut self) {
        if (self.pitch - 1.0).abs() > 1e-9 || matches!(self.state, State::Winding { .. }) {
            return;
        }
        self.update_compensation();
        self.state = State::Playing;
        if self.rec_mode == RecMode::Off {
            self.rec_mode = RecMode::Ready;
        }
        if self.auto_punch && self.punch_span().is_some() {
            self.auto_record = true;
        } else {
            self.punch_in_armed();
        }
    }

    /// Go to a point: wind there in tape style, jump there in cycle style (keeping the motion).
    fn search(&mut self, target: f64, then_play: bool) {
        let target = target.max(0.0);
        match self.style {
            TransportStyle::Cycle => {
                self.position = target;
                if then_play {
                    self.state = State::Playing;
                }
            }
            TransportStyle::Tape if (target - self.position).abs() < 0.5 => {
                self.position = target;
                self.state = if then_play { State::Playing } else { State::Stopped };
            }
            TransportStyle::Tape => {
                let speed = if target < self.position { -WIND_SPEED } else { WIND_SPEED };
                self.state = State::Winding { speed, target: Some(target), then_play };
            }
        }
    }

    /// Apply queued commands. A track swap only happens when the old buffer can be handed back,
    /// so nothing is ever freed on the audio thread.
    fn apply_commands(&mut self) {
        while let Ok(command) = self.commands.peek() {
            if matches!(command, Command::SetTrack(..)) && self.garbage.slots() == 0 {
                break;
            }
            let Ok(command) = self.commands.pop() else { break };
            match command {
                Command::Play => {
                    // PLAY while recording punches out and plays on.
                    self.end_recording();
                    match &mut self.state {
                        State::Winding { target: Some(_), then_play, .. } => *then_play = true,
                        state => *state = State::Playing,
                    }
                }
                Command::Stop => {
                    self.end_recording();
                    self.state = State::Stopped;
                }
                Command::Rewind => {
                    self.end_recording();
                    self.state = State::Winding { speed: -WIND_SPEED, target: None, then_play: false };
                }
                Command::FastForward => {
                    self.end_recording();
                    self.state = State::Winding { speed: WIND_SPEED, target: None, then_play: false };
                }
                Command::Record => self.record(),
                Command::SetArmed(track, armed) => {
                    if track < TRACKS {
                        self.armed[track] = armed;
                        if !armed {
                            self.punch_out(track);
                        } else if self.rec_mode != RecMode::Off && !self.auto_record && self.state == State::Playing {
                            // Record-ready: pressing REC FUNCTION punches that track in.
                            self.punch_in(track);
                        }
                    }
                }
                Command::SetInputChannel(track, channel) => {
                    if track < TRACKS {
                        self.punch_out(track);
                        self.input_channel[track] = channel.map(usize::from);
                    }
                }
                Command::SetInsert(insert) => self.insert = insert,
                Command::SetMonitoring(monitoring) => self.monitoring = monitoring,
                Command::SetAutoPunch(auto) => self.auto_punch = auto,
                Command::SetLatencyOffset(seconds) => self.latency_offset = seconds,
                Command::ReturnToZero => {
                    self.end_recording();
                    // In cycle style RTZ keeps playing, as "go to start" does in a DAW.
                    let keep_playing = self.style == TransportStyle::Cycle && self.state == State::Playing;
                    self.search(self.zero, keep_playing);
                }
                Command::SearchToCue => {
                    self.end_recording();
                    let keep_playing = self.style == TransportStyle::Cycle && self.state == State::Playing;
                    self.search(self.cue.unwrap_or(0.0), keep_playing);
                }
                Command::Locate(position) => {
                    self.end_recording();
                    self.position = position.max(0.0);
                }
                Command::ResetZero => self.zero = self.position,
                Command::SetZero(zero) => self.zero = zero.max(0.0),
                Command::MarkCue => self.cue = Some(self.position),
                Command::SetCue(cue) => self.cue = cue.map(|c| c.max(0.0)),
                Command::SetRepeat(repeat) => self.repeat = repeat,
                Command::SetStyle(style) => self.style = style,
                Command::SetPitch(pitch) => {
                    // Recording runs at normal speed only.
                    if self.rec_mode == RecMode::Off {
                        self.pitch = pitch.clamp(1.0 - PITCH_RANGE, 1.0 + PITCH_RANGE);
                    }
                }
                Command::SetTrack(index, audio) => {
                    if let Some(track) = self.tracks.get_mut(index) {
                        let old = std::mem::replace(&mut track.audio, audio);
                        if let Some(old) = old {
                            // Room was checked above.
                            let _ = self.garbage.push(old);
                        }
                    }
                    self.end = self.tracks.iter().filter_map(|t| t.audio.as_ref()).map(|a| a.samples.len()).max().unwrap_or(0) as f64;
                }
                Command::SetGain(index, gain) => {
                    if let Some(track) = self.tracks.get_mut(index) {
                        track.gain = gain.max(0.0);
                    }
                }
                Command::SetPan(index, pan) => {
                    if let Some(track) = self.tracks.get_mut(index) {
                        track.pan = pan;
                    }
                }
                Command::SetMute(index, mute) => {
                    if let Some(track) = self.tracks.get_mut(index) {
                        track.mute = mute;
                    }
                }
                Command::SetMaster(gain) => self.master = gain.max(0.0),
            }
        }
    }

    /// The end of the loop when REPEAT applies: a CUE point after the zero point.
    fn loop_end(&self) -> Option<f64> {
        self.cue.filter(|&cue| self.repeat && cue > self.zero + 1.0)
    }

    /// Advance the tape by one sample of time.
    fn advance(&mut self) {
        match self.state {
            State::Stopped => {}
            State::Playing => {
                let previous = self.position;
                self.position += self.pitch;
                if self.rec_mode != RecMode::Off {
                    if self.auto_record {
                        if let Some((zero, cue)) = self.punch_span() {
                            if self.position >= zero && self.position < cue {
                                self.punch_in_armed();
                            } else {
                                self.punch_out_all();
                            }
                        }
                    } else if let Some(cue) = self.cue
                        && self.recording.iter().any(|r| *r)
                        && previous < cue
                        && self.position >= cue
                    {
                        // The CUE point punches out; the transport stops, or winds back with REPEAT.
                        self.end_recording();
                        if self.loop_end().is_none() {
                            self.state = State::Stopped;
                            return;
                        }
                    }
                }
                if let Some(end) = self.loop_end()
                    && self.position >= end
                {
                    if self.auto_record {
                        // A new pass: the next take starts at the zero point again.
                        self.punch_out_all();
                    }
                    self.search(self.zero, true);
                } else if self.rec_mode == RecMode::Off && self.end > 0.0 && self.position >= self.end {
                    // While recording the tape runs on past the end of the longest track.
                    self.state = State::Stopped;
                }
            }
            State::Winding { speed, target, then_play } => {
                self.position += speed;
                if let Some(target) = target
                    && ((speed < 0.0 && self.position <= target) || (speed > 0.0 && self.position >= target))
                {
                    self.position = target;
                    self.state = if then_play { State::Playing } else { State::Stopped };
                } else if self.position <= 0.0 {
                    self.position = 0.0;
                    self.state = State::Stopped;
                } else if self.position >= self.end {
                    self.position = self.end;
                    self.state = State::Stopped;
                }
            }
        }
    }

    fn motion(&self) -> Motion {
        match self.state {
            State::Stopped => Motion::Stopped,
            State::Playing => Motion::Playing,
            State::Winding { speed, .. } if speed < 0.0 => Motion::Rewinding,
            State::Winding { .. } => Motion::Forwarding,
        }
    }

    /// Fill an interleaved output buffer with `channels` channels. Channel 0 and 1 get the stereo
    /// mix, further channels silence; a mono device gets the average of left and right. Winding
    /// is silent, as on the 388.
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        self.apply_commands();
        self.update_compensation();
        let channels = channels.max(1);
        let mut track_peaks = [0.0f32; TRACKS];
        let mut track_peaks_post = [0.0f32; TRACKS];
        let mut master_peaks = [0.0f32; 2];

        for frame in out.chunks_mut(channels) {
            let mut mix = [0.0f32; 2];
            let playing = self.state == State::Playing;
            if let Some(input) = &mut self.input {
                input.next_frame();
            }
            for (index, ((track, peak), peak_post)) in
                self.tracks.iter_mut().zip(track_peaks.iter_mut()).zip(track_peaks_post.iter_mut()).enumerate()
            {
                let target = track.target();
                smooth(&mut track.current[0], target[0]);
                smooth(&mut track.current[1], target[1]);
                let input = self.input.as_ref().zip(self.input_channel[index]).map(|(input, channel)| input.sample(channel));
                if self.recording[index] {
                    let value = input.unwrap_or(0.0);
                    if self.rec_out.tracks[index].push(RecItem::Sample(value)).is_err() {
                        self.shared.overflow.store(true, Ordering::Relaxed);
                    }
                }
                // What the channel hears, as the 388's switching logic decides: an armed track
                // hears its input, except with INSERT while the tape plays and it is not (yet)
                // recording; a recording track never hears the old take.
                let hears_input = input.is_some() && self.armed[index] && (self.recording[index] || !self.insert || !playing);
                let sample = if hears_input {
                    if self.monitoring { input.unwrap_or(0.0) } else { 0.0 }
                } else if playing {
                    track.audio.as_ref().map_or(0.0, |a| sample_at(&a.samples, self.position))
                } else {
                    continue;
                };
                *peak = peak.max(sample.abs());
                // Constant-power pan keeps left² + right² equal to the fader gain squared.
                let gain = track.current[0].hypot(track.current[1]);
                *peak_post = peak_post.max(sample.abs() * gain);
                mix[0] += sample * track.current[0];
                mix[1] += sample * track.current[1];
            }
            self.advance();
            smooth(&mut self.master_current, self.master);
            let left = mix[0] * self.master_current;
            let right = mix[1] * self.master_current;
            master_peaks[0] = master_peaks[0].max(left.abs());
            master_peaks[1] = master_peaks[1].max(right.abs());
            match frame {
                [mono] => *mono = 0.5 * (left + right),
                [l, r, rest @ ..] => {
                    *l = left;
                    *r = right;
                    rest.fill(0.0);
                }
                [] => {}
            }
        }

        self.shared.position.store(self.position.to_bits(), Ordering::Relaxed);
        self.shared.zero.store(self.zero.to_bits(), Ordering::Relaxed);
        self.shared.cue.store(self.cue.map_or(NO_CUE, f64::to_bits), Ordering::Relaxed);
        self.shared.motion.store(motion_code(self.motion()), Ordering::Relaxed);
        self.shared.rec_mode.store(rec_mode_code(self.rec_mode), Ordering::Relaxed);
        let mask = self.recording.iter().enumerate().fold(0u8, |m, (i, r)| if *r { m | (1 << i) } else { m });
        self.shared.recording.store(mask, Ordering::Relaxed);
        self.shared.compensation.store((self.compensation / f64::from(self.sample_rate)).to_bits(), Ordering::Relaxed);
        for (cell, peak) in self.shared.track_peaks.iter().zip(track_peaks) {
            store_peak(cell, peak);
        }
        for (cell, peak) in self.shared.track_peaks_post.iter().zip(track_peaks_post) {
            store_peak(cell, peak);
        }
        for (cell, peak) in self.shared.master_peaks.iter().zip(master_peaks) {
            store_peak(cell, peak);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(samples: Vec<f32>) -> Option<Arc<TrackAudio>> {
        Some(Arc::new(TrackAudio { name: "test".into(), samples }))
    }

    /// Let the gain smoothing settle so the next block plays at full target gain.
    fn settle(engine: &mut Engine) {
        let mut scratch = vec![0.0; 2 * 20_000];
        engine.render(&mut scratch, 2);
    }

    #[test]
    fn stopped_engine_is_silent_and_does_not_move() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![1.0; 100])));
        let mut out = vec![1.0; 64];
        engine.render(&mut out, 2);
        assert!(out.iter().all(|s| *s == 0.0));
        assert_eq!(handle.position_samples(), 0.0);
        assert!(!handle.is_playing());
    }

    #[test]
    fn plays_all_tracks_in_sync_and_mixes_to_stereo() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![0.25; 100_000])));
        handle.send(Command::SetTrack(7, audio(vec![0.5; 100_000])));
        handle.send(Command::SetPan(0, -1.0));
        handle.send(Command::SetPan(7, 1.0));
        settle(&mut engine);
        handle.play();
        let mut out = vec![0.0; 2 * 4];
        engine.render(&mut out, 2);
        for frame in out.chunks(2) {
            assert!((frame[0] - 0.25).abs() < 1e-4, "left {}", frame[0]);
            assert!((frame[1] - 0.5).abs() < 1e-4, "right {}", frame[1]);
        }
        assert_eq!(handle.position_samples(), 4.0);
        assert!(handle.is_playing());
        assert!((handle.take_track_peaks(0).pre - 0.25).abs() < 1e-6);
        assert_eq!(handle.take_track_peaks(0), TrackPeaks::default());
    }

    #[test]
    fn centre_pan_is_constant_power() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![1.0; 100_000])));
        settle(&mut engine);
        handle.play();
        let mut out = vec![0.0; 2];
        engine.render(&mut out, 2);
        let expected = std::f32::consts::FRAC_1_SQRT_2;
        assert!((out[0] - expected).abs() < 1e-4 && (out[1] - expected).abs() < 1e-4);
    }

    #[test]
    fn stops_at_the_end_of_the_longest_track() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![0.1; 10])));
        handle.send(Command::SetTrack(1, audio(vec![0.1; 20])));
        handle.play();
        let mut out = vec![0.0; 2 * 64];
        engine.render(&mut out, 2);
        assert_eq!(handle.position_samples(), 20.0);
        assert!(!handle.is_playing());
    }

    /// Render `frames` frames of stereo and return them.
    fn run(engine: &mut Engine, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; 2 * frames];
        engine.render(&mut out, 2);
        out
    }

    #[test]
    fn locate_moves_the_tape_at_once() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![0.1; 1_000])));
        handle.locate_seconds(0.01);
        run(&mut engine, 0);
        assert_eq!(handle.position_samples(), 480.0);
    }

    #[test]
    fn rewind_and_fast_forward_wind_silently_and_stop_at_the_ends() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![0.5; 3_000])));
        handle.send(Command::FastForward);
        let out = run(&mut engine, 10);
        assert!(out.iter().all(|s| *s == 0.0), "winding is silent");
        assert_eq!(handle.position_samples(), 10.0 * WIND_SPEED);
        assert_eq!(handle.motion(), Motion::Forwarding);
        run(&mut engine, 1_000);
        assert_eq!(handle.position_samples(), 3_000.0, "stops at the end of the longest track");
        assert_eq!(handle.motion(), Motion::Stopped);
        handle.send(Command::Rewind);
        run(&mut engine, 1);
        assert_eq!(handle.motion(), Motion::Rewinding);
        run(&mut engine, 1_000);
        assert_eq!(handle.position_samples(), 0.0);
        assert_eq!(handle.motion(), Motion::Stopped);
    }

    #[test]
    fn tape_style_rtz_winds_back_and_stops_unless_play_was_pressed() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![0.5; 10_000])));
        handle.send(Command::Locate(3_000.0));
        handle.play();
        run(&mut engine, 10);
        handle.rtz();
        run(&mut engine, 10);
        assert_eq!(handle.motion(), Motion::Rewinding, "RTZ winds in time");
        assert!((handle.position_samples() - (3_010.0 - 10.0 * WIND_SPEED)).abs() < 1e-6);
        run(&mut engine, 200);
        assert_eq!(handle.position_samples(), 0.0);
        assert_eq!(handle.motion(), Motion::Stopped, "and stops at zero");

        // PLAY during the search: play on arrival.
        handle.send(Command::Locate(3_000.0));
        handle.rtz();
        run(&mut engine, 10);
        handle.play();
        run(&mut engine, 10);
        assert_eq!(handle.motion(), Motion::Rewinding, "PLAY does not interrupt the search");
        run(&mut engine, 200);
        assert_eq!(handle.motion(), Motion::Playing);
    }

    #[test]
    fn cycle_style_rtz_jumps_and_keeps_playing() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![0.5; 10_000])));
        handle.send(Command::SetStyle(TransportStyle::Cycle));
        handle.send(Command::Locate(3_000.0));
        handle.play();
        run(&mut engine, 10);
        handle.rtz();
        run(&mut engine, 5);
        assert_eq!(handle.position_samples(), 5.0);
        assert_eq!(handle.motion(), Motion::Playing);
    }

    #[test]
    fn reset_sets_the_zero_point_and_cue_is_remembered() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![0.5; 100_000])));
        handle.send(Command::Locate(4_800.0));
        handle.send(Command::ResetZero);
        run(&mut engine, 0);
        assert!((handle.zero_seconds() - 0.1).abs() < 1e-9);
        assert!(handle.counter_seconds().abs() < 1e-9, "the counter reads 00:00 at the new zero");
        handle.send(Command::Locate(0.0));
        run(&mut engine, 0);
        assert!((handle.counter_seconds() + 0.1).abs() < 1e-9, "negative before zero");
        assert_eq!(handle.cue_seconds(), None);
        handle.send(Command::Locate(9_600.0));
        handle.send(Command::MarkCue);
        run(&mut engine, 0);
        assert!((handle.cue_seconds().unwrap_or_default() - 0.2).abs() < 1e-9);
        // STC goes to the cue, RESET does not clear it.
        handle.send(Command::SetStyle(TransportStyle::Cycle));
        handle.send(Command::Locate(0.0));
        handle.send(Command::ResetZero);
        handle.send(Command::SearchToCue);
        run(&mut engine, 0);
        assert_eq!(handle.position_samples(), 9_600.0);
        assert!(handle.cue_seconds().is_some());
    }

    #[test]
    fn search_to_cue_without_a_cue_goes_to_the_start() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![0.5; 10_000])));
        handle.send(Command::Locate(600.0));
        handle.send(Command::SearchToCue);
        run(&mut engine, 100);
        assert_eq!(handle.position_samples(), 0.0);
    }

    #[test]
    fn repeat_in_tape_style_winds_back_to_zero_and_plays_again() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![0.5; 100_000])));
        handle.send(Command::Locate(1_000.0));
        handle.send(Command::ResetZero);
        handle.send(Command::SetCue(Some(4_000.0)));
        handle.send(Command::SetRepeat(true));
        handle.play();
        run(&mut engine, 3_000);
        assert_eq!(handle.motion(), Motion::Rewinding, "at the cue the tape winds back");
        run(&mut engine, 200);
        assert_eq!(handle.motion(), Motion::Playing, "and plays again from zero");
        assert!(handle.position_samples() < 1_300.0);
    }

    #[test]
    fn repeat_in_cycle_style_jumps_without_a_gap() {
        let (mut handle, mut engine) = create(48_000);
        let ramp: Vec<f32> = (0..10_000).map(|n| n as f32 / 10_000.0).collect();
        handle.send(Command::SetTrack(0, audio(ramp)));
        handle.send(Command::SetPan(0, -1.0));
        handle.send(Command::SetStyle(TransportStyle::Cycle));
        handle.send(Command::SetCue(Some(2_000.0)));
        handle.send(Command::SetRepeat(true));
        settle(&mut engine);
        handle.play();
        let out = run(&mut engine, 2_100);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        assert!(left.iter().all(|s| *s < 0.2001), "never past the cue");
        assert!((left[2_000] - 0.0).abs() < 1e-4 && (left[2_050] - 0.005).abs() < 1e-4, "straight back to zero");
        // Turning REPEAT off lets it play on.
        handle.send(Command::SetRepeat(false));
        run(&mut engine, 3_000);
        assert!(handle.position_samples() > 2_000.0);
    }

    #[test]
    fn pitch_changes_the_speed_and_interpolates() {
        let (mut handle, mut engine) = create(48_000);
        let ramp: Vec<f32> = (0..10_000).map(|n| n as f32 / 10_000.0).collect();
        handle.send(Command::SetTrack(0, audio(ramp)));
        handle.send(Command::SetPan(0, -1.0));
        handle.send(Command::SetPitch(1.1));
        settle(&mut engine);
        handle.play();
        let out = run(&mut engine, 1_000);
        assert!((handle.position_samples() - 1_100.0).abs() < 1e-6);
        // Frame 455 reads tape position 500.5, between samples 500 and 501.
        assert!((out[2 * 455] - 455.0 * 1.1 / 10_000.0).abs() < 1e-4);
        handle.send(Command::SetPitch(2.0));
        run(&mut engine, 100);
        assert!((handle.position_samples() - (1_100.0 + 100.0 * (1.0 + PITCH_RANGE))).abs() < 1e-6, "limited to +15%");
    }

    /// An engine with a two-channel input; returns the input producer and the recorder.
    fn with_input() -> (EngineHandle, Engine, rtrb::Producer<f32>, Recorder) {
        let (mut handle, mut engine) = create(48_000);
        let (tx, rx) = input_ring(48_000, 2);
        engine.attach_input(rx, 2, 1);
        let recorder = handle.take_recorder().unwrap_or_else(|| unreachable!());
        (handle, engine, tx, recorder)
    }

    /// Feed `frames` input frames: channel 0 counts up from `from`, channel 1 is constant.
    fn feed(tx: &mut rtrb::Producer<f32>, from: usize, frames: usize) {
        for n in from..from + frames {
            let _ = tx.push(n as f32);
            let _ = tx.push(-1.0);
        }
    }

    fn takes(recorder: &mut Recorder, track: usize) -> Vec<RecItem> {
        let mut items = Vec::new();
        recorder.drain(track, |item| items.push(item));
        items
    }

    #[test]
    fn rec_records_the_armed_track_from_its_input_and_play_punches_out() {
        let (mut handle, mut engine, mut tx, mut recorder) = with_input();
        handle.send(Command::SetInputChannel(2, Some(0)));
        handle.send(Command::SetArmed(2, true));
        handle.send(Command::Locate(1_000.0));
        handle.send(Command::Record);
        feed(&mut tx, 0, 11);
        run(&mut engine, 10);
        assert_eq!(handle.rec_mode(), RecMode::Recording);
        assert!(handle.is_recording(2) && !handle.is_recording(1));
        assert!(handle.is_playing(), "REC from stop rolls the tape, as REC and PLAY together");
        handle.play();
        run(&mut engine, 0);
        assert_eq!(handle.rec_mode(), RecMode::Off);
        assert!(handle.is_playing(), "PLAY punches out and plays on");
        let items = takes(&mut recorder, 2);
        // Compensation here is the one-frame input fill.
        assert_eq!(items.first(), Some(&RecItem::Start(999.0)));
        assert_eq!(items.last(), Some(&RecItem::Stop));
        let samples: Vec<f32> = items.iter().filter_map(|i| if let RecItem::Sample(v) = i { Some(*v) } else { None }).collect();
        assert_eq!(samples.len(), 10);
        assert_eq!(samples[0] + 1.0, samples[1], "consecutive input frames");
        assert!(takes(&mut recorder, 1).is_empty(), "safe tracks record nothing");
    }

    #[test]
    fn record_ready_and_rec_function_punch_in_and_out() {
        let (mut handle, mut engine, mut tx, mut recorder) = with_input();
        handle.send(Command::SetInputChannel(0, Some(1)));
        handle.play();
        handle.send(Command::Record);
        feed(&mut tx, 0, 100);
        run(&mut engine, 10);
        assert_eq!(handle.rec_mode(), RecMode::Ready, "REC without armed tracks: record-ready");
        handle.send(Command::SetArmed(0, true));
        run(&mut engine, 20);
        assert!(handle.is_recording(0), "REC FUNCTION punches in");
        handle.send(Command::SetArmed(0, false));
        run(&mut engine, 10);
        assert!(!handle.is_recording(0) && handle.rec_mode() == RecMode::Ready, "and out again");
        let items = takes(&mut recorder, 0);
        assert_eq!(items.iter().filter(|i| matches!(i, RecItem::Sample(v) if *v == -1.0)).count(), 20, "channel 1 recorded");
        handle.stop();
        run(&mut engine, 0);
        assert_eq!(handle.rec_mode(), RecMode::Off);
    }

    #[test]
    fn the_cue_point_punches_out_and_stops() {
        let (mut handle, mut engine, mut tx, mut recorder) = with_input();
        handle.send(Command::SetInputChannel(0, Some(0)));
        handle.send(Command::SetArmed(0, true));
        handle.send(Command::SetCue(Some(50.0)));
        handle.send(Command::Record);
        feed(&mut tx, 0, 200);
        run(&mut engine, 100);
        assert_eq!(handle.rec_mode(), RecMode::Off);
        assert_eq!(handle.motion(), Motion::Stopped);
        let items = takes(&mut recorder, 0);
        assert_eq!(items.iter().filter(|i| matches!(i, RecItem::Sample(_))).count(), 50);
        assert_eq!(items.last(), Some(&RecItem::Stop));
    }

    #[test]
    fn auto_punch_records_between_zero_and_cue_on_every_pass() {
        let (mut handle, mut engine, mut tx, mut recorder) = with_input();
        handle.send(Command::SetInputChannel(0, Some(0)));
        handle.send(Command::SetArmed(0, true));
        handle.send(Command::SetStyle(TransportStyle::Cycle));
        handle.send(Command::SetZero(100.0));
        handle.send(Command::SetCue(Some(200.0)));
        handle.send(Command::SetRepeat(true));
        handle.send(Command::SetAutoPunch(true));
        handle.send(Command::Locate(50.0));
        handle.send(Command::Record);
        feed(&mut tx, 0, 500);
        run(&mut engine, 40);
        assert_eq!(handle.rec_mode(), RecMode::Ready, "before the zero point nothing records");
        run(&mut engine, 280);
        let items = takes(&mut recorder, 0);
        let starts = items.iter().filter(|i| matches!(i, RecItem::Start(_))).count();
        let stops = items.iter().filter(|i| matches!(i, RecItem::Stop)).count();
        // 10 frames to the zero point, two passes of 100, then the third under way.
        assert_eq!((starts, stops), (3, 2), "a take on every pass");
        assert_eq!(items.iter().take_while(|i| !matches!(i, RecItem::Stop)).filter(|i| matches!(i, RecItem::Sample(_))).count(), 100);
    }

    #[test]
    fn monitoring_follows_the_388_switching_logic() {
        let (mut handle, mut engine, mut tx, _recorder) = with_input();
        handle.send(Command::SetTrack(0, audio(vec![0.25; 10_000])));
        handle.send(Command::SetPan(0, -1.0));
        handle.send(Command::SetInputChannel(0, Some(1)));
        settle(&mut engine);
        let left = |engine: &mut Engine, tx: &mut rtrb::Producer<f32>| {
            feed(tx, 0, 4);
            run(engine, 2)[2]
        };
        handle.play();
        assert!((left(&mut engine, &mut tx) - 0.25).abs() < 1e-4, "a safe track plays the tape");
        handle.send(Command::SetArmed(0, true));
        assert!((left(&mut engine, &mut tx) + 1.0).abs() < 1e-4, "armed, INSERT off: the input");
        handle.send(Command::SetInsert(true));
        assert!((left(&mut engine, &mut tx) - 0.25).abs() < 1e-4, "INSERT on: the tape until the punch-in");
        handle.send(Command::Record);
        assert!((left(&mut engine, &mut tx) + 1.0).abs() < 1e-4, "recording: the input");
        handle.send(Command::SetMonitoring(false));
        assert!(left(&mut engine, &mut tx).abs() < 1e-4, "direct monitoring: the app plays nothing for it");
    }

    #[test]
    fn no_recording_at_varispeed() {
        let (mut handle, mut engine, mut tx, _recorder) = with_input();
        handle.send(Command::SetInputChannel(0, Some(0)));
        handle.send(Command::SetArmed(0, true));
        handle.send(Command::SetPitch(1.05));
        handle.send(Command::Record);
        feed(&mut tx, 0, 10);
        run(&mut engine, 5);
        assert_eq!(handle.rec_mode(), RecMode::Off);
    }

    #[test]
    fn interpolation_is_exact_on_whole_positions() {
        let samples = [0.0, 1.0, -1.0, 0.5];
        assert_eq!(sample_at(&samples, 1.0), 1.0);
        assert_eq!(sample_at(&samples, 2.0), -1.0);
        assert_eq!(sample_at(&samples, -5.0), 0.0);
        assert_eq!(sample_at(&samples, 10.0), 0.0);
    }

    #[test]
    fn replaced_tracks_come_back_to_the_ui() {
        let (mut handle, mut engine) = create(48_000);
        let first = audio(vec![0.1; 10]);
        let weak = Arc::downgrade(first.as_ref().unwrap_or_else(|| unreachable!()));
        handle.send(Command::SetTrack(0, first));
        engine.render(&mut [], 2);
        handle.send(Command::SetTrack(0, None));
        engine.render(&mut [], 2);
        assert!(weak.upgrade().is_some(), "freed on the audio thread");
        handle.collect_garbage();
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn mute_and_master_apply() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![1.0; 100_000])));
        handle.send(Command::SetMute(0, true));
        settle(&mut engine);
        handle.play();
        let mut out = vec![0.0; 2];
        engine.render(&mut out, 2);
        assert!(out[0].abs() < 1e-4);
        handle.send(Command::SetMute(0, false));
        handle.send(Command::SetMaster(0.0));
        settle(&mut engine);
        engine.render(&mut out, 2);
        assert_eq!(out, [0.0, 0.0]);
    }

    #[test]
    fn post_fader_meter_follows_fader_and_mute_but_not_pan() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![0.8; 100_000])));
        handle.send(Command::SetGain(0, 0.5));
        handle.send(Command::SetPan(0, 1.0));
        settle(&mut engine);
        handle.play();
        let mut out = vec![0.0; 2 * 64];
        engine.render(&mut out, 2);
        let peaks = handle.take_track_peaks(0);
        assert!((peaks.pre - 0.8).abs() < 1e-6, "pre is the track itself");
        assert!((peaks.post - 0.4).abs() < 1e-3, "post follows the fader: {}", peaks.post);
        handle.send(Command::SetMute(0, true));
        settle(&mut engine);
        // Forget the peaks of the fade-out itself.
        handle.take_track_peaks(0);
        engine.render(&mut out, 2);
        let peaks = handle.take_track_peaks(0);
        assert!((peaks.pre - 0.8).abs() < 1e-6);
        assert!(peaks.post < 1e-4, "muted: {}", peaks.post);
    }

    #[test]
    fn master_changes_are_smoothed() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![1.0; 100_000])));
        handle.send(Command::SetPan(0, -1.0));
        settle(&mut engine);
        handle.play();
        handle.send(Command::SetMaster(0.0));
        let mut out = vec![0.0; 2 * 2_000];
        engine.render(&mut out, 2);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        // No jump: every step is small, and the level does fall towards zero.
        assert!(left.windows(2).all(|w| (w[0] - w[1]).abs() < 0.01));
        assert!(left[0] > 0.99 && left[left.len() - 1] < 0.01);
    }

    #[test]
    fn mono_and_multichannel_devices() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![1.0; 100_000])));
        handle.send(Command::SetPan(0, -1.0));
        settle(&mut engine);
        handle.play();
        let mut mono = vec![0.0; 1];
        engine.render(&mut mono, 1);
        assert!((mono[0] - 0.5).abs() < 1e-4);
        let mut four = vec![9.0; 4];
        engine.render(&mut four, 4);
        assert!((four[0] - 1.0).abs() < 1e-4);
        assert_eq!(&four[1..], &[0.0, 0.0, 0.0]);
    }
}
