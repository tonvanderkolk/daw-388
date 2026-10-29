//! The project folder: `project.json` with the mix settings, `audio/` with the WAV files the
//! tracks play and `backups/` with earlier versions of `project.json`.
//!
//! Saving copies every track's audio into `audio/` (unless it is already there), so a project
//! folder can be moved, backed up or shared as a whole.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
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
const FORMAT_VERSION: u32 = 3;

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
    pub tape: TapeSettings,
    #[serde(default)]
    pub editor: EditorSettings,
    #[serde(default)]
    pub tracks: Vec<TrackEntry>,
}

/// The drawer with the tracks, saved with the project (optional in format 3; missing: closed,
/// 320 pixels high, the whole song in view, RASTER on).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorSettings {
    pub open: bool,
    /// Height in pixels, the toolbar and the grip included.
    pub height: f32,
    /// Pixels per second; `None` shows the whole song.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
    /// RASTER: snap to the metronome's beats.
    pub snap: bool,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self { open: false, height: 320.0, zoom: None, snap: true }
    }
}

/// One track in `project.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackEntry {
    /// Track number, 1 to 8.
    pub number: u8,
    /// Format 1 and 2 only: the track's audio file, relative to the project folder. Opening turns
    /// it into a region.
    #[serde(default, skip_serializing)]
    pub file: Option<String>,
    /// The track's name (the strip, the stems and the names of new takes).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<RegionEntry>,
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

/// One region in `project.json`. `length` is missing only for the single region of a track that
/// fills it from 0:00 while its length is unknown (see [`crate::regions::Unresolved`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionEntry {
    /// Audio file, relative to the project folder (for example `audio/01_Kick.wav`).
    pub file: String,
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub offset: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
    #[serde(default)]
    pub name: String,
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

/// A closed send: the bottom of its knob, which the UI shows as -inf.
pub const SEND_CLOSED_DB: f32 = -60.0;

/// A mixer channel in `project.json` (the fader, pan, mute and input stay on [`TrackEntry`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelEntry {
    pub source: ChannelSource,
    pub trim_db: f32,
    pub eq: EqEntry,
    /// AUX send, PRE (before EQ and fader) or POST; closed by default.
    pub aux_db: f32,
    pub aux_pre: bool,
    /// EFF send, after the fader; closed by default.
    pub eff_db: f32,
    /// ASSIGN 1-2, 3-4, 5-6 and 7-8.
    pub assign: [bool; 4],
    /// ASSIGN L-R: to the stereo bus.
    pub lr: bool,
    /// The track records this channel after TRIM and EQ instead of its PGM bus.
    pub direct: bool,
    /// AUTO: the tape, and the input while the track hears it.
    pub auto: bool,
}

impl Default for ChannelEntry {
    fn default() -> Self {
        Self {
            source: ChannelSource::default(),
            trim_db: 0.0,
            eq: EqEntry::default(),
            aux_db: SEND_CLOSED_DB,
            aux_pre: false,
            eff_db: SEND_CLOSED_DB,
            assign: [false; 4],
            lr: false,
            direct: false,
            auto: false,
        }
    }
}

impl ChannelEntry {
    /// A channel as the simple mode sets it: AUTO, recorded through DIRECT, heard on L-R.
    pub fn simple(source: ChannelSource) -> Self {
        let source = if source == ChannelSource::Rmx { ChannelSource::Line } else { source };
        Self { source, auto: true, direct: true, lr: true, ..Self::default() }
    }
}

/// One EQ band in `project.json`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BandEntry {
    pub freq_hz: f32,
    pub gain_db: f32,
}

/// A channel's EQ; missing bands are flat at the 388's centre frequencies.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqEntry {
    pub low: BandEntry,
    pub mid: BandEntry,
    pub high: BandEntry,
}

impl Default for EqEntry {
    fn default() -> Self {
        Self {
            low: BandEntry { freq_hz: 100.0, gain_db: 0.0 },
            mid: BandEntry { freq_hz: 1000.0, gain_db: 0.0 },
            high: BandEntry { freq_hz: 8000.0, gain_db: 0.0 },
        }
    }
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

/// The plate reverb on AUX.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlateEntry {
    pub decay_s: f32,
    pub predelay_ms: f32,
    pub tone: f32,
}

impl Default for PlateEntry {
    fn default() -> Self {
        Self { decay_s: 2.0, predelay_ms: 20.0, tone: 0.5 }
    }
}

