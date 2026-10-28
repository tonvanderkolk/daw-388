//! The project folder: `project.json` with the mix settings, `audio/` with the WAV files the
//! tracks play and `backups/` with earlier versions of `project.json`.
//!
//! Saving copies every track's audio into `audio/` (unless it is already there), so a project
//! folder can be moved, backed up or shared as a whole.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const PROJECT_FILE: &str = "project.json";
pub const AUDIO_DIR: &str = "audio";
pub const BACKUP_DIR: &str = "backups";
/// Number of backups kept; older ones are removed.
pub const MAX_BACKUPS: usize = 10;
/// An automatic save makes a backup only when the newest one is older than this.
pub const BACKUP_INTERVAL: Duration = Duration::from_secs(5 * 60);

const FORMAT: &str = "daw388-project";
const FORMAT_VERSION: u32 = 2;

/// Contents of `project.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectFile {
    pub format: String,
    pub version: u32,
    /// Version of DAW 388 that wrote the file.
    pub app_version: String,
    #[serde(default)]
    pub master_db: f32,
    /// Track meters show the level after the fader instead of the track's own level.
    #[serde(default)]
    pub meter_post: bool,
    #[serde(default)]
    pub transport: TransportSettings,
    #[serde(default)]
    pub metronome: MetronomeSettings,
    #[serde(default)]
    pub mixer: MixerEntry,
    #[serde(default)]
    pub tracks: Vec<TrackEntry>,
}

/// One track in `project.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackEntry {
    /// Track number, 1 to 8.
    pub number: u8,
    /// Audio file, relative to the project folder (for example `audio/01_Kick.wav`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default)]
    pub fader_db: f32,
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub mute: bool,
    /// The input channel this track records from (counted from 1, as interfaces label them).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<u16>,
    #[serde(default)]
    pub channel: ChannelEntry,
    #[serde(default)]
    pub monitor: MonitorEntry,
}

/// Where a channel's signal comes from: the microphone preamp, the line input or the tape (RMX,
/// the 388's remix position).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelSource {
    Mic,
    Line,
    #[default]
    Rmx,
}

/// A mixer channel in `project.json` (the fader, pan, mute and input stay on [`TrackEntry`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelEntry {
    pub source: ChannelSource,
    pub trim_db: f32,
    /// ASSIGN 1-2, 3-4, 5-6 and 7-8.
    pub assign: [bool; 4],
    /// ASSIGN L-R: to the stereo bus.
    pub lr: bool,
    /// The track records this channel after TRIM instead of its PGM bus.
    pub direct: bool,
}

/// A track's MONITOR controls in `project.json`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MonitorEntry {
    pub gain_db: f32,
    pub pan: f32,
    pub mute: bool,
}

/// The MON switch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MonSwitch {
    Pgm,
    #[default]
    Cue,
}

/// The master section in `project.json` (the STEREO MASTER is `master_db`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MixerEntry {
    /// PGM MASTERS ODD and EVEN.
    pub odd_db: f32,
    pub even_db: f32,
    pub mon: MonSwitch,
    pub mono: bool,
}

/// How REPEAT (and RTZ and STC) reach their point.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepeatMode {
    /// Wind back through the tape in time, as the 388 does.
    #[default]
    Tape,
    /// Jump back at once, as Logic's cycle mode does.
    Cycle,
}

/// The tape transport's memory and settings in `project.json`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct TransportSettings {
    /// The zero point set with RESET, in seconds from the start of the tape.
    #[serde(default)]
    pub zero_seconds: f64,
    /// The CUE point, in seconds from the start of the tape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cue_seconds: Option<f64>,
    #[serde(default)]
    pub repeat_mode: RepeatMode,
    /// Pitch control setting in percent (-15 to +15).
    #[serde(default)]
    pub pitch_percent: f32,
    /// VARI (true): the pitch control works; FIX (false): normal speed.
    #[serde(default)]
    pub vari: bool,
    /// INSERT: armed tracks play the tape until the punch-in.
    #[serde(default)]
    pub insert: bool,
    /// AUTO PUNCH: record between the zero point and the CUE point.
    #[serde(default)]
    pub auto_punch: bool,
}

