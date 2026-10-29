//! Recording: the input signal coming in, what is recorded, and how it leaves the audio thread.
//!
//! The platform's input callback pushes interleaved input frames into a lock-free ring; the
//! output callback ([`crate::Engine::render`]) takes one input frame for every output frame, so
//! input and tape run on the same clock. A small fill is kept in the ring so that input and output
//! callbacks of different sizes never run dry; when they drift apart (two devices with their own
//! clocks) the engine drops or repeats a frame to keep the fill bounded.
//!
//! Recorded samples leave through one ring per track, as a stream of [`RecItem`]s: a `Start`
//! with the tape position (already corrected for latency), the samples, and a `Stop`. A writer
//! on another thread ([`Recorder`]) turns them into takes.

use crate::TRACKS;

/// What the audio thread hands the writer for one track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecItem {
    /// A take starts at this tape position (in samples at the engine rate), with the tape
    /// running at this speed (1.0 normal, VARI otherwise; fixed for the take).
    Start(f64, f64),
    Sample(f32),
    Stop,
}

/// The input side, owned by the engine.
pub(crate) struct Input {
    pub(crate) rx: rtrb::Consumer<f32>,
    pub(crate) channels: usize,
    /// Frames to keep in the ring before reading, so callbacks of different sizes do not starve.
    pub(crate) target: usize,
    pub(crate) primed: bool,
    /// The current input frame; one value per input channel.
    pub(crate) frame: Vec<f32>,
}

impl Input {
    pub(crate) fn new(rx: rtrb::Consumer<f32>, channels: usize, target: usize) -> Self {
        Self { rx, channels: channels.max(1), target: target.max(1), primed: false, frame: vec![0.0; channels.max(1)] }
    }

    /// Read the next input frame into `self.frame` (silence while the ring fills up).
    pub(crate) fn next_frame(&mut self) {
        let available = self.rx.slots() / self.channels;
        if !self.primed {
            if available < self.target {
                self.frame.fill(0.0);
                return;
            }
            self.primed = true;
        }
        if available == 0 {
            // Ran dry: fill up again before reading on.
            self.primed = false;
            self.frame.fill(0.0);
            return;
        }
        // Far too far ahead (the input clock runs faster than the output's): skip a frame to keep
        // the delay bounded. The margin leaves room for large input callbacks.
        if available > 4 * self.target + 4096
            && let Ok(chunk) = self.rx.read_chunk(self.channels)
        {
            chunk.commit_all();
        }
        if let Ok(chunk) = self.rx.read_chunk(self.channels) {
            let (a, b) = chunk.as_slices();
            for (slot, value) in self.frame.iter_mut().zip(a.iter().chain(b.iter())) {
                *slot = *value;
            }
            chunk.commit_all();
        }
    }

    pub(crate) fn sample(&self, channel: usize) -> f32 {
        self.frame.get(channel).copied().unwrap_or(0.0)
    }
}

/// The input ring for the platform: create it, hand the producer to the input callback and the
/// consumer to [`crate::Engine::attach_input`].
pub fn input_ring(sample_rate: u32, channels: usize) -> (rtrb::Producer<f32>, rtrb::Consumer<f32>) {
    // A second of audio is far more than any callback needs.
    rtrb::RingBuffer::new(sample_rate.max(8_000) as usize * channels.max(1))
}

/// The writer's side: recorded audio coming out of the audio thread.
pub struct Recorder {
    pub(crate) tracks: Vec<rtrb::Consumer<RecItem>>,
}

impl Recorder {
    /// Take everything recorded on `track` since the last call.
    pub fn drain(&mut self, track: usize, mut each: impl FnMut(RecItem)) {
        let Some(rx) = self.tracks.get_mut(track) else { return };
        let available = rx.slots();
        if let Ok(chunk) = rx.read_chunk(available) {
            for item in chunk.into_iter() {
                each(item);
            }
        }
    }

    /// The engine is gone (device change): nothing more will arrive once drained.
    pub fn is_abandoned(&self) -> bool {
        self.tracks.iter().all(rtrb::Consumer::is_abandoned)
    }
}

/// The engine's side of [`Recorder`].
pub(crate) struct RecordOut {
    pub(crate) tracks: Vec<rtrb::Producer<RecItem>>,
}

pub(crate) fn recorder(sample_rate: u32) -> (RecordOut, Recorder) {
    // Two seconds per track give the writer plenty of time.
    let capacity = sample_rate.max(8_000) as usize * 2;
    let (tx, rx): (Vec<_>, Vec<_>) = (0..TRACKS).map(|_| rtrb::RingBuffer::new(capacity)).unzip();
    (RecordOut { tracks: tx }, Recorder { tracks: rx })
}

/// What the record logic is doing, as the REC LED shows it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RecMode {
    #[default]
    Off,
    /// Record-ready: REC was pressed, no armed track is recording yet (the LED blinks).
    Ready,
    /// At least one track is recording.
    Recording,
}
