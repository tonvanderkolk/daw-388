//! Export: a second engine without a device renders the mix, faster than real time, on a
//! background thread. A stem "with mixer" is the mix with that track on SOLO; "tape only" runs
//! the track through a neutral mixer; "dry" is the recording itself.

use daw388_engine::{ChannelParams, Command, Meter, MixerParams, MonitorParams, Source, TRACKS, TapeParams, TrackAudio};
use daw388_project::MonoClip;
use daw388_project::export::{BitDepth, ExportFormat, export_name, resample_hq, unused_path, write_audio};
use daw388_project::regions::{Region, SourceId};
use daw388_project::render::{RateSources, render_track};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// How a single track is exported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum StemKind {
    /// Stereo, as in the mix: EQ, fader, pan and its own reverb and echo.
    #[default]
    Mixed,
    /// Mono, through the tape only.
    TapeOnly,
    /// Mono, the recording itself.
    Dry,
}

/// One track for an export: its name (for the stem's file name) and its regions.
#[derive(Clone, Default)]
pub(crate) struct ExportTrack {
    pub name: String,
    pub regions: Vec<Region>,
}

/// Everything an export needs, copied from the app so it can run on its own thread.
#[derive(Clone)]
pub(crate) struct ExportJob {
    pub rate: u32,
    pub tracks: [ExportTrack; TRACKS],
    /// The sources the regions use, at their own rate.
    pub sources: HashMap<SourceId, Arc<MonoClip>>,
    pub channels: [ChannelParams; TRACKS],
    pub monitors: [MonitorParams; TRACKS],
    pub mixer: MixerParams,
    pub master: f32,
    pub tape: TapeParams,
    pub meter: Meter,
    /// Start and end on the tape, in seconds.
    pub range: (f64, f64),
    /// The stereo bounce, with (true) or without the tape's character.
    pub stereo: Option<bool>,
    pub stems: Vec<(usize, StemKind)>,
    /// Files start at the range's start; otherwise at the start of the tape.
    pub from_zero: bool,
    pub dir: PathBuf,
    pub project: String,
    pub format: ExportFormat,
    pub depth: BitDepth,
}

/// Shared with the UI while an export runs.
#[derive(Debug, Default)]
pub(crate) struct Progress {
    /// Per mille.
    pub done: AtomicU32,
    pub cancel: AtomicBool,
}

/// A pre-roll while stopped, so faders, sends and the echo's TIME have settled before the start.
const PREROLL_S: f64 = 3.0;
/// After the end the effects ring out until this quiet for half a second, at most `TAIL_MAX_S`.
const TAIL_QUIET: f32 = 3.162_277_7e-5; // -90 dBFS
const TAIL_MAX_S: f64 = 10.0;
const BLOCK: usize = 4_096;

struct Renderer<'a> {
    job: &'a ExportJob,
    audio: [Option<Arc<TrackAudio>>; TRACKS],
    progress: &'a Progress,
    /// Frames done and to do over all renders, for the progress bar.
    done: u64,
    total: u64,
}