/// The metronome in `project.json`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetronomeSettings {
    /// CLICK on.
    pub click: bool,
    /// Click during plain playback too, not only while recording.
    pub always: bool,
    /// Quarter notes per minute.
    pub bpm: f64,
    /// Time signature: beats per bar and the note value of a beat (4 or 8).
    pub beats: u8,
    pub unit: u8,
    /// Click level in dB.
    pub volume_db: f32,
    /// Bars to count in before recording.
    pub count_in: u8,
}

impl Default for MetronomeSettings {
    fn default() -> Self {
        Self { click: false, always: false, bpm: 120.0, beats: 4, unit: 4, volume_db: -6.0, count_in: 0 }
    }
}

/// Project-wide settings saved with the tracks.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProjectSettings {
    pub master_db: f32,
    pub meter_post: bool,
    pub transport: TransportSettings,
    pub metronome: MetronomeSettings,
    pub mixer: MixerEntry,
}

/// What the UI hands over for one track when saving.
#[derive(Debug, Clone, Default)]
pub struct TrackToSave {
    /// Where the track's audio is now (anywhere on disk), if it has any.
    pub source: Option<PathBuf>,
    pub fader_db: f32,
    pub pan: f32,
    pub mute: bool,
    /// Input channel, counted from 1.
    pub input: Option<u16>,
    pub channel: ChannelEntry,
    pub monitor: MonitorEntry,
}

/// The result of a save.
#[derive(Debug, Clone)]
pub struct Saved {
    /// Per track, where its audio lives now (inside the project's `audio/` folder).
    pub sources: Vec<Option<PathBuf>>,
    /// The backup made of the previous `project.json`, if any.
    pub backup: Option<PathBuf>,
}

/// A project read from disk.
#[derive(Debug, Clone)]
pub struct OpenedProject {
    pub dir: PathBuf,
    pub file: ProjectFile,
}

impl OpenedProject {
    /// Absolute path of a track's audio file, if the track has one.
    pub fn track_source(&self, entry: &TrackEntry) -> Option<PathBuf> {
        entry.file.as_ref().map(|f| self.dir.join(f))
    }
}

#[derive(Debug)]
pub enum ProjectError {
    Io { action: String, path: PathBuf, error: std::io::Error },
    Json(serde_json::Error),
    NotAProject(PathBuf),
    TooNew(u32),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { action, path, error } => write!(f, "{action} {} mislukt: {error}", path.display()),
            Self::Json(e) => write!(f, "project.json is niet leesbaar: {e}"),
            Self::NotAProject(path) => write!(f, "{} is geen DAW 388-project", path.display()),
            Self::TooNew(v) => write!(f, "dit project is gemaakt met een nieuwere versie van DAW 388 (formaat {v})"),
        }
    }
}

impl std::error::Error for ProjectError {}

fn io(action: &str, path: &Path) -> impl FnOnce(std::io::Error) -> ProjectError {
    let (action, path) = (action.to_string(), path.to_path_buf());
    move |error| ProjectError::Io { action, path, error }
}

/// The project's name: the name of its folder.
pub fn project_name(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Naamloos".into())
}

/// Read a project. `path` may be the project folder or its `project.json`.
pub fn open(path: &Path) -> Result<OpenedProject, ProjectError> {
    let (dir, file) = if path.is_dir() {
        (path.to_path_buf(), path.join(PROJECT_FILE))
    } else {
        (path.parent().map(Path::to_path_buf).unwrap_or_default(), path.to_path_buf())
    };
    let text = fs::read_to_string(&file).map_err(io("Lezen van", &file))?;
    let mut project: ProjectFile = serde_json::from_str(&text).map_err(ProjectError::Json)?;
    if project.format != FORMAT {
        return Err(ProjectError::NotAProject(file));
    }
    if project.version > FORMAT_VERSION {
        return Err(ProjectError::TooNew(project.version));
    }
    if project.version < 2 {
        upgrade_v1(&mut project);
    }
    Ok(OpenedProject { dir, file: project })
}

