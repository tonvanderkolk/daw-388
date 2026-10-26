//! Platform-independent audio engine of DAW 388.
//!
//! The UI thread owns an [`EngineHandle`]; the audio callback owns the [`Engine`]. Commands travel
//! to the callback through a lock-free ring buffer, position and meters come back through atomics,
//! and track buffers the engine lets go of travel back through a second ring buffer so they are
//! freed on the UI thread. [`Engine::render`] never allocates, locks or does I/O.
//!
//! The engine knows nothing about audio devices: a platform crate (desktop: cpal, later the
//! browser: WebAudio) calls [`Engine::render`] from its callback.

use std::f32::consts::FRAC_PI_4;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Number of tape tracks, as on the 388.
pub const TRACKS: usize = 8;

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

/// A request from the UI to the audio thread.
#[derive(Debug)]
pub enum Command {
    Play,
    Stop,
    /// Move the playhead to a position in samples.
    Locate(u64),
    SetTrack(usize, Option<Arc<TrackAudio>>),
    /// Linear gain.
    SetGain(usize, f32),
    /// -1.0 is hard left, 1.0 is hard right.
    SetPan(usize, f32),
    SetMute(usize, bool),
    /// Linear gain of the stereo master.
    SetMaster(f32),
}

/// State the audio thread publishes for the UI.
#[derive(Debug, Default)]
struct Shared {
    position: AtomicU64,
    playing: AtomicBool,
    /// Highest peak per track since the UI last read it, as `f32` bits.
    track_peaks: [AtomicU32; TRACKS],
    master_peaks: [AtomicU32; 2],
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

/// Build a connected handle and engine for a device running at `sample_rate`.
pub fn create(sample_rate: u32) -> (EngineHandle, Engine) {
    let (commands, command_rx) = rtrb::RingBuffer::new(COMMANDS);
    let (garbage_tx, garbage) = rtrb::RingBuffer::new(GARBAGE);
    let shared = Arc::new(Shared::default());
    let handle = EngineHandle { sample_rate, commands, garbage, shared: Arc::clone(&shared) };
    let engine = Engine {
        commands: command_rx,
        garbage: garbage_tx,
        shared,
        tracks: Default::default(),
        master: 1.0,
        master_current: 1.0,
        position: 0,
        playing: false,
        end: 0,
    };
    (handle, engine)
}

/// The UI side of the engine.
pub struct EngineHandle {
    sample_rate: u32,
    commands: rtrb::Producer<Command>,
    garbage: rtrb::Consumer<Arc<TrackAudio>>,
    shared: Arc<Shared>,
}

impl EngineHandle {
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
        self.send(Command::Locate(0));
    }

    pub fn locate_seconds(&mut self, seconds: f64) {
        let samples = (seconds.max(0.0) * f64::from(self.sample_rate)).round() as u64;
        self.send(Command::Locate(samples));
    }

    pub fn position_samples(&self) -> u64 {
        self.shared.position.load(Ordering::Relaxed)
    }

    pub fn position_seconds(&self) -> f64 {
        self.position_samples() as f64 / f64::from(self.sample_rate)
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    /// Peak of a track since the last call, linear.
    pub fn take_track_peak(&self, track: usize) -> f32 {
        self.shared.track_peaks.get(track).map_or(0.0, take_peak)
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

/// The audio-thread side of the engine.
pub struct Engine {
    commands: rtrb::Consumer<Command>,
    garbage: rtrb::Producer<Arc<TrackAudio>>,
    shared: Arc<Shared>,
    tracks: [Track; TRACKS],
    master: f32,
    /// Smoothed master gain.
    master_current: f32,
    position: u64,
    playing: bool,
    /// Length of the longest track in samples; playback stops there.
    end: u64,
}

impl Engine {
    /// Apply queued commands. A track swap only happens when the old buffer can be handed back,
    /// so nothing is ever freed on the audio thread.
    fn apply_commands(&mut self) {
        while let Ok(command) = self.commands.peek() {
            if matches!(command, Command::SetTrack(..)) && self.garbage.slots() == 0 {
                break;
            }
            let Ok(command) = self.commands.pop() else { break };
            match command {
                Command::Play => self.playing = true,
                Command::Stop => self.playing = false,
                Command::Locate(position) => self.position = position,
                Command::SetTrack(index, audio) => {
                    if let Some(track) = self.tracks.get_mut(index) {
                        let old = std::mem::replace(&mut track.audio, audio);
                        if let Some(old) = old {
                            // Room was checked above.
                            let _ = self.garbage.push(old);
                        }
                    }
                    self.end = self.tracks.iter().filter_map(|t| t.audio.as_ref()).map(|a| a.samples.len() as u64).max().unwrap_or(0);
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

    /// Fill an interleaved output buffer with `channels` channels. Channel 0 and 1 get the stereo
    /// mix, further channels silence; a mono device gets the average of left and right.
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        self.apply_commands();
        let channels = channels.max(1);
        let mut track_peaks = [0.0f32; TRACKS];
        let mut master_peaks = [0.0f32; 2];

        for frame in out.chunks_mut(channels) {
            let mut mix = [0.0f32; 2];
            if self.playing && self.end > 0 && self.position >= self.end {
                self.playing = false;
            }
            for (track, peak) in self.tracks.iter_mut().zip(track_peaks.iter_mut()) {
                let target = track.target();
                smooth(&mut track.current[0], target[0]);
                smooth(&mut track.current[1], target[1]);
                if !self.playing {
                    continue;
                }
                let sample = track
                    .audio
                    .as_ref()
                    .and_then(|a| usize::try_from(self.position).ok().and_then(|p| a.samples.get(p)))
                    .copied()
                    .unwrap_or(0.0);
                *peak = peak.max(sample.abs());
                mix[0] += sample * track.current[0];
                mix[1] += sample * track.current[1];
            }
            if self.playing {
                self.position += 1;
            }
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

        self.shared.position.store(self.position, Ordering::Relaxed);
        self.shared.playing.store(self.playing, Ordering::Relaxed);
        for (cell, peak) in self.shared.track_peaks.iter().zip(track_peaks) {
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
        assert_eq!(handle.position_samples(), 0);
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
        assert_eq!(handle.position_samples(), 4);
        assert!(handle.is_playing());
        assert!((handle.take_track_peak(0) - 0.25).abs() < 1e-6);
        assert_eq!(handle.take_track_peak(0), 0.0);
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
        assert_eq!(handle.position_samples(), 20);
        assert!(!handle.is_playing());
    }

    #[test]
    fn rtz_and_locate_move_the_playhead() {
        let (mut handle, mut engine) = create(48_000);
        handle.send(Command::SetTrack(0, audio(vec![0.1; 1_000])));
        handle.play();
        let mut out = vec![0.0; 2 * 100];
        engine.render(&mut out, 2);
        handle.rtz();
        engine.render(&mut out[..0], 2);
        assert_eq!(handle.position_samples(), 0);
        handle.locate_seconds(0.01);
        engine.render(&mut out[..0], 2);
        assert_eq!(handle.position_samples(), 480);
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