impl Renderer<'_> {
    fn frames(&self) -> usize {
        ((self.job.range.1 - self.job.range.0).max(0.0) * f64::from(self.job.rate)) as usize
    }

    fn tick(&mut self, frames: usize) -> Result<(), String> {
        self.done += frames as u64;
        let per_mille = (self.done * 1000 / self.total.max(1)).min(1000) as u32;
        self.progress.done.store(per_mille, Ordering::Relaxed);
        if self.progress.cancel.load(Ordering::Relaxed) { Err("Export geannuleerd".into()) } else { Ok(()) }
    }

    /// The mix as the app plays it, with the tape `on` or not, and `solo` on one track. With
    /// `neutral`, every channel is a plain RMX channel hard left on L-R: the tape alone.
    fn render(&mut self, solo: Option<usize>, tape_on: bool, neutral: bool) -> Result<[Vec<f32>; 2], String> {
        let job = self.job;
        let (mut handle, mut engine) = daw388_engine::create(job.rate);
        for (index, audio) in self.audio.iter().enumerate() {
            handle.send(Command::SetTrack(index, audio.clone()));
            let channel = if neutral {
                ChannelParams { source: Source::Remix, lr: true, pan: -1.0, ..ChannelParams::default() }
            } else {
                job.channels[index]
            };
            handle.send(Command::SetChannel(index, channel));
            handle.send(Command::SetMonitor(index, if neutral { MonitorParams::default() } else { job.monitors[index] }));
            handle.send(Command::SetSolo(index, solo == Some(index)));
        }
        handle.send(Command::SetMixer(if neutral { MixerParams::default() } else { job.mixer }));
        handle.send(Command::SetMaster(if neutral { 1.0 } else { job.master }));
        handle.send(Command::SetTape(TapeParams { on: tape_on, ..job.tape }));
        handle.send(Command::SetMeter(job.meter));
        handle.locate_seconds(job.range.0);
        let mut block = vec![0.0f32; 2 * BLOCK];
        let preroll = (PREROLL_S * f64::from(job.rate)) as usize;
        for _ in 0..preroll.div_ceil(BLOCK) {
            engine.render(&mut block, 2);
        }
        handle.play();
        let mut out = [Vec::new(), Vec::new()];
        let mut left = self.frames();
        while left > 0 {
            let n = left.min(BLOCK);
            engine.render(&mut block[..2 * n], 2);
            for frame in block[..2 * n].chunks(2) {
                out[0].push(frame[0]);
                out[1].push(frame[1]);
            }
            left -= n;
            self.tick(n)?;
        }
        // The tape stops; reverb and echo ring out.
        handle.stop();
        let (mut quiet, max) = (0usize, (TAIL_MAX_S * f64::from(job.rate)) as usize);
        let mut tail = 0usize;
        while tail < max && quiet < job.rate as usize / 2 {
            engine.render(&mut block, 2);
            for frame in block.chunks(2) {
                out[0].push(frame[0]);
                out[1].push(frame[1]);
                quiet = if frame[0].abs().max(frame[1].abs()) < TAIL_QUIET { quiet + 1 } else { 0 };
            }
            tail += BLOCK;
        }
        // Cut the silence at the end, but keep the half second that told it was quiet short.
        let keep = out[0].len().saturating_sub(quiet.saturating_sub(job.rate as usize / 20));
        out[0].truncate(keep);
        out[1].truncate(keep);
        Ok(out)
    }

    /// The recording itself over the range, at the export's rate.
    fn dry(&mut self, index: usize) -> Result<Vec<f32>, String> {
        let start = (self.job.range.0 * f64::from(self.job.rate)) as usize;
        let frames = self.frames();
        let samples = self.audio[index].as_ref().map(|a| a.samples.as_slice()).unwrap_or_default();
        let mut out: Vec<f32> = samples.iter().skip(start).take(frames).copied().collect();
        out.resize(frames, 0.0);
        self.tick(frames)?;
        Ok(out)
    }

    /// Silence before the range's start when the files should line up with the tape.
    fn place(&self, mut channels: Vec<Vec<f32>>) -> Vec<Vec<f32>> {
        if !self.job.from_zero {
            let lead = (self.job.range.0 * f64::from(self.job.rate)) as usize;
            for channel in &mut channels {
                channel.splice(0..0, std::iter::repeat_n(0.0, lead));
            }
        }
        channels
    }
}

/// Every track with regions as one buffer at the export's rate, the way the app renders them.
fn render_tracks(job: &ExportJob) -> [Option<Arc<TrackAudio>>; TRACKS] {
    let used = |id: &SourceId| job.tracks.iter().any(|t| t.regions.iter().any(|r| r.source == *id));
    let sources: RateSources = job
        .sources
        .iter()
        .filter(|(id, _)| used(id))
        .map(|(id, clip)| (*id, Arc::new(resample_hq(&clip.samples, clip.sample_rate, job.rate))))
        .collect();
    std::array::from_fn(|i| {
        let track = &job.tracks[i];
        (!track.regions.is_empty())
            .then(|| Arc::new(TrackAudio { name: track.name.clone(), samples: render_track(&track.regions, &sources, job.rate) }))
    })
}