/// Format 1 had one fader, pan and mute per track and recorded straight from the input. In
/// format 2 these become the MONITOR controls, so the project sounds as before; the channel
/// starts neutral, and a track that had an input records it through DIRECT on a LINE channel.
fn upgrade_v1(project: &mut ProjectFile) {
    for track in &mut project.tracks {
        track.monitor = MonitorEntry { gain_db: track.fader_db, pan: track.pan, mute: track.mute };
        track.fader_db = 0.0;
        track.pan = 0.0;
        track.mute = false;
        track.channel = if track.input.is_some() {
            ChannelEntry { source: ChannelSource::Line, direct: true, ..ChannelEntry::default() }
        } else {
            ChannelEntry::default()
        };
    }
    project.version = FORMAT_VERSION;
}

/// Save a project into `dir` (created when needed). Audio from outside the project is copied
/// into `audio/`. The previous `project.json` goes to `backups/` first: always when `backup` is
/// [`Backup::Always`], else only when the newest backup is older than [`BACKUP_INTERVAL`].
pub fn save(dir: &Path, settings: &ProjectSettings, tracks: &[TrackToSave], backup: Backup) -> Result<Saved, ProjectError> {
    let audio_dir = dir.join(AUDIO_DIR);
    fs::create_dir_all(&audio_dir).map_err(io("Aanmaken van", &audio_dir))?;

    let mut entries = Vec::with_capacity(tracks.len());
    let mut sources = Vec::with_capacity(tracks.len());
    for (index, track) in tracks.iter().enumerate() {
        let number = u8::try_from(index + 1).unwrap_or(u8::MAX);
        let source = match &track.source {
            Some(source) => Some(place_audio(&audio_dir, source)?),
            None => None,
        };
        let file = source.as_ref().and_then(|s| relative(dir, s));
        entries.push(TrackEntry {
            number,
            file,
            fader_db: track.fader_db,
            pan: track.pan,
            mute: track.mute,
            input: track.input,
            channel: track.channel,
            monitor: track.monitor,
        });
        sources.push(source);
    }

    let project = ProjectFile {
        format: FORMAT.into(),
        version: FORMAT_VERSION,
        app_version: env!("CARGO_PKG_VERSION").into(),
        master_db: settings.master_db,
        meter_post: settings.meter_post,
        transport: settings.transport,
        metronome: settings.metronome,
        mixer: settings.mixer,
        tracks: entries,
    };
    let path = dir.join(PROJECT_FILE);
    let backup = if path.exists() { make_backup(dir, &path, backup)? } else { None };
    let json = serde_json::to_string_pretty(&project).map_err(ProjectError::Json)?;
    // Write next to the target and rename, so a crash never leaves a half-written project.json.
    let temporary = dir.join(format!("{PROJECT_FILE}.tmp"));
    fs::write(&temporary, json).map_err(io("Schrijven van", &temporary))?;
    fs::rename(&temporary, &path).map_err(io("Schrijven van", &path))?;
    Ok(Saved { sources, backup })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backup {
    /// A save the user asked for.
    Always,
    /// An automatic save.
    WhenDue,
}

/// Make sure `source` is inside `audio/` and return where it is. A file that is already there
/// stays; one from elsewhere is copied under a name that is not taken yet. A file that no longer
/// exists keeps its path, so a missing file is still referenced after saving.
fn place_audio(audio_dir: &Path, source: &Path) -> Result<PathBuf, ProjectError> {
    if !source.exists() || is_inside(audio_dir, source) {
        return Ok(source.to_path_buf());
    }
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "audio".into());
    let extension = source.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "wav".into());
    let mut target = audio_dir.join(format!("{stem}.{extension}"));
    let mut n = 2;
    while target.exists() {
        target = audio_dir.join(format!("{stem}-{n}.{extension}"));
        n += 1;
    }
    fs::copy(source, &target).map_err(io("Kopiëren naar", &target))?;
    Ok(target)
}