/// The echo's TIME from the metronome, or free.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EchoSyncEntry {
    #[default]
    Off,
    Quarter,
    Eighth,
    DottedEighth,
}

/// The tape echo on EFF.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EchoEntry {
    pub time_ms: f32,
    pub sync: EchoSyncEntry,
    pub feedback: f32,
    pub tone: f32,
}

impl Default for EchoEntry {
    fn default() -> Self {
        Self { time_ms: 300.0, sync: EchoSyncEntry::Off, feedback: 0.35, tone: 0.5 }
    }
}

/// An EFFECT RTN; closed when missing, so an older project's AUX headphone mix gets no reverb.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RtnEntry {
    pub gain_db: f32,
    pub pan: f32,
}

impl Default for RtnEntry {
    fn default() -> Self {
        Self { gain_db: SEND_CLOSED_DB, pan: 0.0 }
    }
}

/// The built-in effects and their returns: RTN 1 is the plate, RTN 2 the echo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffectsEntry {
    pub plate: PlateEntry,
    pub echo: EchoEntry,
    pub rtn: [RtnEntry; 2],
}

/// What the main output carries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListenSwitch {
    #[default]
    Stereo,
    Aux,
    Eff,
}

/// What the two stereo meters show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeterSwitch {
    #[default]
    Lr,
    AuxEff,
}

/// Which routing the mixer shows: one strip per track, or the 388's buses and MONITOR section.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Routing {
    #[serde(rename = "simple")]
    Simple,
    /// Projects from before 0.10.0 were made with the 388's routing.
    #[default]
    #[serde(rename = "388")]
    Classic,
}

/// The master section in `project.json` (the STEREO MASTER is `master_db`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MixerEntry {
    /// Eenvoudig or 388.
    pub mode: Routing,
    /// PGM MASTERS ODD and EVEN.
    pub odd_db: f32,
    pub even_db: f32,
    pub mon: MonSwitch,
    pub mono: bool,
    pub aux_master_db: f32,
    pub eff_master_db: f32,
    /// MONITOR/PHONES: the main output's level.
    pub phones_db: f32,
    pub listen: ListenSwitch,
    pub meter: MeterSwitch,
    pub effects: EffectsEntry,
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

/// Clean or 388: the tape's character on playback. Missing (older projects): Clean.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TapeMode {
    #[default]
    #[serde(rename = "clean")]
    Clean,
    #[serde(rename = "388")]
    Machine,
}

/// Tape speed in inches per second.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpeedEntry {
    #[serde(rename = "3.75")]
    Ips3_75,
    #[default]
    #[serde(rename = "7.5")]
    Ips7_5,
    #[serde(rename = "15")]
    Ips15,
}

/// Tape stock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KindEntry {
    #[default]
    #[serde(rename = "456")]
    T456,
    #[serde(rename = "250")]
    T250,
    #[serde(rename = "911")]
    T911,
}

/// The tape in `project.json`; the four amounts run from 0 to 2, 1 being the machine.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TapeSettings {
    pub mode: TapeMode,
    pub speed: SpeedEntry,
    pub kind: KindEntry,
    pub saturation: f32,
    pub wow_flutter: f32,
    pub noise: f32,
    pub crosstalk: f32,
    /// dbx on tracks 1-4 and 5-8; on when missing, as the 388 has it built in.
    pub dbx: [bool; 2],
}

impl Default for TapeSettings {
    fn default() -> Self {
        Self {
            mode: TapeMode::Clean,
            speed: SpeedEntry::Ips7_5,
            kind: KindEntry::T456,
            saturation: 1.0,
            wow_flutter: 1.0,
            noise: 1.0,
            crosstalk: 1.0,
            dbx: [true; 2],
        }
    }
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
    pub tape: TapeSettings,
    pub editor: EditorSettings,
}

/// What the UI hands over for one track when saving.
#[derive(Debug, Clone, Default)]
pub struct TrackToSave {
    pub name: String,
    pub regions: Vec<RegionToSave>,
    pub fader_db: f32,
    pub pan: f32,
    pub mute: bool,
    /// Input channel, counted from 1.
    pub input: Option<u16>,
    pub channel: ChannelEntry,
    pub monitor: MonitorEntry,
}