/// Run an export; returns the files written. On an error or a cancel, nothing is left behind.
pub(crate) fn run(job: &ExportJob, progress: &Progress) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(&job.dir).map_err(|e| format!("Map {} maken mislukt: {e}", job.dir.display()))?;
    let audio = render_tracks(job);
    let mut renderer = Renderer { job, audio, progress, done: 0, total: 0 };
    let renders = job.stereo.iter().count() + job.stems.len();
    renderer.total = (renders * renderer.frames()) as u64;
    let mut written = Vec::new();
    let result = (|| {
        let save = |name: String, channels: Vec<Vec<f32>>, written: &mut Vec<PathBuf>| {
            let path = unused_path(&job.dir, &name, job.format.extension());
            write_audio(&path, &channels, job.rate, job.format, job.depth)
                .map_err(|e| format!("{} schrijven mislukt: {e}", path.display()))?;
            written.push(path);
            Ok::<(), String>(())
        };
        if let Some(tape) = job.stereo {
            let [l, r] = renderer.render(None, tape && job.tape.on, false)?;
            let channels = renderer.place(vec![l, r]);
            save(export_name(&job.project, None, None), channels, &mut written)?;
        }
        for &(index, kind) in &job.stems {
            let channels = match kind {
                StemKind::Mixed => {
                    let [l, r] = renderer.render(Some(index), job.tape.on, false)?;
                    vec![l, r]
                }
                StemKind::TapeOnly => vec![renderer.render(Some(index), job.tape.on, true)?[0].clone()],
                StemKind::Dry => vec![renderer.dry(index)?],
            };
            let name = &job.tracks[index].name;
            save(
                export_name(&job.project, Some(index), (!name.is_empty()).then_some(name.as_str())),
                renderer.place(channels),
                &mut written,
            )?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            progress.done.store(1000, Ordering::Relaxed);
            Ok(written)
        }
        Err(e) => {
            for path in &written {
                let _ = std::fs::remove_file(path);
            }
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw388_engine::{EchoParams, EchoSync, RtnParams};

    const RATE: u32 = 48_000;

    /// Put `samples` on track `index` as one region from 0:00, with the track's index as source.
    fn put(job: &mut ExportJob, index: usize, name: &str, samples: Vec<f32>) {
        let clip = MonoClip { name: name.into(), sample_rate: RATE, samples };
        let region = Region { source: index as SourceId, start: 0.0, offset: 0.0, length: clip.duration_seconds(), name: name.into() };
        job.tracks[index] = ExportTrack { name: name.into(), regions: vec![region] };
        job.sources.insert(index as SourceId, Arc::new(clip));
    }

    fn job(dir: PathBuf) -> ExportJob {
        let mut job = empty_job(dir);
        put(&mut job, 0, "Kick", (0..24_000).map(|i| if i % 4_800 == 0 { 0.8 } else { 0.0 }).collect());
        put(&mut job, 1, "Bas", (0..24_000).map(|i| 0.3 * (i as f32 * 0.01).sin()).collect());
        job
    }

    fn empty_job(dir: PathBuf) -> ExportJob {
        let channels = std::array::from_fn(|i| ChannelParams {
            lr: true,
            pan: if i == 0 { -0.5 } else { 0.3 },
            aux: 0.5,
            eff: 0.5,
            ..ChannelParams::default()
        });
        let rtn = [RtnParams { gain: 1.0, pan: 0.0 }; 2];
        let echo = EchoParams { time_ms: 100.0, feedback: 0.0, tone: 1.0, sync: EchoSync::Off };
        ExportJob {
            rate: RATE,
            tracks: Default::default(),
            sources: HashMap::new(),
            channels,
            monitors: [MonitorParams::default(); TRACKS],
            mixer: MixerParams { rtn, echo, ..MixerParams::default() },
            master: 1.0,
            tape: TapeParams::default(),
            meter: Meter::default(),
            range: (0.0, 0.5),
            stereo: Some(false),
            stems: Vec::new(),
            from_zero: true,
            dir,
            project: "Demo".into(),
            format: ExportFormat::Wav,
            depth: BitDepth::Float32,
        }
    }

    fn renderer<'a>(job: &'a ExportJob, progress: &'a Progress) -> Renderer<'a> {
        Renderer { job, audio: render_tracks(job), progress, done: 0, total: 1 }
    }

    #[test]
    fn the_sum_of_the_mixed_stems_is_the_bounce() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let job = job(dir.path().to_path_buf());
        let progress = Progress::default();
        let mut r = renderer(&job, &progress);
        let bounce = r.render(None, false, false).unwrap_or_else(|e| panic!("{e}"));
        let kick = r.render(Some(0), false, false).unwrap_or_else(|e| panic!("{e}"));
        let bass = r.render(Some(1), false, false).unwrap_or_else(|e| panic!("{e}"));
        let n = bounce[0].len().min(kick[0].len()).min(bass[0].len());
        assert!(n > 24_000, "the mix and its tail: {n}");
        for side in 0..2 {
            for i in 0..n {
                let sum = kick[side][i] + bass[side][i];
                assert!((sum - bounce[side][i]).abs() < 1e-4, "{side} {i}: {sum} {}", bounce[side][i]);
            }
        }
        assert!(bounce[0].iter().skip(24_000).any(|s| s.abs() > 1e-4), "reverb and echo ring out after the end");
    }

    #[test]
    fn a_dry_stem_is_the_recording_and_files_are_named_and_complete() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let job = ExportJob { stems: vec![(0, StemKind::Dry), (1, StemKind::Mixed)], ..job(dir.path().join("export")) };
        let files = run(&job, &Progress::default()).unwrap_or_else(|e| panic!("{e}"));
        let names: Vec<String> = files.iter().filter_map(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["Demo_mix.wav", "Demo_01_Kick.wav", "Demo_02_Bas.wav"]);
        let dry = daw388_project::load_audio(&files[1]).unwrap_or_else(|e| panic!("{e}"));
        let original = job.sources[&0].samples.clone();
        // Bit for bit, apart from the 3 ms fades at the edges (144 samples at 48 kHz).
        assert_eq!(dry.samples.len(), original.len());
        assert_eq!(dry.samples[144..original.len() - 144], original[144..original.len() - 144]);
        assert!(dry.samples[0] < original[0], "the first sample is faded in");
    }

    #[test]
    fn a_stem_follows_the_regions() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let mut job =
            ExportJob { range: (0.0, 0.75), stereo: None, stems: vec![(0, StemKind::Dry)], ..empty_job(dir.path().to_path_buf()) };
        put(&mut job, 0, "Gitaar", vec![0.5; 48_000]);
        let region = |start: f64| Region { source: 0, start, offset: 0.0, length: 0.25, name: "Gitaar".into() };
        job.tracks[0].regions = vec![region(0.0), region(0.5)];
        let progress = Progress::default();
        let dry = renderer(&job, &progress).dry(0).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(dry.len(), 36_000);
        assert!(dry[12_500..23_500].iter().all(|s| *s == 0.0));
        assert_eq!((dry[6_000], dry[30_000]), (0.5, 0.5));
    }

    #[test]
    fn zero_to_cue_gives_that_stretch_and_lines_up_on_request() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let base = ExportJob { range: (0.25, 0.4), stereo: None, stems: vec![(0, StemKind::Dry)], ..job(dir.path().to_path_buf()) };
        let progress = Progress::default();
        let mut r = renderer(&base, &progress);
        assert_eq!(r.dry(0).map(|d| d.len()), Ok(7_200), "0.15 s");
        let lined_up = ExportJob { from_zero: false, ..base.clone() };
        let r = renderer(&lined_up, &progress);
        assert_eq!(r.place(vec![vec![1.0; 10]])[0].len(), 12_010, "silence for the first 0.25 s");
    }

    #[test]
    fn a_cancelled_export_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let job = ExportJob { stems: vec![(0, StemKind::Mixed)], ..job(dir.path().to_path_buf()) };
        let progress = Progress::default();
        progress.cancel.store(true, Ordering::Relaxed);
        assert!(run(&job, &progress).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).map(|d| d.count()).unwrap_or(1), 0);
    }
}