fn is_inside(dir: &Path, path: &Path) -> bool {
    match (dir.canonicalize(), path.canonicalize()) {
        (Ok(dir), Ok(path)) => path.starts_with(dir),
        _ => false,
    }
}

/// `path` relative to `dir`, with forward slashes so the file is the same on every platform.
/// Paths outside the project are stored as they are.
fn relative(dir: &Path, path: &Path) -> Option<String> {
    let join = |rel: &Path| rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
    // Compare canonical paths when both exist (macOS puts temp folders behind a symlink), else as given.
    let canonical = dir.canonicalize().ok().zip(path.canonicalize().ok());
    let rel = canonical.as_ref().and_then(|(d, p)| p.strip_prefix(d).ok()).or_else(|| path.strip_prefix(dir).ok());
    Some(rel.map_or_else(|| path.to_string_lossy().into_owned(), join))
}

fn make_backup(dir: &Path, current: &Path, policy: Backup) -> Result<Option<PathBuf>, ProjectError> {
    let backups = dir.join(BACKUP_DIR);
    let existing = list_backups(&backups);
    if policy == Backup::WhenDue
        && let Some(newest) = existing.last()
        && fs::metadata(newest)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < BACKUP_INTERVAL)
    {
        return Ok(None);
    }
    fs::create_dir_all(&backups).map_err(io("Aanmaken van", &backups))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut target = backups.join(format!("project-{stamp}.json"));
    let mut n = 2;
    while target.exists() {
        target = backups.join(format!("project-{stamp}-{n}.json"));
        n += 1;
    }
    fs::copy(current, &target).map_err(io("Backup maken naar", &target))?;
    let mut all = list_backups(&backups);
    while all.len() > MAX_BACKUPS {
        let oldest = all.remove(0);
        let _ = fs::remove_file(oldest);
    }
    Ok(Some(target))
}

/// Backups oldest first (their names sort by time).
fn list_backups(backups: &Path) -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = fs::read_dir(backups)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("project-")))
                .collect()
        })
        .unwrap_or_default();
    list.sort();
    list
}