/// A region as the UI hands it over: where its source is now (anywhere on disk).
#[derive(Debug, Clone, PartialEq)]
pub struct RegionToSave {
    pub source: PathBuf,
    pub start: f64,
    pub offset: f64,
    pub length: Option<f64>,
    pub name: String,
}

/// The result of a save.
#[derive(Debug, Clone)]
pub struct Saved {
    /// Per source, where it was and where it lives now (inside the project's `audio/` folder).
    pub sources: HashMap<PathBuf, PathBuf>,
    /// The backup made of the previous `project.json`, if any.
    pub backup: Option<PathBuf>,
}

/// A project read from disk.
#[derive(Debug, Clone)]
pub struct OpenedProject {
    pub dir: PathBuf,
    pub file: ProjectFile,
    /// Regions that were left out because they did not make sense.
    pub warnings: Vec<String>,
}

impl OpenedProject {
    /// Absolute path of a region's audio file.
    pub fn region_path(&self, region: &RegionEntry) -> PathBuf {
        self.dir.join(&region.file)
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
            Self::Io { action, path, error } => write!(f, "{action} {} failed: {error}", path.display()),
            Self::Json(e) => write!(f, "project.json is not readable: {e}"),
            Self::NotAProject(path) => write!(f, "{} is not a DAW 388 project", path.display()),
            Self::TooNew(v) => write!(f, "this project was made with a newer version of DAW 388 (format {v})"),
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
    let text = fs::read_to_string(&file).map_err(io("Reading", &file))?;
    let mut project: ProjectFile = serde_json::from_str(&text).map_err(ProjectError::Json)?;
    if project.format != FORMAT {
        return Err(ProjectError::NotAProject(file));
    }
    if project.version > FORMAT_VERSION {
        return Err(ProjectError::TooNew(project.version));
    }
    let from = project.version;
    if from < 2 {
        upgrade_v1(&mut project);
    }
    if from < 3 {
        upgrade_v2(&mut project);
    }
    project.version = FORMAT_VERSION;
    let warnings = check_regions(&mut project);
    Ok(OpenedProject { dir, file: project, warnings })
}

/// Format 2 had one file per track: it becomes one region from 0:00 whose length is known once
/// the file has loaded.
fn upgrade_v2(project: &mut ProjectFile) {
    for track in &mut project.tracks {
        if let Some(file) = track.file.take() {
            let name = Path::new(&file).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            track.name.clone_from(&name);
            track.regions = vec![RegionEntry { file, start: 0.0, offset: 0.0, length: None, name }];
        }
    }
}

/// Sort the regions and leave out what does not make sense (a file edited by hand): a region
/// without a length unless it is the only one at 0:00, a too short one, negative times and
/// overlaps.
fn check_regions(project: &mut ProjectFile) -> Vec<String> {
    let mut warnings = Vec::new();
    for track in &mut project.tracks {
        track.regions.sort_by(|a, b| a.start.total_cmp(&b.start));
        let alone = track.regions.len() == 1;
        let mut end = 0.0f64;
        let mut kept = Vec::with_capacity(track.regions.len());
        for region in track.regions.drain(..) {
            let fine = match region.length {
                None => alone && region.start == 0.0 && region.offset == 0.0,
                Some(length) => {
                    length >= crate::regions::MIN_LENGTH && region.start >= 0.0 && region.offset >= 0.0 && region.start >= end - 1e-9
                }
            };
            if fine {
                end = region.start + region.length.unwrap_or(0.0);
                kept.push(region);
            } else {
                warnings.push(format!("Track {}: region {} skipped", track.number, region.name));
            }
        }
        track.regions = kept;
    }
    warnings
}

/// Format 1 had one strip per track with its own input: that is the simple mode. Fader, pan
/// and mute stay where they are; the input stays the track's input.
fn upgrade_v1(project: &mut ProjectFile) {
    for track in &mut project.tracks {
        track.channel = ChannelEntry::simple(ChannelSource::Line);
        track.monitor = MonitorEntry::default();
    }
    project.mixer = MixerEntry { mode: Routing::Simple, ..MixerEntry::default() };
}

/// Save a project into `dir` (created when needed). Audio from outside the project is copied
/// into `audio/`. The previous `project.json` goes to `backups/` first: always when `backup` is
/// [`Backup::Always`], else only when the newest backup is older than [`BACKUP_INTERVAL`].
pub fn save(dir: &Path, settings: &ProjectSettings, tracks: &[TrackToSave], backup: Backup) -> Result<Saved, ProjectError> {
    let audio_dir = dir.join(AUDIO_DIR);
    fs::create_dir_all(&audio_dir).map_err(io("Creating", &audio_dir))?;

    // Every source once, however many regions use it.
    let mut placed: HashMap<PathBuf, PathBuf> = HashMap::new();
    for region in tracks.iter().flat_map(|t| &t.regions) {
        if let Entry::Vacant(entry) = placed.entry(region.source.clone()) {
            entry.insert(place_audio(&audio_dir, &region.source)?);
        }
    }
    let entry_of = |region: &RegionToSave| {
        let path = placed.get(&region.source).unwrap_or(&region.source);
        RegionEntry {
            file: relative(dir, path).unwrap_or_else(|| path.to_string_lossy().into_owned()),
            start: region.start,
            offset: region.offset,
            length: region.length,
            name: region.name.clone(),
        }
    };
    let mut entries = Vec::with_capacity(tracks.len());
    for (index, track) in tracks.iter().enumerate() {
        let number = u8::try_from(index + 1).unwrap_or(u8::MAX);
        entries.push(TrackEntry {
            number,
            file: None,
            name: track.name.clone(),
            regions: track.regions.iter().map(entry_of).collect(),
            fader_db: track.fader_db,
            pan: track.pan,
            mute: track.mute,
            input: track.input,
            channel: track.channel,
            monitor: track.monitor,
        });
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
        tape: settings.tape,
        editor: settings.editor,
        tracks: entries,
    };
    let path = dir.join(PROJECT_FILE);
    let backup = if path.exists() { make_backup(dir, &path, backup)? } else { None };
    let json = serde_json::to_string_pretty(&project).map_err(ProjectError::Json)?;
    // Write next to the target and rename, so a crash never leaves a half-written project.json.
    let temporary = dir.join(format!("{PROJECT_FILE}.tmp"));
    fs::write(&temporary, json).map_err(io("Writing", &temporary))?;
    fs::rename(&temporary, &path).map_err(io("Writing", &path))?;
    Ok(Saved { sources: placed, backup })
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
    fs::copy(source, &target).map_err(io("Copying to", &target))?;
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
    fs::create_dir_all(&backups).map_err(io("Creating", &backups))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut target = backups.join(format!("project-{stamp}.json"));
    let mut n = 2;
    while target.exists() {
        target = backups.join(format!("project-{stamp}-{n}.json"));
        n += 1;
    }
    fs::copy(current, &target).map_err(io("Backing up to", &target))?;
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
                name: source.as_ref().map(|_| format!("Track {}", i + 1)).unwrap_or_default(),
                regions: source.map(|source| vec![region(source, 0.0, Some(100.0 / 48_000.0))]).unwrap_or_default(),
                fader_db: -3.0 * i as f32,
                pan: 0.5,
                mute: i == 1,
                input: Some(i as u16 + 1),
                channel: ChannelEntry {
                    source: ChannelSource::Mic,
                    trim_db: 12.0,
                    eq: EqEntry { mid: BandEntry { freq_hz: 2500.0, gain_db: -4.5 }, ..EqEntry::default() },
                    aux_db: if i == 1 { -6.0 } else { SEND_CLOSED_DB },
                    aux_pre: i == 1,
                    eff_db: if i == 1 { -12.0 } else { SEND_CLOSED_DB },
                    assign: [false, i == 1, false, false],
                    lr: i == 0,
                    direct: i == 1,
                    auto: i == 1,
                },
                monitor: MonitorEntry { gain_db: -4.0, pan: 0.25, mute: i == 0 },
            })
            .collect()
    }

    fn region(source: PathBuf, start: f64, length: Option<f64>) -> RegionToSave {
        let name = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        RegionToSave { source, start, offset: 0.0, length, name }
    }

    #[test]
    fn a_channel_without_eq_opens_flat() {
        let channel: ChannelEntry = serde_json::from_str(r#"{"source":"line","trim_db":3.0}"#).unwrap_or_default();
        assert_eq!(channel.trim_db, 3.0);
        assert_eq!(channel.eq, EqEntry::default());
        assert_eq!((channel.eq.low.freq_hz, channel.eq.mid.freq_hz, channel.eq.high.freq_hz), (100.0, 1000.0, 8000.0));
        assert_eq!(channel.eq.mid.gain_db, 0.0);
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
                editor: EditorSettings { open: true, height: 400.0, zoom: Some(80.0), snap: false },
                tape: TapeSettings {
                    mode: TapeMode::Machine,
                    speed: SpeedEntry::Ips15,
                    kind: KindEntry::T250,
                    saturation: 1.5,
                    wow_flutter: 0.5,
                    noise: 0.0,
                    crosstalk: 2.0,
                    dbx: [false, true],
                },
                mixer: MixerEntry {
                    mode: Routing::Simple,
                    odd_db: -2.0,
                    even_db: 1.0,
                    mon: MonSwitch::Pgm,
                    mono: true,
                    aux_master_db: -1.0,
                    eff_master_db: -2.0,
                    phones_db: -3.0,
                    listen: ListenSwitch::Aux,
                    meter: MeterSwitch::AuxEff,
                    effects: EffectsEntry {
                        plate: PlateEntry { decay_s: 4.0, predelay_ms: 35.0, tone: 0.2 },
                        echo: EchoEntry { time_ms: 120.0, sync: EchoSyncEntry::Eighth, feedback: 0.6, tone: 0.8 },
                        rtn: [RtnEntry { gain_db: -3.0, pan: 0.25 }, RtnEntry { gain_db: 0.0, pan: -0.5 }],
                    },
                },
            },
            &tracks([Some(kick.clone()), None]),
            Backup::Always,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let copied = saved.sources.get(&kick).cloned().unwrap_or_else(|| panic!("no source"));
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
        assert_eq!(
            opened.file.tape,
            TapeSettings {
                mode: TapeMode::Machine,
                speed: SpeedEntry::Ips15,
                kind: KindEntry::T250,
                saturation: 1.5,
                wow_flutter: 0.5,
                noise: 0.0,
                crosstalk: 2.0,
                dbx: [false, true],
            }
        );
        assert_eq!(opened.file.tracks.len(), 2);
        assert_eq!(opened.file.tracks[0].regions[0].file, "audio/01_Kick.wav");
        assert_eq!(opened.file.tracks[0].name, "Track 1");
        assert!(opened.file.tracks[1].regions.is_empty());
        assert!(opened.file.tracks[1].mute);
        assert_eq!(opened.file.tracks[1].fader_db, -3.0);
        assert_eq!(opened.file.tracks[1].input, Some(2));
        assert_eq!(
            opened.file.tracks[1].channel,
            ChannelEntry {
                source: ChannelSource::Mic,
                trim_db: 12.0,
                eq: EqEntry { mid: BandEntry { freq_hz: 2500.0, gain_db: -4.5 }, ..EqEntry::default() },
                aux_db: -6.0,
                aux_pre: true,
                eff_db: -12.0,
                assign: [false, true, false, false],
                lr: false,
                direct: true,
                auto: true
            }
        );
        assert_eq!(opened.file.tracks[0].monitor, MonitorEntry { gain_db: -4.0, pan: 0.25, mute: true });
        assert_eq!(
            opened.file.mixer,
            MixerEntry {
                mode: Routing::Simple,
                odd_db: -2.0,
                even_db: 1.0,
                mon: MonSwitch::Pgm,
                mono: true,
                aux_master_db: -1.0,
                eff_master_db: -2.0,
                phones_db: -3.0,
                listen: ListenSwitch::Aux,
                meter: MeterSwitch::AuxEff,
                effects: EffectsEntry {
                    plate: PlateEntry { decay_s: 4.0, predelay_ms: 35.0, tone: 0.2 },
                    echo: EchoEntry { time_ms: 120.0, sync: EchoSyncEntry::Eighth, feedback: 0.6, tone: 0.8 },
                    rtn: [RtnEntry { gain_db: -3.0, pan: 0.25 }, RtnEntry { gain_db: 0.0, pan: -0.5 }]
                },
            }
        );
        assert_eq!(opened.file.editor, EditorSettings { open: true, height: 400.0, zoom: Some(80.0), snap: false });
        assert_eq!(opened.file.version, FORMAT_VERSION, "a format 3 file is not upgraded again");
        assert!(crate::load_wav(&opened.region_path(&opened.file.tracks[0].regions[0])).is_ok());
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
        let saved = save(&dir, &ProjectSettings::default(), &tracks([Some(first.clone()), Some(second.clone())]), Backup::Always)
            .unwrap_or_else(|e| panic!("{e}"));
        let opened = open(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(opened.file.tracks[0].regions[0].file, "audio/take.wav");
        assert_eq!(opened.file.tracks[1].regions[0].file, "audio/take-2.wav");

        let (inside_first, inside_second) = (saved.sources.get(&first).cloned(), saved.sources.get(&second).cloned());
        let again = save(&dir, &ProjectSettings::default(), &tracks([inside_first.clone(), inside_second.clone()]), Backup::Always)
            .unwrap_or_else(|e| panic!("{e}"));
        for inside in [inside_first, inside_second].into_iter().flatten() {
            assert_eq!(again.sources.get(&inside), Some(&inside), "a file in audio/ stays where it is");
        }
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
        assert_eq!(opened.file.tracks[0].regions[0].file, "audio/weg.wav");
    }

    fn write_project(dir: &Path, json: &str) -> PathBuf {
        let path = dir.join(PROJECT_FILE);
        fs::write(&path, json).unwrap_or_else(|e| panic!("{e}"));
        path
    }

    #[test]
    fn a_format_2_track_becomes_one_region_without_a_length() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        for version in [1, 2] {
            let json = format!(
                r#"{{"format":"daw388-project","version":{version},"app_version":"0.16.0","tracks":[{{"number":1,"file":"audio/01_Kick.wav"}}]}}"#
            );
            let opened = open(&write_project(root.path(), &json)).unwrap_or_else(|e| panic!("{e}"));
            let track = &opened.file.tracks[0];
            assert_eq!(opened.file.version, FORMAT_VERSION);
            assert_eq!((track.name.as_str(), track.file.as_deref()), ("01_Kick", None));
            assert_eq!(
                track.regions,
                vec![RegionEntry { file: "audio/01_Kick.wav".into(), start: 0.0, offset: 0.0, length: None, name: "01_Kick".into() }]
            );
            assert!(opened.warnings.is_empty());
            assert_eq!(opened.file.editor, EditorSettings::default(), "no editor block: the default");
        }
    }

    #[test]
    fn regions_survive_save_and_open() {
        let outside = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (gitaar, bas) = (outside.path().join("gitaar.wav"), outside.path().join("bas.wav"));
        write_wav(&gitaar);
        write_wav(&bas);
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("p");
        let mut list = tracks([None, None]);
        list[0].regions =
            vec![region(gitaar.clone(), 0.0, Some(0.001)), RegionToSave { offset: 0.0005, ..region(gitaar.clone(), 1.5, Some(0.0012)) }];
        list[1].regions = vec![region(bas.clone(), 2.25, Some(0.002))];
        let saved = save(&dir, &ProjectSettings::default(), &list, Backup::Always).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(fs::read_dir(dir.join(AUDIO_DIR)).map(Iterator::count).unwrap_or(0), 2, "each source is copied once");
        assert!(saved.sources.get(&gitaar).is_some_and(|p| p.starts_with(dir.join(AUDIO_DIR))));
        let opened = open(&dir).unwrap_or_else(|e| panic!("{e}"));
        let regions = &opened.file.tracks[0].regions;
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].file, regions[1].file);
        assert!((regions[1].start - 1.5).abs() < 1e-9 && (regions[1].offset - 0.0005).abs() < 1e-9);
        assert_eq!(regions[1].length, Some(0.0012));
        assert_eq!(opened.file.tracks[1].regions[0].file, "audio/bas.wav");
    }

    #[test]
    fn an_unresolved_region_keeps_its_reference() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("p");
        let mut list = tracks([None, None]);
        list[0].regions = vec![region(dir.join(AUDIO_DIR).join("weg.wav"), 0.0, None)];
        save(&dir, &ProjectSettings::default(), &list, Backup::Always).unwrap_or_else(|e| panic!("{e}"));
        let opened = open(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(opened.file.tracks[0].regions.len(), 1);
        assert_eq!(opened.file.tracks[0].regions[0].length, None);
    }

    #[test]
    fn a_region_without_length_elsewhere_is_dropped() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let json = r#"{"format":"daw388-project","version":3,"app_version":"0.17.0","tracks":[{"number":1,"regions":[
            {"file":"audio/a.wav","start":0.0,"length":1.0,"name":"a"},
            {"file":"audio/b.wav","start":2.0,"name":"b"},
            {"file":"audio/c.wav","start":0.5,"length":1.0,"name":"c"}]}]}"#;
        let opened = open(&write_project(root.path(), json)).unwrap_or_else(|e| panic!("{e}"));
        let names: Vec<&str> = opened.file.tracks[0].regions.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["a"], "b has no length and c overlaps a");
        assert_eq!(opened.warnings.len(), 2);
    }

    #[test]
    fn a_format_1_project_becomes_a_simple_project() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = root.path().join(PROJECT_FILE);
        let v1 = r#"{"format":"daw388-project","version":1,"app_version":"0.7.0","tracks":[
            {"number":1,"fader_db":-6.0,"pan":-0.5,"mute":true},
            {"number":2,"fader_db":0.0,"pan":0.0,"mute":false,"input":2}]}"#;
        fs::write(&path, v1).unwrap_or_else(|e| panic!("{e}"));
        let file = open(&path).unwrap_or_else(|e| panic!("{e}")).file;
        assert_eq!(file.version, FORMAT_VERSION);
        assert_eq!(file.mixer.mode, Routing::Simple);
        let (one, two) = (&file.tracks[0], &file.tracks[1]);
        assert_eq!((one.fader_db, one.pan, one.mute), (-6.0, -0.5, true), "the fader stays the fader");
        assert_eq!(one.monitor, MonitorEntry::default());
        assert_eq!(one.channel, ChannelEntry::simple(ChannelSource::Line));
        assert_eq!(two.channel, ChannelEntry::simple(ChannelSource::Line));
        assert_eq!(two.input, Some(2));
    }

    #[test]
    fn sends_start_closed_and_the_master_section_at_its_defaults() {
        let channel: ChannelEntry = serde_json::from_str(r#"{"source":"line"}"#).unwrap_or_default();
        assert_eq!((channel.aux_db, channel.eff_db, channel.aux_pre), (SEND_CLOSED_DB, SEND_CLOSED_DB, false));
        assert_eq!(ChannelEntry::default().aux_db, SEND_CLOSED_DB);
        assert_eq!(ChannelEntry::simple(ChannelSource::Line).eff_db, SEND_CLOSED_DB);
        let mixer: MixerEntry = serde_json::from_str("{}").unwrap_or_default();
        assert_eq!((mixer.aux_master_db, mixer.eff_master_db, mixer.phones_db), (0.0, 0.0, 0.0));
        assert_eq!((mixer.listen, mixer.meter), (ListenSwitch::Stereo, MeterSwitch::Lr));
        assert_eq!(serde_json::to_string(&MeterSwitch::AuxEff).unwrap_or_default(), r#""aux_eff""#);
        assert_eq!(mixer.effects.rtn, [RtnEntry::default(); 2], "older projects: the returns stay closed");
        assert_eq!(mixer.effects.rtn[0].gain_db, SEND_CLOSED_DB);
        assert_eq!((mixer.effects.plate, mixer.effects.echo), (PlateEntry::default(), EchoEntry::default()));
        assert_eq!(serde_json::to_string(&EchoSyncEntry::DottedEighth).unwrap_or_default(), r#""dotted_eighth""#);
    }

    #[test]
    fn a_project_without_a_mode_is_a_388_project() {
        let mixer: MixerEntry = serde_json::from_str(r#"{"odd_db":-1.0}"#).unwrap_or_default();
        assert_eq!((mixer.mode, mixer.odd_db), (Routing::Classic, -1.0));
        let channel: ChannelEntry = serde_json::from_str(r#"{"source":"line"}"#).unwrap_or_default();
        assert!(!channel.auto);
        assert_eq!(serde_json::to_string(&Routing::Classic).unwrap_or_default(), r#""388""#);
        assert_eq!(serde_json::to_string(&Routing::Simple).unwrap_or_default(), r#""simple""#);
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
        assert_eq!(old.file.tape, TapeSettings::default(), "and Clean tape, so it sounds as it was mixed");
        assert_eq!(old.file.tape.dbx, [true; 2], "dbx is built in");
        assert_eq!(serde_json::to_string(&SpeedEntry::Ips3_75).unwrap_or_default(), r#""3.75""#);
        fs::write(&path, r#"{"format":"iets-anders","version":1,"app_version":"x"}"#).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(open(&path), Err(ProjectError::NotAProject(_))));
        fs::write(&path, r#"{"format":"daw388-project","version":99,"app_version":"x"}"#).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(open(&path), Err(ProjectError::TooNew(99))));
        assert!(matches!(open(&root.path().join("nergens")), Err(ProjectError::Io { .. })));
    }
}
