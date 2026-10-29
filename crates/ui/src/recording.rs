//! Takes on their way to disk: a writer that turns the engine's recorded stream into temporary
//! files, and the job that saves a finished take in the project.

use daw388_engine::{RecItem, Recorder, TRACKS};
use daw388_project::MonoClip;
use daw388_project::peaks::Peaks;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;

/// A finished take: a mono 32-bit float file starting at tape position `start`.
#[derive(Debug, Clone, PartialEq)]
pub struct TakeDone {
    pub track: usize,
    /// Tape position of the first sample, in samples at `rate` (already latency-corrected).
    pub start: f64,
    pub path: PathBuf,
    pub frames: usize,
    pub rate: u32,
}

struct OpenTake {
    writer: hound::WavWriter<BufWriter<File>>,
    path: PathBuf,
    start: f64,
    frames: usize,
}

/// Writes what the engine records into one temporary file per take.
pub struct TakeWriter {
    dir: PathBuf,
    rate: u32,
    open: Vec<Option<OpenTake>>,
    counter: u64,
}

impl TakeWriter {
    pub fn new(dir: PathBuf, rate: u32) -> Self {
        Self { dir, rate, open: (0..TRACKS).map(|_| None).collect(), counter: 0 }
    }

    fn start(&mut self, track: usize, start: f64) {
        self.counter += 1;
        let path = self.dir.join(format!("take-{}-{}-{}.wav", std::process::id(), track + 1, self.counter));
        let spec = hound::WavSpec { channels: 1, sample_rate: self.rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let opened = std::fs::create_dir_all(&self.dir).map_err(hound::Error::IoError).and_then(|()| hound::WavWriter::create(&path, spec));
        match opened {
            Ok(writer) => self.open[track] = Some(OpenTake { writer, path, start, frames: 0 }),
            Err(e) => log::error!("cannot write take {}: {e}", path.display()),
        }
    }

    fn finish(&mut self, track: usize) -> Option<TakeDone> {
        let take = self.open.get_mut(track)?.take()?;
        if let Err(e) = take.writer.finalize() {
            log::error!("cannot finish take {}: {e}", take.path.display());
            return None;
        }
        Some(TakeDone { track, start: take.start, path: take.path, frames: take.frames, rate: self.rate })
    }

    /// Write everything recorded so far; returns the takes that ended.
    pub fn pump(&mut self, recorder: &mut Recorder) -> Vec<TakeDone> {
        let mut done = Vec::new();
        for track in 0..TRACKS {
            let mut items = Vec::new();
            recorder.drain(track, |item| items.push(item));
            for item in items {
                match item {
                    RecItem::Start(start) => {
                        // A take still open was cut short (it should not happen): keep it.
                        done.extend(self.finish(track));
                        self.start(track, start);
                    }
                    RecItem::Sample(value) => {
                        if let Some(take) = &mut self.open[track] {
                            if let Err(e) = take.writer.write_sample(value) {
                                log::error!("take {}: {e}", take.path.display());
                            }
                            take.frames += 1;
                        }
                    }
                    RecItem::Stop => done.extend(self.finish(track)),
                }
            }
        }
        done
    }

    /// End every open take (the engine went away mid-recording).
    pub fn finish_all(&mut self) -> Vec<TakeDone> {
        (0..TRACKS).filter_map(|track| self.finish(track)).collect()
    }
}

/// The writer loop for a background thread: runs until the engine is gone.
pub fn run_writer(mut recorder: Recorder, dir: PathBuf, rate: u32, tx: Sender<TakeDone>, ctx: egui::Context) {
    let mut writer = TakeWriter::new(dir, rate);
    loop {
        let gone = recorder.is_abandoned();
        let mut done = writer.pump(&mut recorder);
        if gone {
            done.extend(writer.finish_all());
        }
        if !done.is_empty() {
            for take in done {
                let _ = tx.send(take);
            }
            ctx.request_repaint();
        }
        if gone {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A take saved in the project: its file, and its audio for the source table.
pub struct StoredTake {
    pub path: PathBuf,
    pub clip: Arc<MonoClip>,
    /// The take at the engine's rate.
    pub at_rate: Arc<Vec<f32>>,
    /// The waveform overview, for drawing.
    pub peaks: Arc<Peaks>,
    pub engine_rate: u32,
}

fn read_take(path: &Path) -> Result<Vec<f32>, String> {
    let reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    reader.into_samples::<f32>().collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Save `take` as a 24-bit WAV at `dest`; the temporary take file is removed afterwards.
pub fn store_take(take: &TakeDone, dest: &Path, engine_rate: u32) -> Result<StoredTake, String> {
    let samples = read_take(&take.path)?;
    daw388_project::take::write_wav_24(dest, take.rate, &samples).map_err(|e| format!("{}: {e}", dest.display()))?;
    if let Err(e) = std::fs::remove_file(&take.path) {
        log::warn!("cannot remove {}: {e}", take.path.display());
    }
    let name = dest.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let clip = MonoClip { name, sample_rate: take.rate, samples };
    let at_rate = Arc::new(clip.at_rate(engine_rate));
    let peaks = Arc::new(Peaks::of(&clip.samples, clip.sample_rate));
    Ok(StoredTake { path: dest.to_path_buf(), clip: Arc::new(clip), at_rate, peaks, engine_rate })
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw388_engine::{ChannelParams, Command, Source, TrackAudio, input_ring};

    #[test]
    fn a_recorded_take_is_stored() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (mut handle, mut engine) = daw388_engine::create(48_000);
        let (mut tx, rx) = input_ring(48_000, 1);
        engine.attach_input(rx, 1, 1);
        let mut recorder = handle.take_recorder().unwrap_or_else(|| unreachable!());
        handle.send(Command::SetTrack(0, Some(Arc::new(TrackAudio { name: "old".into(), samples: vec![0.1; 100] }))));
        handle
            .send(Command::SetChannel(0, ChannelParams { source: Source::Line, input: Some(0), direct: true, ..ChannelParams::default() }));
        handle.send(Command::SetArmed(0, true));
        handle.send(Command::Locate(21.0));
        handle.send(Command::Record);
        for _ in 0..31 {
            let _ = tx.push(0.5);
        }
        let mut out = vec![0.0; 2 * 30];
        engine.render(&mut out, 2);
        handle.stop();
        engine.render(&mut [], 2);

        let mut writer = TakeWriter::new(dir.path().join("tmp"), 48_000);
        let done = writer.pump(&mut recorder);
        assert_eq!(done.len(), 1);
        let take = &done[0];
        assert_eq!((take.track, take.frames, take.start), (0, 30, 20.0));

        let dest = dir.path().join("01_old_take1.wav");
        let stored = store_take(take, &dest, 48_000).unwrap_or_else(|e| panic!("{e}"));
        assert!(!take.path.exists(), "the temporary file is removed");
        assert_eq!(stored.clip.samples.len(), 30);
        assert_eq!(stored.at_rate.len(), 30);
        assert_eq!(stored.clip.name, "01_old_take1");
        let reloaded = daw388_project::load_audio(&dest).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(reloaded.samples.len(), 30);
        assert!((reloaded.samples[0] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn an_engine_that_goes_away_mid_take_still_leaves_the_take() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (mut handle, mut engine) = daw388_engine::create(48_000);
        let (mut tx, rx) = input_ring(48_000, 1);
        engine.attach_input(rx, 1, 1);
        let mut recorder = handle.take_recorder().unwrap_or_else(|| unreachable!());
        handle
            .send(Command::SetChannel(0, ChannelParams { source: Source::Line, input: Some(0), direct: true, ..ChannelParams::default() }));
        handle.send(Command::SetArmed(0, true));
        handle.send(Command::Record);
        for _ in 0..11 {
            let _ = tx.push(0.5);
        }
        let mut out = vec![0.0; 2 * 10];
        engine.render(&mut out, 2);
        drop(engine);
        let mut writer = TakeWriter::new(dir.path().to_path_buf(), 48_000);
        assert!(writer.pump(&mut recorder).is_empty());
        assert!(recorder.is_abandoned());
        let done = writer.finish_all();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].frames, 10);
    }
}