/// The local time as `HH:MM`, for "Opgeslagen 17:06".
pub fn clock_time() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_wav(path: &Path) {
        let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut writer = hound::WavWriter::create(path, spec).unwrap_or_else(|e| panic!("{e}"));
        for n in 0..100 {
            writer.write_sample(n as i16).unwrap_or_else(|e| panic!("{e}"));
        }
        writer.finalize().unwrap_or_else(|e| panic!("{e}"));
    }

    fn tracks(sources: [Option<PathBuf>; 2]) -> Vec<TrackToSave> {
        sources
            .into_iter()
            .enumerate()
            .map(|(i, source)| TrackToSave {
                source,
                fader_db: -3.0 * i as f32,
                pan: 0.5,
                mute: i == 1,
                input: Some(i as u16 + 1),
                channel: ChannelEntry {
                    source: ChannelSource::Mic,
                    trim_db: 12.0,
                    assign: [false, i == 1, false, false],
                    lr: i == 0,
                    direct: i == 1,
                },
                monitor: MonitorEntry { gain_db: -4.0, pan: 0.25, mute: i == 0 },
            })
            .collect()
    }

    #[test]
    fn save_copies_audio_and_open_reads_it_back() {
        let outside = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let kick = outside.path().join("01_Kick.wav");
        write_wav(&kick);
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("Mijn liedje");

        let saved = save(
            &dir,
            &ProjectSettings {
                master_db: -1.5,
                meter_post: true,
                transport: TransportSettings {
                    zero_seconds: 1.25,
                    cue_seconds: Some(30.5),
                    repeat_mode: RepeatMode::Cycle,
                    pitch_percent: -2.5,
                    vari: true,
                    insert: true,
                    auto_punch: true,
                },
                metronome: MetronomeSettings { click: true, always: true, bpm: 96.5, beats: 6, unit: 8, volume_db: -12.0, count_in: 2 },
                mixer: MixerEntry { odd_db: -2.0, even_db: 1.0, mon: MonSwitch::Pgm, mono: true },
            },
            &tracks([Some(kick.clone()), None]),
            Backup::Always,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let copied = saved.sources[0].clone().unwrap_or_else(|| panic!("no source"));
        assert!(copied.starts_with(dir.join(AUDIO_DIR)));
        assert!(copied.exists() && kick.exists(), "the original stays where it was");
        assert!(saved.backup.is_none(), "nothing to back up on the first save");

        let opened = open(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(opened.file.master_db, -1.5);
        assert!(opened.file.meter_post);
        let transport = opened.file.transport;
        assert_eq!(transport.zero_seconds, 1.25);
        assert_eq!(transport.cue_seconds, Some(30.5));
        assert_eq!(transport.repeat_mode, RepeatMode::Cycle);
        assert_eq!(transport.pitch_percent, -2.5);
        assert!(transport.vari && transport.insert && transport.auto_punch);
        assert_eq!(
            opened.file.metronome,
            MetronomeSettings { click: true, always: true, bpm: 96.5, beats: 6, unit: 8, volume_db: -12.0, count_in: 2 }
        );
        assert_eq!(opened.file.tracks.len(), 2);
        assert_eq!(opened.file.tracks[0].file.as_deref(), Some("audio/01_Kick.wav"));
        assert_eq!(opened.file.tracks[1].file, None);
        assert!(opened.file.tracks[1].mute);
        assert_eq!(opened.file.tracks[1].fader_db, -3.0);
        assert_eq!(opened.file.tracks[1].input, Some(2));
        assert_eq!(
            opened.file.tracks[1].channel,
            ChannelEntry { source: ChannelSource::Mic, trim_db: 12.0, assign: [false, true, false, false], lr: false, direct: true }
        );
        assert_eq!(opened.file.tracks[0].monitor, MonitorEntry { gain_db: -4.0, pan: 0.25, mute: true });
        assert_eq!(opened.file.mixer, MixerEntry { odd_db: -2.0, even_db: 1.0, mon: MonSwitch::Pgm, mono: true });
        assert_eq!(opened.file.version, FORMAT_VERSION, "a format 2 file is not upgraded again");
        assert!(crate::load_wav(&opened.track_source(&opened.file.tracks[0]).unwrap_or_default()).is_ok());
        assert_eq!(project_name(&dir), "Mijn liedje");

        // Opening via project.json works too.
        assert!(open(&dir.join(PROJECT_FILE)).is_ok());
    }

    #[test]
    fn saving_again_does_not_copy_twice_and_name_clashes_get_a_suffix() {
        let outside = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let first = outside.path().join("a/take.wav");
        let second = outside.path().join("b/take.wav");
        for p in [&first, &second] {
            fs::create_dir_all(p.parent().unwrap_or(Path::new("."))).unwrap_or_else(|e| panic!("{e}"));
            write_wav(p);
        }
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("p");
        let saved =
            save(&dir, &ProjectSettings::default(), &tracks([Some(first), Some(second)]), Backup::Always).unwrap_or_else(|e| panic!("{e}"));
        let opened = open(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(opened.file.tracks[0].file.as_deref(), Some("audio/take.wav"));
        assert_eq!(opened.file.tracks[1].file.as_deref(), Some("audio/take-2.wav"));

        let again = save(&dir, &ProjectSettings::default(), &tracks([saved.sources[0].clone(), saved.sources[1].clone()]), Backup::Always)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(again.sources, saved.sources);
        assert_eq!(fs::read_dir(dir.join(AUDIO_DIR)).map(Iterator::count).unwrap_or(0), 2);
    }

    #[test]
    fn backups_are_made_and_pruned() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("p");
        let empty = tracks([None, None]);
        save(&dir, &ProjectSettings::default(), &empty, Backup::Always).unwrap_or_else(|e| panic!("{e}"));
        let second = save(&dir, &ProjectSettings::default(), &empty, Backup::Always).unwrap_or_else(|e| panic!("{e}"));
        assert!(second.backup.is_some());
        // An automatic save right after a backup does not make another one.
        let auto = save(&dir, &ProjectSettings::default(), &empty, Backup::WhenDue).unwrap_or_else(|e| panic!("{e}"));
        assert!(auto.backup.is_none());
        for _ in 0..MAX_BACKUPS + 5 {
            save(&dir, &ProjectSettings::default(), &empty, Backup::Always).unwrap_or_else(|e| panic!("{e}"));
        }
        assert_eq!(list_backups(&dir.join(BACKUP_DIR)).len(), MAX_BACKUPS);
    }

    #[test]
    fn missing_audio_stays_referenced() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("p");
        let gone = dir.join(AUDIO_DIR).join("weg.wav");
        save(&dir, &ProjectSettings::default(), &tracks([Some(gone), None]), Backup::Always).unwrap_or_else(|e| panic!("{e}"));
        let opened = open(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(opened.file.tracks[0].file.as_deref(), Some("audio/weg.wav"));
    }

    #[test]
    fn a_format_1_project_is_upgraded_and_sounds_the_same() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = root.path().join(PROJECT_FILE);
        let v1 = r#"{"format":"daw388-project","version":1,"app_version":"0.7.0","tracks":[
            {"number":1,"fader_db":-6.0,"pan":-0.5,"mute":true},
            {"number":2,"fader_db":0.0,"pan":0.0,"mute":false,"input":2}]}"#;
        fs::write(&path, v1).unwrap_or_else(|e| panic!("{e}"));
        let file = open(&path).unwrap_or_else(|e| panic!("{e}")).file;
        assert_eq!(file.version, FORMAT_VERSION);
        let (one, two) = (&file.tracks[0], &file.tracks[1]);
        assert_eq!(one.monitor, MonitorEntry { gain_db: -6.0, pan: -0.5, mute: true }, "the old fader becomes MONITOR GAIN");
        assert_eq!((one.fader_db, one.pan, one.mute), (0.0, 0.0, false), "the channel starts neutral");
        assert_eq!(one.channel, ChannelEntry::default(), "RMX, nothing assigned");
        assert_eq!(
            two.channel,
            ChannelEntry { source: ChannelSource::Line, direct: true, ..ChannelEntry::default() },
            "it recorded its input"
        );
        assert_eq!(two.input, Some(2));
        assert_eq!(file.mixer, MixerEntry::default());
    }

    #[test]
    fn rejects_other_files_and_newer_formats() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = root.path().join(PROJECT_FILE);
        // A project from before the meter setting existed opens with PRE meters.
        fs::write(&path, r#"{"format":"daw388-project","version":1,"app_version":"0.2.0","master_db":-3.0}"#)
            .unwrap_or_else(|e| panic!("{e}"));
        let old = open(&path).unwrap_or_else(|e| panic!("{e}"));
        assert!(!old.file.meter_post && old.file.master_db == -3.0);
        assert_eq!(old.file.transport, TransportSettings::default(), "older projects: zero at the start, no cue, 388 mode, FIX");
        assert_eq!(old.file.metronome.bpm, 120.0, "and 120 BPM in 4/4");
        fs::write(&path, r#"{"format":"iets-anders","version":1,"app_version":"x"}"#).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(open(&path), Err(ProjectError::NotAProject(_))));
        fs::write(&path, r#"{"format":"daw388-project","version":99,"app_version":"x"}"#).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(open(&path), Err(ProjectError::TooNew(99))));
        assert!(matches!(open(&root.path().join("nergens")), Err(ProjectError::Io { .. })));
    }
}
