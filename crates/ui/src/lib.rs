//! The egui interface of DAW 388. Platform-specific parts (audio devices, file dialogs) come in
//! through the [`Platform`] trait, so the same UI can later run in the browser.

mod console;
mod content;
mod drawer;
mod edit;
mod export;
mod history;
mod recording;
mod skin;
mod sources;
mod timeline;

use daw388_engine::{
    ChannelParams, Command, EQ_DEFAULTS, EchoParams, EchoSync, EngineHandle, EqBand, Listen, Meter, MixerParams, MonitorMode,
    MonitorParams, Motion, PITCH_RANGE, PlateParams, RecMode, RtnParams, Source, Speed, TRACKS, TapeKind, TapeParams, TrackAudio,
    TransportStyle,
};
use daw388_project::export::{BitDepth, ExportFormat};
use daw388_project::peaks::Peaks;
use daw388_project::preferences::{self, Preferences};
use daw388_project::regions::{Clipboard, Region, SourceId, Unresolved, track_end};
use daw388_project::session::{self, PROJECT_FILE};
use daw388_project::{
    Backup, BandEntry, ChannelEntry, ChannelSource, EchoSyncEntry, EditorSettings, EqEntry, KindEntry, ListenSwitch, MeterSwitch,
    MetronomeSettings, MixerEntry, MonSwitch, MonitorEntry, MonoClip, ProjectSettings, RegionToSave, RepeatMode, Routing, SpeedEntry,
    TapeMode, TapeSettings, TrackToSave, TransportSettings,
};
use egui::{Align, Color32, CornerRadius, Frame, Key, KeyboardShortcut, Layout, Margin, Modifiers, RichText, Sense, Stroke, Ui, vec2};
use export::{ExportJob, ExportTrack, Progress, StemKind};
use history::History;
use recording::{StoredTake, TakeDone};
use skin::keys::{self, KeyCap, KeyColour, Symbol};
pub(crate) use skin::palette;
use sources::Sources;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

pub use daw388_project::AUDIO_EXTENSIONS;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

/// An audio output device and what it offers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceInfo {
    pub name: String,
    /// Standard sample rates the device can run at.
    pub sample_rates: Vec<u32>,
    /// Smallest and largest buffer size in frames, when the device says.
    pub buffer_range: Option<(u32, u32)>,
    /// Most channels the device offers.
    pub channels: u16,
}

/// Which output to open and how.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputRequest {
    /// `None`: the system's default device.
    pub device: Option<String>,
    /// `None`: the device's own rate.
    pub sample_rate: Option<u32>,
    /// `None`: the device's default buffer.
    pub buffer_frames: Option<u32>,
    /// The input device to record from, opened at the output's rate; `None`: no input.
    pub input_device: Option<String>,
}

/// The input that was opened with the output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputInfo {
    pub device: String,
    pub channels: u16,
}

/// The output that was actually opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputInfo {
    pub device: String,
    pub sample_rate: u32,
    /// `None` when the device chose the buffer size itself.
    pub buffer_frames: Option<u32>,
    /// Interface channels of the output (AUX and EFF can use 3 and up).
    pub channels: u16,
    pub input: Option<InputInfo>,
    /// Why the requested input could not be opened.
    pub input_error: Option<String>,
}

/// A take saved in the project, back from the background job.
struct ComposeResult {
    track: usize,
    /// The project it was made in; a take for another project is not placed.
    project_generation: u64,
    /// Where it starts on the tape, in seconds (latency-corrected).
    start: f64,
    outcome: Result<StoredTake, String>,
}

/// A track rendered for the engine.
struct RenderDone {
    track: usize,
    generation: u64,
    rate: u32,
    audio: Arc<TrackAudio>,
}

/// A file chosen for a track, placed at `start` (seconds) once its source has loaded.
struct PendingImport {
    source: SourceId,
    track: usize,
    start: f64,
    /// Files dropped together share a group: one undo step.
    group: Option<u64>,
}

/// Buffer sizes offered in the settings, when the device allows them.
const BUFFER_SIZES: [u32; 7] = [32, 64, 128, 256, 512, 1024, 2048];
/// Seconds between checks whether the chosen device is back.
const DEVICE_CHECK_EVERY: f64 = 2.0;
/// Sample rate of the silent clock when no rate was chosen.
const SILENT_RATE: u32 = 48_000;

/// What the UI needs from the platform it runs on.
pub trait Platform {
    /// The available audio output devices.
    fn output_devices(&mut self) -> Vec<DeviceInfo>;
    /// The available audio input devices.
    fn input_devices(&mut self) -> Vec<DeviceInfo>;
    /// Name of the default output device, if there is one.
    fn default_output_device(&mut self) -> Option<String>;
    /// Open an output and return the engine that now feeds it. A named device that is missing is
    /// an error; the UI chooses the fallback.
    fn open_output(&mut self, request: &OutputRequest) -> Result<(EngineHandle, OutputInfo), String>;
    /// Run an engine in real time without any sound output, so the transport keeps working.
    fn open_silent(&mut self, sample_rate: u32) -> EngineHandle;
    /// The open output's device went away since it was opened.
    fn output_lost(&mut self) -> bool;
    /// Where app-wide preferences are kept, if anywhere.
    fn preferences_path(&self) -> Option<PathBuf>;
    /// Ask the user for one or more audio files.
    fn pick_audio_files(&mut self, multiple: bool) -> Vec<PathBuf>;
    /// Ask the user for a project to open (its folder or its `project.json`).
    fn pick_project_to_open(&mut self) -> Option<PathBuf>;
    /// Ask the user where to save a project. Returns the project folder to create or reuse.
    fn pick_project_location(&mut self, suggested_name: &str) -> Option<PathBuf>;
    /// Run slow work (decoding and resampling audio) away from the UI thread.
    fn spawn(&mut self, job: Box<dyn FnOnce() + Send>);
    /// Run work that lasts as long as an engine (the recording writer) on its own thread.
    fn spawn_service(&mut self, job: Box<dyn FnOnce() + Send>);
    /// A folder for temporary files (takes while they are being recorded).
    fn temp_dir(&self) -> PathBuf;
}

/// A source decoded (or resampled) in the background.
struct LoadResult {
    source: SourceId,
    generation: u64,
    outcome: Result<Loaded, String>,
    /// The rate of `Loaded::at_rate`.
    rate: u32,
}

/// A source as loaded in the background.
struct Loaded {
    clip: Arc<MonoClip>,
    /// The samples at the engine's rate.
    at_rate: Arc<Vec<f32>>,
    /// The overview for drawing; a resample keeps the one it has.
    peaks: Option<Arc<Peaks>>,
}

/// The drawer: what is saved with the project, and what is not.
#[derive(Default)]
struct EditorState {
    settings: EditorSettings,
    /// The time at the left edge of the lanes, in seconds.
    scroll: f64,
    /// Selected regions as (track, index).
    selection: Vec<(usize, usize)>,
    /// A stretch over all tracks, from and to in seconds.
    range: Option<(f64, f64)>,
    clipboard: Option<Clipboard>,
    /// What the mouse is doing.
    drag: Option<drawer::Drag>,
    /// The track whose name is being edited, the text so far, and whether to ask for focus.
    renaming: Option<(usize, String, bool)>,
    /// The track dropped files go to: the one clicked last.
    target: usize,
    /// The zoom drawn last, for − and + while the whole song is shown.
    shown_zoom: f64,
    /// The group of the last drop and its note about files that found no track.
    drop_note: Option<(u64, String)>,
}

fn style_of(mode: RepeatMode) -> TransportStyle {
    match mode {
        RepeatMode::Tape => TransportStyle::Tape,
        RepeatMode::Cycle => TransportStyle::Cycle,
    }
}

/// Seconds between a change and the automatic save that follows it.
const AUTOSAVE_AFTER: f64 = 30.0;

/// Something that would throw away unsaved changes, waiting for the user's answer.
#[derive(Debug, Clone, PartialEq)]
enum Pending {
    New,
    Close,
    Open(Option<PathBuf>),
    Quit,
}

/// The pitch control's range in percent (plus or minus).
const PITCH_PERCENT: f32 = (PITCH_RANGE * 100.0) as f32;

const MIN_DB: f32 = -60.0;
const MAX_DB: f32 = 6.0;
/// MONITOR GAIN goes up to +10 dB.
const MONITOR_MAX_DB: f32 = 10.0;

fn db_to_gain(db: f32) -> f32 {
    if db <= MIN_DB { 0.0 } else { 10f32.powf(db / 20.0) }
}

fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 { MIN_DB } else { (20.0 * gain.log10()).max(MIN_DB) }
}

/// Counter text, minutes and seconds with tenths; a minus sign before the zero point, as on the
/// 388.
pub fn format_counter(seconds: f64) -> String {
    let tenths = (seconds.abs() * 10.0).floor() as u64;
    let sign = if seconds < 0.0 && tenths > 0 { "-" } else { "" };
    format!("{sign}{:02}:{:02}.{}", tenths / 600, (tenths / 10) % 60, tenths % 10)
}

#[derive(Default)]
struct TrackState {
    /// What plays on this track, sorted and without overlap.
    regions: Vec<Region>,
    /// A file whose length is not known yet; nothing new can come on the track meanwhile.
    unresolved: Option<Unresolved>,
    /// The track's name: the strip, the exported stems and the names of new takes.
    name: String,
    /// Identifies the latest render, so a late one is ignored.
    render_generation: u64,
    /// What the engine got last: its rate and length.
    #[cfg_attr(not(test), allow(dead_code))] // Read by the tests.
    rendered: Option<(u32, usize)>,
    fader_db: f32,
    pan: f32,
    mute: bool,
    /// The track's VU needle in the meter bridge.
    vu: skin::vu::Needle,
    /// REC FUNCTION: the track is ready to record.
    armed: bool,
    /// The input channel of its mixer channel (MIC and LINE), counted from 1.
    input: Option<u16>,
    /// Source, TRIM, ASSIGN, L-R and DIRECT; fader, pan and mute are the fields above.
    channel: ChannelEntry,
    monitor: MonitorEntry,
    /// Displayed TRK/PGM meter level in dB: what the track's MONITOR hears.
    trk_meter_db: f32,
    /// SOLO: only what you hear, and not saved with the project.
    solo: bool,
    /// OL stays lit until this time (`DawApp::now`).
    ol_until: f64,
    /// The PEAK lamp of the track's VU meter stays lit until this time.
    peak_until: f64,
}

pub struct DawApp {
    platform: Box<dyn Platform>,
    engine: Option<EngineHandle>,
    devices: Vec<DeviceInfo>,
    /// The output that is open; `None` while the silent clock runs.
    output: Option<OutputInfo>,
    /// The audio output the user chose (kept between sessions).
    preferences: Preferences,
    /// When to look again whether the chosen device is available.
    next_device_check: f64,
    tracks: [TrackState; TRACKS],
    master_db: f32,
    /// PGM MASTERS, MON and MONO.
    mixer: MixerEntry,
    /// Track meters after the fader (POST) instead of the track's own level (PRE, as on the 388).
    meter_post: bool,
    /// The two big VU needles: stereo L and R, or AUX and EFF (the METER switch).
    big_vu: [skin::vu::Needle; 2],
    /// The big meters' OL lamps stay lit until this time.
    big_ol_until: [f64; 2],
    /// The reels in the tape deck.
    reels: skin::reels::Reels,
    /// Where each channel strip's centre was in the last frame (0: not drawn yet), so the meter
    /// bridge can put each TRK meter above its strip.
    strip_centres: [f32; TRACKS],
    /// What the console, header and drawer needed in the last frame, in points: the zoom fits it in the window.
    needed: egui::Vec2,
    /// The routing the window was last fitted to, and the frames to wait before fitting it
    /// (the console's size is measured a frame later).
    fitted_for: Option<Routing>,
    fit_in: u8,
    /// The zoom to keep when the window is fitted after a switch (0: take it from the height).
    fit_zoom: f32,
    /// Where the blanking plate in the EFFECT RTN column starts (Simple), for the mixer view to draw.
    return_plate: Option<egui::Rect>,
    status: String,
    /// The project folder, once saved or opened.
    project_dir: Option<PathBuf>,
    dirty: bool,
    /// "Opgeslagen 17:06" and the like.
    saved_label: Option<String>,
    /// When the next automatic save is due (in egui time), while there are unsaved changes.
    autosave_at: Option<f64>,
    now: f64,
    pending: Option<Pending>,
    /// The user agreed to quit; let the next close request through.
    may_close: bool,
    title: String,
    /// Every audio file the regions play.
    sources: Sources,
    /// Background loads report back here.
    loads_tx: Sender<LoadResult>,
    loads_rx: Receiver<LoadResult>,
    renders_tx: Sender<RenderDone>,
    renders_rx: Receiver<RenderDone>,
    pending_imports: Vec<PendingImport>,
    /// Changes with every new or opened project, so a late take of another project is not placed.
    project_generation: u64,
    next_generation: u64,
    ctx: egui::Context,
    /// Transport memory and settings, kept here so they survive a device change and are saved.
    zero_seconds: f64,
    cue_seconds: Option<f64>,
    repeat: bool,
    repeat_mode: RepeatMode,
    pitch_percent: f32,
    vari: bool,
    show_settings: bool,
    /// The tape on playback, saved with the project.
    tape: TapeSettings,
    show_tape: bool,
    export: ExportChoices,
    show_export: bool,
    running_export: Option<RunningExport>,
    /// The question before the 388 routing is reset for the simple mode.
    confirm_simple: bool,
    input_devices: Vec<DeviceInfo>,
    insert: bool,
    auto_punch: bool,
    /// Finished takes from the writer, and the jobs that lay them over their tracks.
    takes_tx: Sender<TakeDone>,
    takes_rx: Receiver<TakeDone>,
    composed_tx: Sender<ComposeResult>,
    composed_rx: Receiver<ComposeResult>,
    /// Takes waiting for their track's previous take to be laid down.
    queued_takes: [VecDeque<TakeDone>; TRACKS],
    composing: [bool; TRACKS],
    history: History,
    editor: EditorState,
    metronome: MetronomeSettings,
    /// Times of the last TAP presses, for tap tempo.
    taps: Vec<f64>,
}

/// Time signatures offered: beats per bar and the note value of a beat.
const TIME_SIGNATURES: [(u8, u8); 9] = [(2, 4), (3, 4), (4, 4), (5, 4), (6, 4), (7, 4), (3, 8), (6, 8), (7, 8)];

fn meter_of(metronome: &MetronomeSettings) -> Meter {
    Meter { bpm: metronome.bpm, beats: metronome.beats, unit: metronome.unit }
}

/// Channel `index` takes input `index + 1` until the user chooses otherwise.
fn default_input(index: usize) -> Option<u16> {
    u16::try_from(index + 1).ok()
}

/// TRIM range per source: more gain on MIC, as on the 388's microphone preamp.
fn trim_range(source: ChannelSource) -> std::ops::RangeInclusive<f32> {
    match source {
        ChannelSource::Mic => 0.0..=40.0,
        ChannelSource::Line | ChannelSource::Rmx => -10.0..=20.0,
    }
}

/// The sweep ranges of LOW, MID and HIGH, as on the 388.
const EQ_RANGES: [std::ops::RangeInclusive<f32>; 3] = [50.0..=1000.0, 500.0..=5000.0, 2500.0..=15000.0];
const EQ_NAMES: [&str; 3] = ["LOW", "MID", "HIGH"];
/// How long OL stays lit after a peak, in seconds.
const OL_HOLD: f64 = 0.5;

/// The PEAK lamp of a VU meter: on the 388 at +8 dBV, 18 dB above 0 VU, which is about full
/// scale with 0 VU at -18 dBFS.
const VU_PEAK: f32 = 0.944;

/// Where `freq` sits on a frequency knob's travel: logarithmic, 0 to 1.
fn freq_fraction(freq: f32, range: &std::ops::RangeInclusive<f32>) -> f32 {
    (freq / range.start()).ln() / (range.end() / range.start()).ln()
}

fn fraction_freq(fraction: f32, range: &std::ops::RangeInclusive<f32>) -> f32 {
    range.start() * (range.end() / range.start()).powf(fraction.clamp(0.0, 1.0))
}

/// OL: a peak after the EQ above -3 dBFS.
fn overload(peak: f32) -> bool {
    peak > 0.708
}

fn bands(eq: &EqEntry) -> [&BandEntry; 3] {
    [&eq.low, &eq.mid, &eq.high]
}

fn channel_params(track: &TrackState) -> ChannelParams {
    ChannelParams {
        source: match track.channel.source {
            ChannelSource::Mic => Source::Mic,
            ChannelSource::Line => Source::Line,
            ChannelSource::Rmx => Source::Remix,
        },
        // The engine counts input channels from 0.
        input: track.input.and_then(|c| c.checked_sub(1)),
        trim: db_to_gain(track.channel.trim_db),
        aux: db_to_gain(track.channel.aux_db),
        aux_pre: track.channel.aux_pre,
        eff: db_to_gain(track.channel.eff_db),
        eq: bands(&track.channel.eq).map(|b| EqBand { freq: b.freq_hz, gain_db: b.gain_db }),
        fader: db_to_gain(track.fader_db),
        pan: track.pan,
        mute: track.mute,
        assign: track.channel.assign,
        lr: track.channel.lr,
        direct: track.channel.direct,
        auto: track.channel.auto,
    }
}

/// The mix of a new project: the simple mode.
fn new_mixer() -> MixerEntry {
    let mut mixer = MixerEntry { mode: Routing::Simple, ..MixerEntry::default() };
    // Open returns: turn a send up and you hear the effect.
    for rtn in &mut mixer.effects.rtn {
        rtn.gain_db = 0.0;
    }
    mixer
}

/// What the Export window asks; kept while the app runs.
#[derive(Debug, Clone, PartialEq)]
struct ExportChoices {
    stereo: bool,
    /// The bounce with the tape's character (when the project plays through the 388 tape).
    stereo_tape: bool,
    stems: bool,
    stem_tracks: [bool; TRACKS],
    stem_kind: StemKind,
    /// From the zero point to the CUE point instead of the whole song.
    to_cue: bool,
    from_zero: bool,
    format: ExportFormat,
    depth: BitDepth,
    rate: u32,
}

impl Default for ExportChoices {
    fn default() -> Self {
        Self {
            stereo: true,
            stereo_tape: true,
            stems: false,
            stem_tracks: [true; TRACKS],
            stem_kind: StemKind::Mixed,
            to_cue: false,
            from_zero: true,
            format: ExportFormat::Wav,
            depth: BitDepth::Int24,
            rate: 48_000,
        }
    }
}

/// An export on its way: its progress and where its result arrives.
struct RunningExport {
    progress: Arc<Progress>,
    result: Receiver<Result<Vec<PathBuf>, String>>,
}

/// The tape of a new project: the 388 at 7½ IPS on 456.
fn new_tape() -> TapeSettings {
    TapeSettings { mode: TapeMode::Machine, ..TapeSettings::default() }
}

fn tape_params(tape: &TapeSettings) -> TapeParams {
    TapeParams {
        on: tape.mode == TapeMode::Machine,
        speed: match tape.speed {
            SpeedEntry::Ips3_75 => Speed::Ips3_75,
            SpeedEntry::Ips7_5 => Speed::Ips7_5,
            SpeedEntry::Ips15 => Speed::Ips15,
        },
        kind: match tape.kind {
            KindEntry::T456 => TapeKind::T456,
            KindEntry::T250 => TapeKind::T250,
            KindEntry::T911 => TapeKind::T911,
        },
        saturation: tape.saturation,
        wow_flutter: tape.wow_flutter,
        noise: tape.noise,
        crosstalk: tape.crosstalk,
        dbx: tape.dbx,
    }
}

/// The tape's state in a few characters, for the TAPE key.
fn tape_label(tape: &TapeSettings) -> String {
    if tape.mode == TapeMode::Clean {
        return "TAPE: CLEAN".into();
    }
    let speed = match tape.speed {
        SpeedEntry::Ips3_75 => "3¾",
        SpeedEntry::Ips7_5 => "7½",
        SpeedEntry::Ips15 => "15",
    };
    let kind = match tape.kind {
        KindEntry::T456 => "456",
        KindEntry::T250 => "250",
        KindEntry::T911 => "911",
    };
    format!("TAPE: {speed} IPS {kind}")
}

fn monitor_params(monitor: &MonitorEntry) -> MonitorParams {
    MonitorParams { gain: db_to_gain(monitor.gain_db), pan: monitor.pan, mute: monitor.mute }
}

/// An AUX or EFF output preference (from 1) as the engine's channel (from 0): only channel 3 and
/// up, and only when the open output has it.
fn send_output(preference: Option<u16>, output_channels: u16) -> Option<usize> {
    preference.filter(|c| (3..=output_channels).contains(c)).map(|c| usize::from(c) - 1)
}

fn mixer_params(mixer: &MixerEntry, preferences: &Preferences, output_channels: u16) -> MixerParams {
    let mon = match mixer.mon {
        MonSwitch::Pgm => MonitorMode::Pgm,
        MonSwitch::Cue => MonitorMode::Cue,
    };
    let effects = &mixer.effects;
    let listen = match mixer.listen {
        ListenSwitch::Stereo => Listen::Stereo,
        ListenSwitch::Aux => Listen::Aux,
        ListenSwitch::Eff => Listen::Eff,
    };
    MixerParams {
        odd: db_to_gain(mixer.odd_db),
        even: db_to_gain(mixer.even_db),
        mon,
        mono: mixer.mono,
        aux_master: db_to_gain(mixer.aux_master_db),
        eff_master: db_to_gain(mixer.eff_master_db),
        listen,
        phones: db_to_gain(mixer.phones_db),
        aux_out: send_output(preferences.aux_output, output_channels),
        eff_out: send_output(preferences.eff_output, output_channels),
        plate: PlateParams { decay_s: effects.plate.decay_s, predelay_ms: effects.plate.predelay_ms, tone: effects.plate.tone },
        echo: EchoParams {
            time_ms: effects.echo.time_ms,
            sync: match effects.echo.sync {
                EchoSyncEntry::Off => EchoSync::Off,
                EchoSyncEntry::Quarter => EchoSync::Quarter,
                EchoSyncEntry::Eighth => EchoSync::Eighth,
                EchoSyncEntry::DottedEighth => EchoSync::DottedEighth,
            },
            feedback: effects.echo.feedback,
            tone: effects.echo.tone,
        },
        rtn: effects.rtn.map(|r| RtnParams { gain: db_to_gain(r.gain_db), pan: r.pan }),
    }
}

impl DawApp {
    pub fn new(ctx: &egui::Context, mut platform: Box<dyn Platform>) -> Self {
        apply_style(ctx);
        let devices = platform.output_devices();
        let preferences = platform.preferences_path().map(|path| preferences::load(&path)).unwrap_or_default();
        let (loads_tx, loads_rx) = channel();
        let (renders_tx, renders_rx) = channel();
        let (takes_tx, takes_rx) = channel();
        let (composed_tx, composed_rx) = channel();
        let input_devices = platform.input_devices();
        let mut app = Self {
            platform,
            engine: None,
            devices,
            output: None,
            preferences,
            next_device_check: DEVICE_CHECK_EVERY,
            tracks: Default::default(),
            master_db: 0.0,
            mixer: new_mixer(),
            meter_post: false,
            big_vu: Default::default(),
            big_ol_until: [0.0; 2],
            reels: Default::default(),
            strip_centres: [0.0; TRACKS],
            needed: egui::Vec2::ZERO,
            fitted_for: None,
            fit_in: 0,
            fit_zoom: 0.0,
            return_plate: None,
            status: String::new(),
            project_dir: None,
            dirty: false,
            saved_label: None,
            autosave_at: None,
            now: 0.0,
            pending: None,
            may_close: false,
            title: String::new(),
            sources: Sources::default(),
            loads_tx,
            loads_rx,
            renders_tx,
            renders_rx,
            pending_imports: Vec::new(),
            project_generation: 0,
            next_generation: 1,
            ctx: ctx.clone(),
            zero_seconds: 0.0,
            cue_seconds: None,
            repeat: false,
            repeat_mode: RepeatMode::Tape,
            pitch_percent: 0.0,
            vari: false,
            show_settings: false,
            tape: new_tape(),
            show_tape: false,
            export: ExportChoices::default(),
            show_export: false,
            running_export: None,
            confirm_simple: false,
            input_devices,
            insert: false,
            auto_punch: false,
            takes_tx,
            takes_rx,
            composed_tx,
            composed_rx,
            queued_takes: Default::default(),
            composing: [false; TRACKS],
            history: History::default(),
            editor: EditorState::default(),
            metronome: MetronomeSettings::default(),
            taps: Vec::new(),
        };
        for (index, track) in app.tracks.iter_mut().enumerate() {
            track.trk_meter_db = MIN_DB;
            track.input = default_input(index);
            track.channel = ChannelEntry::simple(ChannelSource::Line);
        }
        app.open_output();
        app
    }

    /// Open the chosen output, else the default device, else the silent clock, and bring the new
    /// engine up to date with the tracks, the mix and the transport.
    fn open_output(&mut self) {
        let position = self.engine.as_ref().map_or(0.0, EngineHandle::position_seconds);
        // Carry on playing after a device change or a new rate or buffer.
        let was_playing = self.engine.as_ref().is_some_and(EngineHandle::is_playing);
        // Drop the old engine first: some systems allow only one open stream per device.
        self.engine = None;
        let request = OutputRequest {
            device: self.preferences.output_device.clone(),
            sample_rate: self.preferences.sample_rate,
            buffer_frames: self.preferences.buffer_frames,
            input_device: self.preferences.input_device.clone(),
        };
        let (engine, output, status) = match self.platform.open_output(&request) {
            Ok((engine, info)) => {
                let mut status = format!("{}, {}", info.device, describe_output(&info));
                if let Some(error) = &info.input_error {
                    status = format!("{status}. Input: {error}");
                }
                (engine, Some(info), status)
            }
            Err(first) => {
                let fallback = OutputRequest { device: None, ..request.clone() };
                match request.device.is_some().then(|| self.platform.open_output(&fallback)) {
                    Some(Ok((engine, info))) => {
                        let status = format!("{first}, now through {} ({})", info.device, describe_output(&info));
                        (engine, Some(info), status)
                    }
                    other => {
                        let reason = match other {
                            Some(Err(second)) => second,
                            _ => first,
                        };
                        let engine = self.platform.open_silent(request.sample_rate.unwrap_or(SILENT_RATE));
                        (engine, None, format!("No audio output ({reason}): the transport runs on without sound"))
                    }
                }
            }
        };
        self.status = status;
        self.output = output;
        let mut engine = engine;
        engine.locate_seconds(position);
        // The recorded audio leaves the engine through a writer of its own.
        if let Some(recorder) = engine.take_recorder() {
            let (dir, rate, tx, ctx) = (self.platform.temp_dir(), engine.sample_rate(), self.takes_tx.clone(), self.ctx.clone());
            self.platform.spawn_service(Box::new(move || recording::run_writer(recorder, dir, rate, tx, ctx)));
        }
        self.engine = Some(engine);
        self.send_mix();
        self.send_transport();
        if was_playing {
            self.play();
        }
        // The audio has to be resampled for the new rate.
        self.resample_all();
        self.next_device_check = self.now + DEVICE_CHECK_EVERY;
    }

    /// The output is not the one the user chose (it fell back, or nothing could be opened).
    fn on_fallback(&self) -> bool {
        match (&self.output, &self.preferences.output_device) {
            (None, _) => true,
            (Some(output), Some(chosen)) => &output.device != chosen,
            (Some(_), None) => false,
        }
    }

    /// Reopen the output when its device went away, and go back to the chosen device when it
    /// returns.
    fn watch_output(&mut self) {
        if self.platform.output_lost() {
            let gone = self.output.as_ref().map(|o| o.device.clone()).unwrap_or_default();
            log::warn!("audio device {gone} went away");
            self.devices = self.platform.output_devices();
            self.open_output();
            self.status = format!("{gone} went away. {}", self.status);
            return;
        }
        if !self.on_fallback() || self.now < self.next_device_check {
            return;
        }
        self.next_device_check = self.now + DEVICE_CHECK_EVERY;
        self.devices = self.platform.output_devices();
        let back = match &self.preferences.output_device {
            Some(chosen) => self.devices.iter().any(|d| &d.name == chosen),
            None => self.platform.default_output_device().is_some(),
        };
        if back {
            self.open_output();
        }
    }

    /// Change the audio output settings, remember them and reopen the output.
    fn set_output(&mut self, preferences: Preferences) {
        if preferences == self.preferences {
            return;
        }
        let reopen = preferences.output_device != self.preferences.output_device
            || preferences.sample_rate != self.preferences.sample_rate
            || preferences.buffer_frames != self.preferences.buffer_frames
            || preferences.input_device != self.preferences.input_device;
        self.preferences = preferences;
        if let Some(path) = self.platform.preferences_path()
            && let Err(e) = preferences::save(&path, &self.preferences)
        {
            log::warn!("cannot save the preferences: {e}");
        }
        if reopen {
            self.open_output();
        } else {
            // Monitoring, latency offset and the AUX and EFF outputs only need telling the engine.
            self.send_transport();
            self.send_mixer();
        }
    }

    /// What the chosen device offers, if it is in the list.
    fn chosen_device_info(&self) -> Option<&DeviceInfo> {
        let name = self.output.as_ref().map(|o| o.device.as_str());
        self.devices.iter().find(|d| Some(d.name.as_str()) == name)
    }

    fn engine_rate(&self) -> u32 {
        self.engine.as_ref().map_or(48_000, EngineHandle::sample_rate)
    }

    /// Load up to eight files onto the tracks, starting at track 1.
    fn load_all(&mut self) {
        let paths = self.platform.pick_audio_files(true);
        self.load_paths(paths);
    }

    /// Handle command-line arguments: a project folder or `project.json` opens that project,
    /// audio files go onto the tracks.
    pub fn open_arguments(&mut self, paths: Vec<PathBuf>) {
        match paths.as_slice() {
            [] => {}
            [one] if one.is_dir() || one.file_name().is_some_and(|n| n == PROJECT_FILE) => self.open_project(one),
            _ => self.load_paths(paths),
        }
    }

    fn mark_dirty(&mut self) {
        if !self.dirty {
            self.dirty = true;
            self.autosave_at = Some(self.now + AUTOSAVE_AFTER);
        }
    }

    fn project_name(&self) -> String {
        self.project_dir.as_deref().map_or_else(|| "Untitled".into(), session::project_name)
    }

    /// Put every track and the mix back to an empty project.
    fn reset(&mut self) {
        for index in 0..TRACKS {
            let vu = self.tracks[index].vu;
            // A fresh generation drops any render still running for the old project.
            let generation = self.next_generation;
            self.next_generation += 1;
            self.tracks[index] = TrackState {
                vu,
                trk_meter_db: MIN_DB,
                render_generation: generation,
                input: default_input(index),
                channel: ChannelEntry::simple(ChannelSource::Line),
                ..TrackState::default()
            };
            if let Some(engine) = &mut self.engine {
                engine.send(Command::SetTrack(index, None));
            }
        }
        self.master_db = 0.0;
        self.mixer = new_mixer();
        self.meter_post = false;
        self.zero_seconds = 0.0;
        self.cue_seconds = None;
        self.repeat = false;
        self.repeat_mode = RepeatMode::Tape;
        self.pitch_percent = 0.0;
        self.vari = false;
        self.insert = false;
        self.auto_punch = false;
        self.metronome = MetronomeSettings::default();
        self.tape = new_tape();
        self.history.clear();
        self.editor = EditorState::default();
        // Loads still running belong to sources that no longer exist, and are dropped.
        self.sources.clear();
        self.pending_imports.clear();
        self.project_generation += 1;
        if let Some(engine) = &mut self.engine {
            engine.stop();
            engine.send(Command::Locate(0.0));
        }
        self.send_mix();
        self.send_transport();
        self.project_dir = None;
        self.dirty = false;
        self.autosave_at = None;
        self.saved_label = None;
    }

    fn new_project(&mut self) {
        self.reset();
        self.status = "New project".into();
    }

    fn open_project(&mut self, path: &Path) {
        let opened = match session::open(path) {
            Ok(opened) => opened,
            Err(e) => {
                self.status = format!("Cannot open: {e}");
                return;
            }
        };
        self.reset();
        for entry in &opened.file.tracks {
            let index = usize::from(entry.number.saturating_sub(1));
            if index >= TRACKS {
                continue;
            }
            let track = &mut self.tracks[index];
            track.fader_db = entry.fader_db.clamp(MIN_DB, MAX_DB);
            track.pan = entry.pan.clamp(-1.0, 1.0);
            track.mute = entry.mute;
            track.input = entry.input;
            let mut channel = entry.channel;
            let range = trim_range(channel.source);
            channel.trim_db = channel.trim_db.clamp(*range.start(), *range.end());
            track.channel = channel;
            track.monitor = MonitorEntry {
                gain_db: entry.monitor.gain_db.clamp(MIN_DB, MONITOR_MAX_DB),
                pan: entry.monitor.pan.clamp(-1.0, 1.0),
                mute: entry.monitor.mute,
            };
            self.tracks[index].name.clone_from(&entry.name);
            for region in &entry.regions {
                let source = self.source_for(&opened.region_path(region));
                match region.length {
                    Some(length) => self.tracks[index].regions.push(Region {
                        source,
                        start: region.start,
                        offset: region.offset,
                        length,
                        name: region.name.clone(),
                    }),
                    None => self.tracks[index].unresolved = Some(Unresolved { source, name: region.name.clone() }),
                }
            }
            self.resolve_loaded(index);
            self.request_render(index);
        }
        self.master_db = opened.file.master_db.clamp(MIN_DB, MAX_DB);
        self.meter_post = opened.file.meter_post;
        let mut mixer = opened.file.mixer;
        mixer.odd_db = mixer.odd_db.clamp(MIN_DB, MAX_DB);
        mixer.even_db = mixer.even_db.clamp(MIN_DB, MAX_DB);
        self.mixer = mixer;
        let transport = opened.file.transport;
        self.zero_seconds = transport.zero_seconds.max(0.0);
        self.cue_seconds = transport.cue_seconds.map(|c| c.max(0.0));
        self.repeat_mode = transport.repeat_mode;
        self.pitch_percent = transport.pitch_percent.clamp(-PITCH_PERCENT, PITCH_PERCENT);
        self.vari = transport.vari;
        self.insert = transport.insert;
        self.auto_punch = transport.auto_punch;
        let mut metronome = opened.file.metronome;
        metronome.bpm = metronome.bpm.clamp(20.0, 400.0);
        if !TIME_SIGNATURES.contains(&(metronome.beats, metronome.unit)) {
            (metronome.beats, metronome.unit) = (4, 4);
        }
        self.metronome = metronome;
        let mut tape = opened.file.tape;
        for amount in [&mut tape.saturation, &mut tape.wow_flutter, &mut tape.noise, &mut tape.crosstalk] {
            *amount = amount.clamp(0.0, 2.0);
        }
        self.tape = tape;
        let mut editor = opened.file.editor;
        editor.height = editor.height.clamp(160.0, 2000.0);
        editor.zoom = editor.zoom.map(|z| z.clamp(timeline::ZOOM_MIN, timeline::ZOOM_MAX));
        self.editor.settings = editor;
        self.send_mix();
        self.send_transport();
        self.project_dir = Some(opened.dir);
        self.dirty = false;
        self.autosave_at = None;
        self.status = format!("{} opened", self.project_name());
        for warning in &opened.warnings {
            log::warn!("{warning}");
        }
        if let Some(first) = opened.warnings.first() {
            self.status = format!("{}; {first}", self.status);
        }
    }

    /// The pitch control as a speed ratio (1.0 in FIX).
    fn pitch_ratio(&self) -> f64 {
        if self.vari { 1.0 + f64::from(self.pitch_percent) / 100.0 } else { 1.0 }
    }

    /// Bring the engine's transport memory and settings in line with the app's.
    fn send_transport(&mut self) {
        let (zero, cue, repeat, style, pitch) =
            (self.zero_seconds, self.cue_seconds, self.repeat, style_of(self.repeat_mode), self.pitch_ratio());
        let (insert, auto_punch, monitoring) = (self.insert, self.auto_punch, self.preferences.software_monitoring);
        let offset = f64::from(self.preferences.latency_offset_ms) / 1000.0;
        let metronome = self.metronome;
        let tape = tape_params(&self.tape);
        let armed: Vec<bool> = self.tracks.iter().map(|t| t.armed).collect();
        if let Some(engine) = &mut self.engine {
            engine.set_zero_seconds(zero);
            engine.set_cue_seconds(cue);
            engine.send(Command::SetRepeat(repeat));
            engine.send(Command::SetStyle(style));
            engine.send(Command::SetPitch(pitch));
            engine.send(Command::SetInsert(insert));
            engine.send(Command::SetAutoPunch(auto_punch));
            engine.send(Command::SetMonitoring(monitoring));
            engine.send(Command::SetLatencyOffset(offset));
            engine.send(Command::SetClick(metronome.click));
            engine.send(Command::SetClickAlways(metronome.always));
            engine.send(Command::SetMeter(meter_of(&metronome)));
            engine.send(Command::SetClickVolume(db_to_gain(metronome.volume_db)));
            engine.send(Command::SetCountIn(metronome.count_in));
            engine.send(Command::SetTape(tape));
            for (index, armed) in armed.into_iter().enumerate() {
                engine.send(Command::SetArmed(index, armed));
            }
        }
    }

    /// Pick up the zero and CUE points the engine holds (RESET and CUE are set there, at the
    /// exact tape position).
    fn sync_transport(&mut self) {
        let Some(engine) = &self.engine else { return };
        let (zero, cue) = (engine.zero_seconds(), engine.cue_seconds());
        let changed = (zero - self.zero_seconds).abs() > 1e-6
            || cue.is_some() != self.cue_seconds.is_some()
            || cue.zip(self.cue_seconds).is_some_and(|(a, b)| (a - b).abs() > 1e-6);
        self.zero_seconds = zero;
        self.cue_seconds = cue;
        if changed {
            self.mark_dirty();
        }
    }

    /// Save to the project folder, asking for one first when there is none. Returns `true` when saved.
    fn save(&mut self, backup: Backup) -> bool {
        let Some(dir) = self.project_dir.clone() else { return self.save_as() };
        let tracks: Vec<TrackToSave> = self
            .tracks
            .iter()
            .map(|t| TrackToSave {
                name: t.name.clone(),
                regions: t
                    .regions
                    .iter()
                    .map(|r| RegionToSave {
                        source: self.sources.path(r.source).map(Path::to_path_buf).unwrap_or_default(),
                        start: r.start,
                        offset: r.offset,
                        length: Some(r.length),
                        name: r.name.clone(),
                    })
                    .chain(t.unresolved.iter().map(|u| RegionToSave {
                        source: self.sources.path(u.source).map(Path::to_path_buf).unwrap_or_default(),
                        start: 0.0,
                        offset: 0.0,
                        length: None,
                        name: u.name.clone(),
                    }))
                    .collect(),
                fader_db: t.fader_db,
                pan: t.pan,
                mute: t.mute,
                input: t.input,
                channel: t.channel,
                monitor: t.monitor,
            })
            .collect();
        let transport = TransportSettings {
            zero_seconds: self.zero_seconds,
            cue_seconds: self.cue_seconds,
            repeat_mode: self.repeat_mode,
            pitch_percent: self.pitch_percent,
            vari: self.vari,
            insert: self.insert,
            auto_punch: self.auto_punch,
        };
        let settings = ProjectSettings {
            master_db: self.master_db,
            meter_post: self.meter_post,
            transport,
            metronome: self.metronome,
            mixer: self.mixer,
            tape: self.tape,
            editor: self.editor.settings,
        };
        match session::save(&dir, &settings, &tracks, backup) {
            Ok(saved) => {
                self.sources.rename(&saved.sources);
                self.dirty = false;
                self.autosave_at = None;
                let what = if backup == Backup::Always { "Saved" } else { "Autosaved" };
                self.saved_label = Some(format!("{what} {}", session::clock_time()));
                true
            }
            Err(e) => {
                self.status = format!("Cannot save: {e}");
                // Try again later rather than every frame.
                self.autosave_at = Some(self.now + AUTOSAVE_AFTER);
                false
            }
        }
    }

    fn save_as(&mut self) -> bool {
        let Some(dir) = self.platform.pick_project_location(&self.project_name()) else { return false };
        self.project_dir = Some(dir);
        let saved = self.save(Backup::Always);
        if saved {
            self.status =
                format!("Project saved in {}", self.project_dir.as_deref().map(Path::display).map(|d| d.to_string()).unwrap_or_default());
        }
        saved
    }

    /// Do something that replaces the project, asking first when there are unsaved changes.
    fn request(&mut self, action: Pending) {
        if self.dirty { self.pending = Some(action) } else { self.perform(action) }
    }

    fn perform(&mut self, action: Pending) {
        match action {
            Pending::New => self.new_project(),
            Pending::Close => {
                let name = self.project_name();
                self.reset();
                self.status = format!("{name} closed");
            }
            Pending::Open(path) => {
                if let Some(path) = path.or_else(|| self.platform.pick_project_to_open()) {
                    self.open_project(&path);
                }
            }
            Pending::Quit => self.may_close = true,
        }
    }

    /// Whether the mix is already as the simple mode sets it, so switching loses nothing.
    fn routing_is_simple(&self) -> bool {
        let channels = self.tracks.iter().all(|t| {
            let c = &t.channel;
            c.auto && c.direct && c.lr && c.assign == [false; 4] && c.source != ChannelSource::Rmx && t.monitor == MonitorEntry::default()
        });
        channels && self.mixer.odd_db == 0.0 && self.mixer.even_db == 0.0 && self.mixer.mon == MonSwitch::Cue
    }

    /// The Eenvoudig | 388 switch. To the simple mode asks first when that resets routing.
    fn set_routing(&mut self, mode: Routing) {
        if mode == self.mixer.mode {
            return;
        }
        match mode {
            Routing::Classic => {
                self.mixer.mode = Routing::Classic;
                self.mark_dirty();
            }
            Routing::Simple if self.routing_is_simple() => self.apply_simple(),
            Routing::Simple => self.confirm_simple = true,
        }
    }

    /// Every channel to the simple setting; fader, pan, mute, EQ, TRIM, the sends and input stay.
    fn apply_simple(&mut self) {
        self.confirm_simple = false;
        for track in &mut self.tracks {
            let channel = ChannelEntry::simple(track.channel.source);
            let old = track.channel;
            track.channel =
                ChannelEntry { trim_db: old.trim_db, eq: old.eq, aux_db: old.aux_db, aux_pre: old.aux_pre, eff_db: old.eff_db, ..channel };
            track.monitor = MonitorEntry::default();
        }
        // Only the 388 routing goes back; MONO, the sends' masters and PHONES stay.
        self.mixer = MixerEntry { mode: Routing::Simple, odd_db: 0.0, even_db: 0.0, mon: MonSwitch::Cue, ..self.mixer };
        self.send_mix();
        self.mark_dirty();
    }

    fn confirm_simple_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_simple {
            return;
        }
        let mut answer = None;
        let response = skin::paper::paper_modal(ctx, "confirm-simple", "Switch to Simple?", 400.0, |ui| {
            ui.label("The buses, ASSIGN and the MONITOR settings go back to the simple setup. Fader, PAN, EQ, TRIM and the input stay as they are.");
            ui.add_space(14.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if skin::paper::primary_button(ui, "Continue").clicked() {
                    answer = Some(true);
                }
                if skin::paper::plain_button(ui, "Cancel").clicked() {
                    answer = Some(false);
                }
            });
        });
        if answer == Some(true) {
            self.apply_simple();
        } else if answer == Some(false) || response.should_close() {
            self.confirm_simple = false;
        }
    }

    fn unsaved_dialog(&mut self, ctx: &egui::Context) {
        let Some(action) = self.pending.clone() else { return };
        let mut answer = None;
        let mut cancel = false;
        let title = format!("Save changes to {}?", self.project_name());
        let response = skin::paper::paper_modal(ctx, "unsaved-changes", &title, 400.0, |ui| {
            ui.label("If you don't save, the changes since the last save are lost.");
            ui.add_space(14.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if skin::paper::primary_button(ui, "Save").clicked() {
                    answer = Some(true);
                }
                if skin::paper::plain_button(ui, "Don't save").clicked() {
                    answer = Some(false);
                }
                if skin::paper::plain_button(ui, "Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if cancel || (response.should_close() && answer.is_none()) {
            self.pending = None;
        }
        if let Some(save) = answer {
            self.pending = None;
            if !save || self.save(Backup::Always) {
                self.perform(action);
            }
        }
    }

    fn handle_file_shortcuts(&mut self, ui: &Ui) {
        // Full screen: Ctrl+Cmd+F as everywhere on macOS, F11 as on Windows. The app's own
        // macOS menu is off, so this is the way back out.
        if ui.input_mut(|i| i.consume_shortcut(&FULLSCREEN) || i.consume_key(Modifiers::NONE, Key::F11)) {
            toggle_fullscreen(ui.ctx());
        }
        let shortcut = |key| KeyboardShortcut::new(Modifiers::COMMAND, key);
        let save_as = KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
        let (save_as, save, open, new) = ui.input_mut(|i| {
            (
                i.consume_shortcut(&save_as),
                i.consume_shortcut(&shortcut(Key::S)),
                i.consume_shortcut(&shortcut(Key::O)),
                i.consume_shortcut(&shortcut(Key::N)),
            )
        });
        let (close, quit, settings) = ui.input_mut(|i| {
            (i.consume_shortcut(&shortcut(Key::W)), i.consume_shortcut(&shortcut(Key::Q)), i.consume_shortcut(&shortcut(Key::Comma)))
        });
        if settings {
            self.show_settings = true;
        }
        if close {
            self.request(Pending::Close);
        }
        if quit {
            self.request(Pending::Quit);
        }
        if save_as {
            self.save_as();
        } else if save {
            self.save(Backup::Always);
        }
        if open {
            self.request(Pending::Open(None));
        }
        if new {
            self.request(Pending::New);
        }
        // Shift first: consume_shortcut ignores an extra Shift, so Cmd+Z would also take Cmd+Shift+Z.
        let redo = KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
        if ui.input_mut(|i| i.consume_shortcut(&redo)) {
            self.redo();
        } else if ui.input_mut(|i| i.consume_shortcut(&shortcut(Key::Z))) {
            self.undo();
        }
    }

    fn edit_menu(&mut self, ui: &mut Ui) {
        let undo = ui.ctx().format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z));
        let redo = ui.ctx().format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z));
        ui.menu_button("Edit", |ui| {
            let undo_name = self.history.undo_name().map(str::to_string);
            let label = undo_name.as_ref().map_or_else(|| "Undo".to_string(), |n| format!("Undo {n}"));
            if ui.add_enabled(undo_name.is_some(), egui::Button::new(label).shortcut_text(undo)).clicked() {
                self.undo();
            }
            let redo_name = self.history.redo_name().map(str::to_string);
            let label = redo_name.as_ref().map_or_else(|| "Redo".to_string(), |n| format!("Redo {n}"));
            if ui.add_enabled(redo_name.is_some(), egui::Button::new(label).shortcut_text(redo)).clicked() {
                self.redo();
            }
        });
    }

    fn file_menu(&mut self, ui: &mut Ui) {
        let command = |ui: &Ui, key| ui.ctx().format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, key));
        ui.menu_button("File", |ui| {
            if ui.add(egui::Button::new("New").shortcut_text(command(ui, Key::N))).clicked() {
                self.request(Pending::New);
            }
            if ui.add(egui::Button::new("Open...").shortcut_text(command(ui, Key::O))).clicked() {
                self.request(Pending::Open(None));
            }
            ui.separator();
            if ui.add(egui::Button::new("Save").shortcut_text(command(ui, Key::S))).clicked() {
                self.save(Backup::Always);
            }
            let save_as = ui.ctx().format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S));
            if ui.add(egui::Button::new("Save as...").shortcut_text(save_as)).clicked() {
                self.save_as();
            }
            ui.separator();
            if ui.button("Load 8 audio files...").clicked() {
                self.load_all();
            }
            if ui.add(egui::Button::new("Settings...").shortcut_text(command(ui, Key::Comma))).clicked() {
                self.show_settings = true;
            }
            if ui.button("Tape settings...").clicked() {
                self.show_tape = true;
            }
            if ui.button("Export...").clicked() {
                self.show_export = true;
            }
            ui.separator();
            if ui.add(egui::Button::new("Full screen").shortcut_text(ui.ctx().format_shortcut(&FULLSCREEN))).clicked() {
                toggle_fullscreen(ui.ctx());
            }
            ui.separator();
            if ui.add(egui::Button::new("Close project").shortcut_text(command(ui, Key::W))).clicked() {
                self.request(Pending::Close);
            }
            if ui.add(egui::Button::new("Quit DAW 388").shortcut_text(command(ui, Key::Q))).clicked() {
                self.request(Pending::Quit);
            }
        });
    }

    fn engine_send(&mut self, command: Command) {
        if let Some(engine) = &mut self.engine {
            engine.send(command);
        }
    }

    fn send_channel(&mut self, index: usize) {
        let params = channel_params(&self.tracks[index]);
        self.engine_send(Command::SetChannel(index, params));
    }

    fn send_monitor(&mut self, index: usize) {
        let params = monitor_params(&self.tracks[index].monitor);
        self.engine_send(Command::SetMonitor(index, params));
    }

    fn send_mixer(&mut self) {
        let output_channels = self.output.as_ref().map_or(0, |o| o.channels);
        let params = mixer_params(&self.mixer, &self.preferences, output_channels);
        self.engine_send(Command::SetMixer(params));
    }

    /// Bring the engine's whole mixer in line with the app's.
    fn send_mix(&mut self) {
        for index in 0..TRACKS {
            self.send_channel(index);
            self.send_monitor(index);
            let solo = self.tracks[index].solo;
            self.engine_send(Command::SetSolo(index, solo));
        }
        self.send_mixer();
        self.engine_send(Command::SetMaster(db_to_gain(self.master_db)));
    }

    /// Whether an armed track gets anything to record: DIRECT from a channel with an input, or
    /// a channel assigned to its bus. Only for a warning, so it does not look at PAN or at
    /// whether a RMX channel's track has audio.
    fn feeds(&self, index: usize) -> bool {
        let track = &self.tracks[index];
        if track.channel.direct {
            (track.channel.source != ChannelSource::Rmx || track.channel.auto) && track.input.is_some()
        } else {
            self.tracks.iter().any(|t| t.channel.assign[index / 2])
        }
    }

    fn play(&mut self) {
        self.engine_send(Command::Play);
    }

    /// On the 388 every transport key except PLAY and RTZ switches REPEAT off; in cycle mode it
    /// stays on, as in Logic.
    fn transport_key_pressed(&mut self) {
        if self.repeat && self.repeat_mode == RepeatMode::Tape {
            self.set_repeat(false);
        }
    }

    fn stop(&mut self) {
        self.transport_key_pressed();
        self.engine_send(Command::Stop);
    }

    fn rewind(&mut self) {
        self.transport_key_pressed();
        self.engine_send(Command::Rewind);
    }

    fn fast_forward(&mut self) {
        self.transport_key_pressed();
        self.engine_send(Command::FastForward);
    }

    fn rtz(&mut self) {
        self.engine_send(Command::ReturnToZero);
    }

    fn search_to_cue(&mut self) {
        self.transport_key_pressed();
        self.engine_send(Command::SearchToCue);
    }

    fn reset_zero(&mut self) {
        self.engine_send(Command::ResetZero);
    }

    fn mark_cue(&mut self) {
        self.engine_send(Command::MarkCue);
    }

    fn set_repeat(&mut self, on: bool) {
        self.repeat = on;
        self.engine_send(Command::SetRepeat(on));
        if on && self.cue_seconds.is_none_or(|cue| cue <= self.zero_seconds) {
            self.status = "REPEAT runs from the zero point to the CUE point: set a CUE point after the zero point first".into();
        }
    }

    fn set_repeat_mode(&mut self, mode: RepeatMode) {
        if mode != self.repeat_mode {
            self.repeat_mode = mode;
            self.engine_send(Command::SetStyle(style_of(mode)));
            self.mark_dirty();
        }
    }

    fn send_pitch(&mut self) {
        let pitch = self.pitch_ratio();
        self.engine_send(Command::SetPitch(pitch));
        self.mark_dirty();
    }

    /// REC FUNCTION for a track: ready to record, or safe.
    fn toggle_arm(&mut self, index: usize) {
        let armed = !self.tracks[index].armed;
        if armed && self.refuse_unresolved(index) {
            return;
        }
        self.tracks[index].armed = armed;
        self.engine_send(Command::SetArmed(index, armed));
        if armed && !self.feeds(index) && self.mixer.mode == Routing::Simple {
            self.status = format!("Track {} receives nothing: choose an input", index + 1);
        } else if armed && !self.feeds(index) {
            self.status = format!(
                "Track {} receives nothing: switch on DIRECT with an input (MIC or LINE), or ASSIGN a channel to bus {}",
                index + 1,
                index + 1
            );
        }
    }

    /// REC: check what recording needs, then start it.
    fn record(&mut self) {
        if self.project_dir.is_none() {
            self.status = "Save the project first: the take goes into the project folder".into();
            if !self.save_as() {
                return;
            }
        }
        let silent: Vec<String> = (0..TRACKS).filter(|&i| self.tracks[i].armed && !self.feeds(i)).map(|i| (i + 1).to_string()).collect();
        if !silent.is_empty() {
            self.status = format!("Note: track {} receives nothing and records silence", silent.join(", "));
        }
        if self.vari && self.pitch_percent.abs() > 0.001 && silent.is_empty() {
            // As on tape: what is recorded fast plays back slower and lower, and the other way round.
            let (way, sound) = if self.pitch_percent > 0.0 { ("+", "slower and lower") } else { ("", "faster and higher") };
            self.status = format!("Recording at {way}{:.1}%: plays back {sound} at normal speed", self.pitch_percent);
        }
        self.engine_send(Command::Record);
    }

    fn set_insert(&mut self, insert: bool) {
        self.insert = insert;
        self.engine_send(Command::SetInsert(insert));
        self.mark_dirty();
    }

    fn set_auto_punch(&mut self, auto: bool) {
        self.auto_punch = auto;
        self.engine_send(Command::SetAutoPunch(auto));
        self.mark_dirty();
        if auto && self.cue_seconds.is_none_or(|cue| cue <= self.zero_seconds) {
            self.status = "AUTO PUNCH records from the zero point to the CUE point: set a CUE point after the zero point first".into();
        }
    }

    /// Pick up takes the writer finished and queue them for their tracks.
    fn poll_takes(&mut self) {
        while let Ok(take) = self.takes_rx.try_recv() {
            if take.frames == 0 {
                let _ = std::fs::remove_file(&take.path);
                continue;
            }
            let track = take.track;
            if track < TRACKS {
                self.queued_takes[track].push_back(take);
                self.start_compose(track);
            }
        }
        if self.engine.as_ref().is_some_and(EngineHandle::take_overflow) {
            self.status = "Note: the recording stuttered, some audio was lost".into();
        }
    }

    /// Change the metronome and tell the engine.
    fn set_metronome(&mut self, metronome: MetronomeSettings) {
        if metronome == self.metronome {
            return;
        }
        self.metronome = metronome;
        self.engine_send(Command::SetClick(metronome.click));
        self.engine_send(Command::SetClickAlways(metronome.always));
        self.engine_send(Command::SetMeter(meter_of(&metronome)));
        self.engine_send(Command::SetClickVolume(db_to_gain(metronome.volume_db)));
        self.engine_send(Command::SetCountIn(metronome.count_in));
        self.mark_dirty();
    }

    /// TAP: the tempo of the last presses, once there are at least two close enough together.
    fn tap(&mut self) -> Option<f64> {
        let now = self.now;
        // A pause of more than two seconds starts a new count.
        if self.taps.last().is_some_and(|last| now - last > 2.0) {
            self.taps.clear();
        }
        self.taps.push(now);
        if self.taps.len() > 5 {
            self.taps.remove(0);
        }
        let (first, last) = (self.taps.first()?, self.taps.last()?);
        let intervals = self.taps.len() - 1;
        if intervals == 0 {
            return None;
        }
        let beat = (last - first) / intervals as f64;
        // A tap is a beat of the time signature; the tempo counts quarter notes.
        let quarter = beat * f64::from(self.metronome.unit) / 4.0;
        Some((60.0 / quarter).clamp(20.0, 400.0))
    }

    fn metronome_settings(&mut self, ui: &mut Ui) {
        skin::paper::section_title(ui, "Metronome");
        ui.add_space(4.0);
        let mut metronome = self.metronome;
        egui::Grid::new("metronome-settings").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Volume");
            ui.add(egui::Slider::new(&mut metronome.volume_db, -40.0..=6.0).suffix(" dB").fixed_decimals(1));
            ui.end_row();
            ui.label("Count-in");
            let text = |bars: u8| match bars {
                0 => "Off".to_string(),
                1 => "1 bar".to_string(),
                n => format!("{n} bars"),
            };
            egui::ComboBox::from_id_salt("count-in").selected_text(text(metronome.count_in)).width(120.0).show_ui(ui, |ui| {
                for bars in [0, 1, 2] {
                    ui.selectable_value(&mut metronome.count_in, bars, text(bars));
                }
            });
            ui.end_row();
            ui.label("Click");
            skin::paper::check(ui, &mut metronome.always, "Also during normal playback (otherwise only when recording)");
            ui.end_row();
        });
        ui.label(
            RichText::new(
                "Bar 1 starts at the beginning of the tape, so RESET does not shift the rhythm. The click is only heard \
                 and never ends up in a recording. The count-in happens on REC from stop; the tape waits, then starts on the beat. \
                 Tempo, time signature and these settings are saved with the project.",
            )
            .size(11.0)
            .color(skin::paper::INK_SOFT),
        );
        self.set_metronome(metronome);
    }

    fn update_meters(&mut self, dt: f32) {
        let Some(engine) = &mut self.engine else { return };
        engine.collect_garbage();
        // VU-like fall of about 20 dB per second for the TRK bars of the MONITOR section.
        let fall = 20.0 * dt;
        let now = self.now;
        let classic = self.mixer.mode == Routing::Classic;
        for (index, track) in self.tracks.iter_mut().enumerate() {
            let peaks = engine.take_track_peaks(index);
            // The PRE peak lies after the EQ: where the 388's OL looks.
            if overload(peaks.pre) {
                track.ol_until = now + OL_HOLD;
            }
            let energy = engine.take_track_energy(index);
            let (trk_peak, trk_energy) = (engine.take_trk_peak(index), engine.take_trk_energy(index));
            // On the 388 the meter of track N is its TRK/PGM meter: what its MONITOR hears (the
            // bus going to the track, or the tape). The simple mode has no MONITOR section: there
            // it shows the channel, before or after the fader.
            let (level, peak) = if classic {
                (trk_energy, trk_peak)
            } else if self.meter_post {
                (energy.post, peaks.post)
            } else {
                (energy.pre, peaks.pre)
            };
            track.vu.step(dt, skin::vu::target(level));
            if peak > VU_PEAK {
                track.peak_until = now + OL_HOLD;
            }
            let trk = gain_to_db(trk_peak);
            track.trk_meter_db = trk.max(track.trk_meter_db - fall);
        }
        // Both sources are read every frame, so the one not shown does not pile up.
        let (master, sends) = (engine.take_master_energy(), engine.take_send_energy());
        let (master_peaks, send_peaks) = (engine.take_master_peaks(), engine.take_send_peaks());
        let (energy, peaks) = if self.mixer.meter == MeterSwitch::AuxEff { (sends, send_peaks) } else { (master, master_peaks) };
        for side in 0..2 {
            if peaks[side] > VU_PEAK {
                self.big_ol_until[side] = now + OL_HOLD;
            }
            self.big_vu[side].step(dt, skin::vu::target(energy[side]));
        }
        let position = engine.position_seconds();
        let length = self.tracks.iter().map(|t| track_end(&t.regions)).fold(0.0, f64::max);
        self.reels.advance(position, length, dt);
    }

    fn handle_keys(&mut self, ui: &Ui) {
        if ui.memory(|m| m.focused().is_some()) {
            return;
        }
        let keys = ui.input(|i| {
            [Key::Space, Key::Home, Key::C, Key::Comma, Key::Period, Key::R, Key::K].map(|k| i.key_pressed(k) && i.modifiers.is_none())
        });
        let [space, home, c, comma, period, r, k] = keys;
        if k {
            let mut metronome = self.metronome;
            metronome.click = !metronome.click;
            self.set_metronome(metronome);
        }
        if r {
            self.record();
        }
        if space {
            if self.engine.as_ref().is_some_and(|e| e.motion() != Motion::Stopped) { self.stop() } else { self.play() }
        }
        if home {
            self.rtz();
        }
        if c {
            self.set_repeat(!self.repeat);
        }
        if comma {
            self.rewind();
        }
        if period {
            self.fast_forward();
        }
    }

    /// The header: File and Edit on the left, the project in the middle, status, saved and full
    /// screen on the right. The output device lives in Settings; Simple / 388 on the console.
    fn header(&mut self, ui: &mut Ui) {
        let full = ui.max_rect();
        ui.horizontal(|ui| {
            // Menus as plain text that lights up under the mouse, no button frames.
            ui.scope(|ui| {
                let widgets = &mut ui.visuals_mut().widgets;
                for state in [&mut widgets.inactive, &mut widgets.hovered, &mut widgets.active, &mut widgets.open] {
                    state.weak_bg_fill = Color32::TRANSPARENT;
                    state.bg_stroke = Stroke::NONE;
                }
                widgets.inactive.fg_stroke = Stroke::new(1.0, palette::TEXT);
                widgets.hovered.fg_stroke = Stroke::new(1.0, palette::CREAM);
                self.file_menu(ui);
                self.edit_menu(ui);
            });
            let name = format!("{}{}", self.project_name(), if self.dirty { " (modified)" } else { "" });
            ui.painter().text(full.center(), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(12.0), palette::TEXT_DIM);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if fullscreen_icon(ui).clicked() {
                    toggle_fullscreen(ui.ctx());
                }
                if let Some(saved) = &self.saved_label
                    && !self.dirty
                {
                    ui.add_space(6.0);
                    ui.label(RichText::new(saved).color(palette::GREEN).size(11.0));
                }
                // The status keeps clear of the project name in the middle.
                let room = (ui.available_width() - full.width() / 2.0 - 120.0).max(0.0);
                let colour = if self.engine.is_some() { palette::TEXT_DIM } else { palette::LED };
                ui.add_sized(vec2(room, 16.0), egui::Label::new(RichText::new(&self.status).color(colour).size(11.0)).truncate());
            });
        });
    }

    /// The LED counter: min:sec, the CUE point, the pitch and the bar and beat.
    fn led_counter(&self, ui: &mut Ui) {
        let (counter, motion) = self.engine.as_ref().map_or((0.0, Motion::Stopped), |e| (e.counter_seconds(), e.motion()));
        let cue_counter = self.cue_seconds.map(|cue| cue - self.zero_seconds);
        let (tape, count_in) = self.engine.as_ref().map_or((0.0, 0.0), |e| (e.position_seconds(), e.count_in_seconds()));
        let (bar, beat) = meter_of(&self.metronome).bar_and_beat(tape - count_in);
        Frame::new()
            .fill(Color32::from_rgb(0x12, 0x08, 0x07))
            .corner_radius(CornerRadius::same(4))
            .inner_margin(Margin::symmetric(14, 6))
            .stroke(Stroke::new(2.0, palette::EDGE))
            .show(ui, |ui| {
                ui.set_width(200.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(format_counter(counter)).monospace().size(34.0).color(palette::LED));
                    let cue = cue_counter.map_or("CUE  --:--.-".to_string(), |c| format!("CUE  {}", format_counter(c)));
                    let pitch = if self.vari { format!("   PITCH {:+.1}%", self.pitch_percent) } else { String::new() };
                    ui.label(RichText::new(format!("{cue}{pitch}")).monospace().size(11.0).color(Color32::from_rgb(0xc8, 0x55, 0x3c)));
                    let bars = if motion == Motion::CountingIn {
                        format!("COUNT-IN  {}.{}", bar, beat)
                    } else {
                        format!("BAR  {bar}.{beat}   {}/{}", self.metronome.beats, self.metronome.unit)
                    };
                    ui.label(RichText::new(bars).monospace().size(11.0).color(Color32::from_rgb(0xc8, 0x55, 0x3c)));
                });
            });
    }

    /// REW, F FWD, STOP, PLAY and REC.
    fn transport_keys(&mut self, ui: &mut Ui) {
        let motion = self.engine.as_ref().map_or(Motion::Stopped, EngineHandle::motion);
        let rec_mode = self.engine.as_ref().map_or(RecMode::Off, EngineHandle::rec_mode);
        let blink = (self.now * 2.0).fract() < 0.5;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let big =
                |label, symbol, lit| KeyCap::new(label, keys::LARGE).colour(KeyColour::Grey).symbol(symbol).lamp(lit, palette::LED_GREEN);
            if big("REW", Symbol::Rewind, motion == Motion::Rewinding).show(ui).on_hover_text("Rewind (,)").clicked() {
                self.rewind();
            }
            if big("F FWD", Symbol::Forward, motion == Motion::Forwarding).show(ui).on_hover_text("Fast forward (.)").clicked() {
                self.fast_forward();
            }
            let stop = KeyCap::new("STOP", keys::LARGE).colour(KeyColour::Grey).symbol(Symbol::Stop);
            if stop.show(ui).on_hover_text("Stop (space)").clicked() {
                self.stop();
            }
            if big("PLAY", Symbol::Play, motion == Motion::Playing)
                .show(ui)
                .on_hover_text("Play; while recording: punch-out (space)")
                .clicked()
            {
                self.play();
            }
            // REC: blinks when record-ready, lit while recording.
            let lit = rec_mode == RecMode::Recording || (rec_mode == RecMode::Ready && blink);
            let rec = KeyCap::new("REC", keys::LARGE).colour(KeyColour::Grey).symbol(Symbol::Record).lamp(lit, palette::LED);
            if rec.show(ui).on_hover_text("Record on the tracks with REC FUNCTION (R); while playing: punch-in").clicked() {
                self.record();
            }
        });
    }

    /// The tape deck's keys on five columns, the columns of the large transport keys: the
    /// function keys above them, pitch, tape and export under them, tempo and metre at the bottom.
    fn transport(&mut self, ui: &mut Ui) {
        use console::{place, tag};
        let motion = self.engine.as_ref().map_or(Motion::Stopped, EngineHandle::motion);
        let (tape, count_in) = self.engine.as_ref().map_or((0.0, 0.0), |e| (e.position_seconds(), e.count_in_seconds()));
        let meter = meter_of(&self.metronome);
        // The tape's bar grid; during the count-in it runs ahead of the waiting tape.
        let grid = tape - count_in;
        let on_beat = (grid / meter.beat_seconds()).fract() < 0.15
            && (motion == Motion::CountingIn || (motion == Motion::Playing && self.metronome.click));
        // A small key with its label above it.
        const SMALL_H: f32 = 33.0;
        let key_box = vec2(48.0, SMALL_H);
        column(ui, 262.0, |ui| {
            self.led_counter(ui);
            ui.add_space(10.0);
            let (block, _) = ui.allocate_exact_size(vec2(262.0, 214.0), Sense::hover());
            let top = block.top();
            // The five columns of the large keys (44 wide, 6 apart).
            let col = |i: usize| block.center().x + (i as f32 - 2.0) * 50.0;

            // Row 1: the memory keys and RTZ.
            let repeat_tip = match self.repeat_mode {
                RepeatMode::Tape => "Repeat from the zero point to the CUE point; the tape rewinds each time (C)",
                RepeatMode::Cycle => "Repeat from the zero point to the CUE point; jumps straight back (C)",
            };
            if place(ui, col(0), top, key_box, |ui| KeyCap::new("RESET", keys::SMALL).show(ui))
                .on_hover_text("Set the counter to 00:00: the zero point is here")
                .clicked()
            {
                self.reset_zero();
            }
            if place(ui, col(1), top, key_box, |ui| KeyCap::new("CUE", keys::SMALL).show(ui))
                .on_hover_text("Remember this spot as the CUE point")
                .clicked()
            {
                self.mark_cue();
            }
            if place(ui, col(2), top, key_box, |ui| KeyCap::new("STC", keys::SMALL).show(ui))
                .on_hover_text("To the CUE point (Search To Cue)")
                .clicked()
            {
                self.search_to_cue();
            }
            let repeat = self.repeat;
            if place(ui, col(3), top, key_box, |ui| KeyCap::new("REPEAT", keys::SMALL).lamp(repeat, palette::LED_GREEN).show(ui))
                .on_hover_text(repeat_tip)
                .clicked()
            {
                self.set_repeat(!repeat);
            }
            if place(ui, col(4), top, key_box, |ui| KeyCap::new("RTZ", keys::SMALL).show(ui))
                .on_hover_text("Back to the zero point (Home)")
                .clicked()
            {
                self.rtz();
            }

            // Row 2: recording and the click.
            let row = top + 38.0;
            let insert = self.insert;
            if place(ui, col(0), row, key_box, |ui| KeyCap::new("INSERT", keys::SMALL).lamp(insert, palette::LED_GREEN).show(ui))
                .on_hover_text("Armed tracks play the tape until the punch-in, then switch to the input")
                .clicked()
            {
                self.set_insert(!insert);
            }
            let punch = self.auto_punch;
            if place(ui, col(1), row, key_box, |ui| KeyCap::new("PUNCH", keys::SMALL).lamp(punch, palette::LED_GREEN).show(ui))
                .on_hover_text("AUTO PUNCH: after REC, record only from the zero point to the CUE point (with REPEAT, again and again)")
                .clicked()
            {
                self.set_auto_punch(!punch);
            }
            let mut metronome = self.metronome;
            // Flashes on the beat while the tape runs or counts in; steady while stopped.
            let lit = if matches!(motion, Motion::Playing | Motion::CountingIn) { on_beat } else { metronome.click };
            if place(ui, col(2), row, key_box, |ui| KeyCap::new("CLICK", keys::SMALL).lamp(lit, palette::LED_ORANGE).show(ui))
                .on_hover_text("Metronome on or off (K); the LED ticks along")
                .clicked()
            {
                metronome.click = !metronome.click;
            }
            if place(ui, col(3), row, key_box, |ui| KeyCap::new("TAP", keys::SMALL).show(ui))
                .on_hover_text("Tap a few times in tempo")
                .clicked()
                && let Some(bpm) = self.tap()
            {
                metronome.bpm = bpm;
            }

            // Row 3: the large transport keys.
            let keys_row = egui::Rect::from_min_size(egui::pos2(col(0) - 22.0, top + 80.0), vec2(244.0, 47.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(keys_row), |ui| self.transport_keys(ui));

            // Row 4: pitch, tape and export.
            let row = top + 136.0;
            tag(ui, egui::pos2(col(0), row + 5.0), egui::Align2::CENTER_CENTER, "PITCH");
            // The speed is fixed while recording: the knob and VARI/FIX wait for the take to end.
            let recording = self.engine.as_ref().is_some_and(|e| e.rec_mode() != RecMode::Off);
            let vari = self.vari;
            let knob = place(ui, col(0), row + 13.0, vec2(30.0, 30.0), |ui| {
                knob(ui, &mut self.pitch_percent, -PITCH_PERCENT..=PITCH_PERCENT, 0.0, palette::CREAM, 30.0, vari && !recording)
            })
            .on_hover_text("PITCH: speed and pitch, plus or minus 15% (double-click: 0%)");
            if knob.changed() || knob.double_clicked() {
                self.send_pitch();
            }
            for (i, (is_vari, label, tip)) in
                [(true, "VARI", "The PITCH knob works"), (false, "FIX", "Normal speed, the PITCH knob is fixed")].into_iter().enumerate()
            {
                let on = vari == is_vari;
                if place(ui, col(1 + i), row, key_box, |ui| {
                    ui.add_enabled_ui(!recording, |ui| KeyCap::new(label, keys::SMALL).lamp(on, palette::LED_GREEN).show(ui)).inner
                })
                .on_hover_text(tip)
                .clicked()
                    && !on
                {
                    self.vari = is_vari;
                    self.send_pitch();
                }
            }
            let machine = self.tape.mode == TapeMode::Machine;
            let tip = format!("{}: the character of the tape on playback (speed, type, saturation, wow, noise)", tape_label(&self.tape));
            if place(ui, col(3), row, key_box, |ui| KeyCap::new("TAPE", keys::SMALL).lamp(machine, palette::LED_GREEN).show(ui))
                .on_hover_text(tip)
                .clicked()
            {
                self.show_tape = true;
            }
            let running = self.running_export.is_some();
            if place(ui, col(4), row, key_box, |ui| KeyCap::new("EXPORT", keys::SMALL).lamp(running, palette::LED_GREEN).show(ui))
                .on_hover_text("Stereo bounce and separate tracks as WAV or AIFF")
                .clicked()
            {
                self.show_export = true;
            }

            // Row 5: tempo and metre.
            let row = top + 186.0;
            tag(ui, egui::pos2(col(0) - 22.0, row + 10.0), egui::Align2::LEFT_CENTER, "TEMPO");
            place(ui, (col(1) + col(2)) / 2.0 - 12.0, row, vec2(84.0, 20.0), |ui| {
                keys::led_style(ui);
                ui.add(egui::DragValue::new(&mut metronome.bpm).range(20.0..=400.0).speed(0.2).fixed_decimals(1).suffix(" BPM"))
                    .on_hover_text("Tempo in quarter notes per minute (drag, or click and type)");
            });
            tag(ui, egui::pos2(col(3) - 14.0, row + 10.0), egui::Align2::RIGHT_CENTER, "METER");
            place(ui, col(3) + 18.0, row, vec2(56.0, 20.0), |ui| {
                keys::led_style(ui);
                let signature = format!("{}/{}", metronome.beats, metronome.unit);
                egui::ComboBox::from_id_salt("time-signature").selected_text(signature).width(48.0).show_ui(ui, |ui| {
                    for (beats, unit) in TIME_SIGNATURES {
                        let chosen = (metronome.beats, metronome.unit) == (beats, unit);
                        if ui.selectable_label(chosen, format!("{beats}/{unit}")).clicked() {
                            (metronome.beats, metronome.unit) = (beats, unit);
                        }
                    }
                });
            });
            self.set_metronome(metronome);
        });
    }

    /// The output device chooser. Returns the new choice (`Some(None)`: the default device).
    fn device_combo(&self, ui: &mut Ui, id: &str) -> Option<Option<String>> {
        let selected = match (&self.output, &self.preferences.output_device) {
            (Some(output), None) => format!("Default ({})", output.device),
            (Some(output), Some(_)) => output.device.clone(),
            (None, _) => "None (silent clock)".into(),
        };
        let mut choice = None;
        egui::ComboBox::from_id_salt(id).selected_text(selected).width(260.0).show_ui(ui, |ui| {
            if ui.selectable_label(self.preferences.output_device.is_none(), "System default device").clicked() {
                choice = Some(None);
            }
            for device in &self.devices {
                let chosen = self.preferences.output_device.as_deref() == Some(device.name.as_str());
                if ui.selectable_label(chosen, &device.name).clicked() {
                    choice = Some(Some(device.name.clone()));
                }
            }
        });
        choice
    }

    fn audio_settings(&mut self, ui: &mut Ui) {
        skin::paper::section_title(ui, "Audio");
        ui.add_space(4.0);
        let mut wanted = self.preferences.clone();
        egui::Grid::new("audio-settings").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Output");
            if let Some(choice) = self.device_combo(ui, "settings-output-device") {
                wanted.output_device = choice;
            }
            ui.end_row();

            let info = self.chosen_device_info().cloned().unwrap_or_default();
            ui.label("Sample rate");
            let rate_text = |rate: Option<u32>| rate.map_or("From the device".to_string(), |r| format!("{:.1} kHz", f64::from(r) / 1000.0));
            egui::ComboBox::from_id_salt("sample-rate").selected_text(rate_text(wanted.sample_rate)).width(260.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut wanted.sample_rate, None, rate_text(None));
                for rate in &info.sample_rates {
                    ui.selectable_value(&mut wanted.sample_rate, Some(*rate), rate_text(Some(*rate)));
                }
            });
            ui.end_row();

            ui.label("Buffer");
            let buffer_text = |frames: Option<u32>| frames.map_or("Device default".to_string(), |n| format!("{n} samples"));
            egui::ComboBox::from_id_salt("buffer-size").selected_text(buffer_text(wanted.buffer_frames)).width(260.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut wanted.buffer_frames, None, buffer_text(None));
                for frames in BUFFER_SIZES.iter().copied().filter(|n| info.buffer_range.is_none_or(|(min, max)| (min..=max).contains(n))) {
                    ui.selectable_value(&mut wanted.buffer_frames, Some(frames), buffer_text(Some(frames)));
                }
            });
            ui.end_row();

            ui.label("Input");
            let input_text = |name: &Option<String>| name.clone().unwrap_or_else(|| "None (no recording)".into());
            egui::ComboBox::from_id_salt("input-device").selected_text(input_text(&wanted.input_device)).width(260.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut wanted.input_device, None, input_text(&None));
                for device in &self.input_devices {
                    let label = format!("{} ({} inputs)", device.name, device.channels);
                    ui.selectable_value(&mut wanted.input_device, Some(device.name.clone()), label);
                }
            });
            ui.end_row();

            ui.label("Monitoring");
            skin::paper::check(ui, &mut wanted.software_monitoring, "Through DAW 388 (off: direct monitoring on the interface)");
            ui.end_row();

            // AUX and EFF to free outputs of the interface; 1 and 2 carry the main output.
            let output_channels = self.output.as_ref().map_or(0, |o| o.channels);
            let output_text = |c: Option<u16>| match c {
                None => "None".to_string(),
                Some(c) if c > output_channels => format!("Output {c} (not on this device)"),
                Some(c) => format!("Output {c}"),
            };
            for (label, id, choice) in
                [("AUX output", "aux-output", &mut wanted.aux_output), ("EFF output", "eff-output", &mut wanted.eff_output)]
            {
                ui.label(label);
                egui::ComboBox::from_id_salt(id).selected_text(output_text(*choice)).width(260.0).show_ui(ui, |ui| {
                    ui.selectable_value(choice, None, output_text(None));
                    for c in 3..=output_channels {
                        ui.selectable_value(choice, Some(c), output_text(Some(c)));
                    }
                });
                ui.end_row();
            }

            ui.label("Latency");
            ui.horizontal(|ui| {
                let automatic =
                    self.engine.as_ref().map_or(0.0, |e| e.compensation_seconds() * 1000.0) - f64::from(wanted.latency_offset_ms);
                ui.label(format!("automatic {automatic:.1} ms, plus"));
                ui.add(egui::DragValue::new(&mut wanted.latency_offset_ms).range(-100.0..=100.0).speed(0.1).suffix(" ms"));
            });
            ui.end_row();
        });
        let channels = self.output.as_ref().and_then(|o| o.input.as_ref()).map_or(0, |i| i.channels);
        if channels > 0 {
            ui.add_space(4.0);
            ui.label(RichText::new("Input per channel (MIC and LINE)").size(12.0));
            let mut changed = Vec::new();
            egui::Grid::new("track-inputs").num_columns(4).spacing([10.0, 4.0]).show(ui, |ui| {
                for (index, track) in self.tracks.iter().enumerate() {
                    let mut input = track.input;
                    let text = |c: Option<u16>| c.map_or("none".to_string(), |c| format!("in {c}"));
                    ui.label(format!("Channel {}", index + 1));
                    egui::ComboBox::from_id_salt(("track-input", index)).selected_text(text(input)).width(70.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut input, None, text(None));
                        for c in 1..=channels {
                            ui.selectable_value(&mut input, Some(c), text(Some(c)));
                        }
                    });
                    if input != track.input {
                        changed.push((index, input));
                    }
                    if index % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            for (index, input) in changed {
                self.tracks[index].input = input;
                self.send_channel(index);
                self.mark_dirty();
            }
        }
        let now = match &self.output {
            Some(output) => {
                let input = match (&output.input, &output.input_error) {
                    (Some(input), _) => format!("; input {} ({} channels)", input.device, input.channels),
                    (None, Some(error)) => format!("; input: {error}"),
                    (None, None) => String::new(),
                };
                format!("Now: {}, {}{input}", output.device, describe_output(output))
            }
            None => "Now: no audio output, the transport runs on without sound".into(),
        };
        ui.label(RichText::new(now).size(11.0).color(skin::paper::INK_SOFT));
        ui.label(
            RichText::new(
                "A smaller buffer gives less latency but asks more of the computer. If the device goes away, \
                 DAW 388 switches to the default device and back as soon as it returns. These settings apply \
                 to the app, not per project.",
            )
            .size(11.0)
            .color(skin::paper::INK_SOFT),
        );
        self.set_output(wanted);
    }

    /// The export as the app stands now, or why it cannot start.
    fn export_job(&self) -> Result<ExportJob, String> {
        let Some(dir) = &self.project_dir else {
            return Err("Save the project first: the export goes into the project's export folder".into());
        };
        let choices = &self.export;
        let tracks: [ExportTrack; TRACKS] =
            std::array::from_fn(|i| ExportTrack { name: self.tracks[i].name.clone(), regions: self.tracks[i].regions.clone() });
        let sources = tracks
            .iter()
            .flat_map(|t| &t.regions)
            .filter_map(|r| self.sources.get(r.source).and_then(|s| s.clip.clone()).map(|clip| (r.source, clip)))
            .collect();
        let longest = tracks.iter().map(|t| track_end(&t.regions)).fold(0.0, f64::max);
        if longest <= 0.0 {
            return Err("There is nothing on the tape to export yet".into());
        }
        let range = if choices.to_cue {
            let end = self.cue_seconds.filter(|cue| *cue > self.zero_seconds).unwrap_or(longest);
            (self.zero_seconds, end)
        } else {
            (0.0, longest)
        };
        let stems: Vec<(usize, StemKind)> = if choices.stems {
            (0..TRACKS).filter(|i| choices.stem_tracks[*i] && !tracks[*i].regions.is_empty()).map(|i| (i, choices.stem_kind)).collect()
        } else {
            Vec::new()
        };
        if !choices.stereo && stems.is_empty() {
            return Err("Choose a stereo bounce or separate tracks".into());
        }
        Ok(ExportJob {
            rate: choices.rate,
            tracks,
            sources,
            channels: std::array::from_fn(|i| channel_params(&self.tracks[i])),
            monitors: std::array::from_fn(|i| monitor_params(&self.tracks[i].monitor)),
            mixer: mixer_params(&self.mixer, &self.preferences, 2),
            master: db_to_gain(self.master_db),
            tape: tape_params(&self.tape),
            meter: meter_of(&self.metronome),
            range,
            stereo: choices.stereo.then_some(choices.stereo_tape),
            stems,
            from_zero: choices.from_zero,
            dir: dir.join("export"),
            project: self.project_name(),
            format: choices.format,
            depth: choices.depth,
        })
    }

    fn start_export(&mut self) {
        let job = match self.export_job() {
            Ok(job) => job,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        let progress = Arc::new(Progress::default());
        let (tx, rx) = channel();
        let shared = Arc::clone(&progress);
        self.running_export = Some(RunningExport { progress, result: rx });
        self.status = "Exporting...".into();
        self.platform.spawn(Box::new(move || {
            let _ = tx.send(export::run(&job, &shared));
        }));
    }

    /// Pick up a finished export.
    fn poll_export(&mut self) {
        let Some(running) = &self.running_export else { return };
        let Ok(result) = running.result.try_recv() else { return };
        self.running_export = None;
        self.status = match result {
            Ok(files) => {
                let dir = files.first().and_then(|f| f.parent()).map(|d| d.display().to_string()).unwrap_or_default();
                format!("{} file(s) exported to {dir}", files.len())
            }
            Err(e) => e,
        };
    }

    fn export_window(&mut self, ctx: &egui::Context) {
        use skin::paper::{self, check, note, plain_button, primary_button, radio, section_title, toggle_cards};
        let mut open = self.show_export;
        let mut choices = self.export.clone();
        let mut start = false;
        let names: [String; TRACKS] =
            std::array::from_fn(|i| if self.tracks[i].name.is_empty() { "empty".to_string() } else { self.tracks[i].name.clone() });
        let has: [bool; TRACKS] = std::array::from_fn(|i| !self.tracks[i].regions.is_empty());
        let running = self.running_export.as_ref().map(|r| Arc::clone(&r.progress));
        let machine = self.tape.mode == TapeMode::Machine;
        let project = self.project_name();
        let place = self.project_dir.as_ref().map_or("Save the project first".to_string(), |d| d.join("export").display().to_string());
        paper::paper_window(ctx, "export-window", "Export", "", &mut open, 460.0, |ui| {
            ui.add_enabled_ui(running.is_none(), |ui| {
                section_title(ui, "What");
                toggle_cards(ui, &mut [("Stereo bounce", &mut choices.stereo), ("Separate tracks", &mut choices.stems)]);
                ui.add_enabled_ui(choices.stereo && machine, |ui| {
                    check(ui, &mut choices.stereo_tape, "Stereo bounce with tape character");
                });
                ui.add_enabled_ui(choices.stems, |ui| {
                    section_title(ui, "Tracks");
                    egui::Grid::new("export-stems").num_columns(2).spacing([24.0, 4.0]).show(ui, |ui| {
                        for (index, name) in names.iter().enumerate() {
                            ui.add_enabled_ui(has[index], |ui| {
                                check(ui, &mut choices.stem_tracks[index], &format!("{:02}  {name}", index + 1));
                            });
                            if index % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                    section_title(ui, "Processing");
                    for (kind, label, tip) in [
                        (
                            StemKind::Mixed,
                            "With mixer (EQ, pan, fader) and tape character",
                            "Stereo, as in the mix, with its own reverb and echo",
                        ),
                        (StemKind::TapeOnly, "Tape character only, mixer dry", "Mono, the track as it comes off the tape"),
                        (StemKind::Dry, "Completely dry", "Mono, the recording itself"),
                    ] {
                        if radio(ui, choices.stem_kind == kind, label).on_hover_text(tip).clicked() {
                            choices.stem_kind = kind;
                        }
                    }
                });
                section_title(ui, "Range");
                check(ui, &mut choices.to_cue, "Only from the zero point to the CUE point");
                ui.add_enabled_ui(choices.to_cue, |ui| {
                    check(ui, &mut choices.from_zero, "All files start at 0:00");
                });
                ui.add_space(8.0);
                ui.columns(3, |columns| {
                    let rate_text = |rate: u32| format!("{}.{} kHz", rate / 1000, (rate % 1000) / 100);
                    columns[0].label(RichText::new("Format").size(11.5).color(paper::INK_SOFT));
                    egui::ComboBox::from_id_salt("export-format")
                        .selected_text(if choices.format == ExportFormat::Wav { "WAV" } else { "AIFF" })
                        .width(columns[0].available_width())
                        .show_ui(&mut columns[0], |ui| {
                            ui.selectable_value(&mut choices.format, ExportFormat::Wav, "WAV");
                            ui.selectable_value(&mut choices.format, ExportFormat::Aiff, "AIFF");
                        });
                    columns[1].label(RichText::new("Bit depth").size(11.5).color(paper::INK_SOFT));
                    let depth_text = |depth: BitDepth| match depth {
                        BitDepth::Int16 => "16-bit",
                        BitDepth::Int24 => "24-bit",
                        BitDepth::Float32 => "32-bit float",
                    };
                    egui::ComboBox::from_id_salt("export-depth")
                        .selected_text(depth_text(choices.depth))
                        .width(columns[1].available_width())
                        .show_ui(&mut columns[1], |ui| {
                            for depth in [BitDepth::Int16, BitDepth::Int24, BitDepth::Float32] {
                                ui.selectable_value(&mut choices.depth, depth, depth_text(depth));
                            }
                        });
                    columns[2].label(RichText::new("Sample rate").size(11.5).color(paper::INK_SOFT));
                    egui::ComboBox::from_id_salt("export-rate")
                        .selected_text(rate_text(choices.rate))
                        .width(columns[2].available_width())
                        .show_ui(&mut columns[2], |ui| {
                            for rate in [44_100, 48_000, 96_000] {
                                ui.selectable_value(&mut choices.rate, rate, rate_text(rate));
                            }
                        });
                });
                ui.add_space(8.0);
                let example = if choices.stems {
                    daw388_project::export::export_name(&project, Some(0), Some(&names[0]))
                } else {
                    daw388_project::export::export_name(&project, None, None)
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new("File name:").size(11.5).color(paper::INK_SOFT));
                    ui.label(RichText::new(format!("{example}.{}", choices.format.extension())).monospace().size(12.0));
                });
                note(ui, &format!("Folder: {place}"));
            });
            ui.add_space(10.0);
            if let Some(progress) = &running {
                let done = progress.done.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0;
                ui.add(egui::ProgressBar::new(done).show_percentage().fill(paper::RUST));
                ctx.request_repaint();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if running.is_none() {
                    if primary_button(ui, "Export").clicked() {
                        start = true;
                    }
                    if plain_button(ui, "Close").clicked() {
                        self.show_export = false;
                    }
                } else if let Some(progress) = &running
                    && plain_button(ui, "Cancel").clicked()
                {
                    progress.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            });
        });
        // The Close button above clears `show_export` directly; the cross clears `open`.
        let open = open && self.show_export;
        self.show_export = open;
        self.export = choices;
        if start {
            self.start_export();
        }
    }

    /// Clean or 388, tape speed and kind, and four amounts from 0 to 200%.
    fn tape_window(&mut self, ctx: &egui::Context) {
        use skin::paper::{self, check, choice_cards, note, section_title, slider_row, switch_pair};
        let mut open = self.show_tape;
        let mut tape = self.tape;
        paper::paper_window(ctx, "tape-window", "Tape settings", "per project", &mut open, 380.0, |ui| {
            section_title(ui, "Machine");
            let mut machine = 0;
            choice_cards(ui, &[(0, "388 Studio 8", Some("2 heads · 1/4 inch · 8 tracks"))], &mut machine);
            section_title(ui, "Character");
            switch_pair(ui, [(TapeMode::Clean, "Clean"), (TapeMode::Machine, "388")], &mut tape.mode, 200.0);
            note(ui, "The character of the tape on playback. Recordings through the input stay clean.");
            ui.add_enabled_ui(tape.mode == TapeMode::Machine, |ui| {
                section_title(ui, "Tape speed");
                choice_cards(
                    ui,
                    &[(SpeedEntry::Ips3_75, "3¾ IPS", None), (SpeedEntry::Ips7_5, "7½ IPS", None), (SpeedEntry::Ips15, "15 IPS", None)],
                    &mut tape.speed,
                );
                note(ui, "7½ IPS is the original speed of the 388.");
                section_title(ui, "Tape type");
                choice_cards(
                    ui,
                    &[
                        (KindEntry::T456, "456 style", Some("plenty of headroom")),
                        (KindEntry::T250, "250 style", Some("warm, saturates sooner")),
                        (KindEntry::T911, "911 style", Some("a little brighter")),
                    ],
                    &mut tape.kind,
                );
                section_title(ui, "Amounts");
                for (label, amount) in [
                    ("Saturation", &mut tape.saturation),
                    ("Wow & flutter", &mut tape.wow_flutter),
                    ("Noise (hiss)", &mut tape.noise),
                    ("Crosstalk", &mut tape.crosstalk),
                ] {
                    let shown = format!("{:.0}%", *amount * 100.0);
                    slider_row(ui, label, amount, 0.0..=2.0, 1.0, shown, "100% is the machine as it is (double-click: 100%)");
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 18.0;
                    let tip = "Noise reduction: the noise drops away in silences and breathes with the music";
                    check(ui, &mut tape.dbx[0], "dbx tracks 1-4").on_hover_text(tip);
                    check(ui, &mut tape.dbx[1], "dbx tracks 5-8").on_hover_text(tip);
                });
            });
        });
        self.show_tape = open;
        if tape != self.tape {
            self.tape = tape;
            let params = tape_params(&tape);
            self.engine_send(Command::SetTape(params));
            self.mark_dirty();
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        skin::paper::paper_window(ctx, "settings-window", "Settings", "", &mut open, 480.0, |ui| {
            self.audio_settings(ui);
            ui.add_space(8.0);
            self.metronome_settings(ui);
            ui.add_space(8.0);
            skin::paper::section_title(ui, "REPEAT, RTZ and STC");
            let mut mode = self.repeat_mode;
            if skin::paper::radio(ui, mode == RepeatMode::Tape, "388: the tape rewinds to the zero point in real time").clicked() {
                mode = RepeatMode::Tape;
            }
            if skin::paper::radio(ui, mode == RepeatMode::Cycle, "Cycle: jumps straight back, as in Logic").clicked() {
                mode = RepeatMode::Cycle;
            }
            self.set_repeat_mode(mode);
            skin::paper::note(
                ui,
                "REPEAT repeats from the zero point (RESET) to the CUE point. In 388 mode every transport key except PLAY \
                 and RTZ switches REPEAT off; in cycle mode REPEAT stays on. This setting is saved with the project.",
            );
        });
        if open && !self.show_settings {
            // Just opened: show the devices as they are now.
            self.devices = self.platform.output_devices();
            self.input_devices = self.platform.input_devices();
        }
        self.show_settings = open;
    }
}

/// Sample rate, buffer size and the latency that buffer gives.
fn describe_output(output: &OutputInfo) -> String {
    let rate = format!("{:.1} kHz", f64::from(output.sample_rate) / 1000.0);
    match output.buffer_frames {
        Some(frames) => format!("{rate}, buffer {frames} ({:.1} ms)", f64::from(frames) * 1000.0 / f64::from(output.sample_rate.max(1))),
        None => format!("{rate}, default buffer"),
    }
}

/// The shortcut for full screen.
const FULLSCREEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL.plus(Modifiers::MAC_CMD), Key::F);

/// A small icon with four corners for full screen, as in the title bar of many apps.
fn fullscreen_icon(ui: &mut Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(18.0, 16.0), Sense::click());
    let on = ui.input(|i| i.viewport().fullscreen.unwrap_or(false));
    let colour = if response.hovered() { palette::CREAM } else { palette::TEXT_DIM };
    let stroke = Stroke::new(1.5, colour);
    let r = rect.shrink(3.0);
    // The corners point out to go full screen and in to come back.
    let arm = 4.0;
    for (corner, dx, dy) in
        [(r.left_top(), 1.0, 1.0), (r.right_top(), -1.0, 1.0), (r.left_bottom(), 1.0, -1.0), (r.right_bottom(), -1.0, -1.0)]
    {
        let tip = if on { corner + vec2(dx, dy) * arm } else { corner };
        let painter = ui.painter();
        painter.line_segment([tip, tip + vec2(dx * arm * if on { -1.0 } else { 1.0 }, 0.0)], stroke);
        painter.line_segment([tip, tip + vec2(0.0, dy * arm * if on { -1.0 } else { 1.0 })], stroke);
    }
    let shortcut = ui.ctx().format_shortcut(&FULLSCREEN);
    let tip = if on { format!("Exit full screen ({shortcut})") } else { format!("Full screen ({shortcut})") };
    response.on_hover_text(tip)
}

/// Into full screen, or back out of it.
fn toggle_fullscreen(ctx: &egui::Context) {
    let on = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!on));
}

fn column(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Center), |ui| {
        ui.set_width(width);
        add(ui);
    });
}

/// A rotary knob: drag up or down to turn it (Shift for fine), double-click for `default`.
/// `enabled` false draws it dimmed (the pitch control in FIX).
fn knob(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
    colour: Color32,
    size: f32,
    enabled: bool,
) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let (min, max) = (*range.start(), *range.end());
    if enabled && response.dragged() {
        let fine = if ui.input(|i| i.modifiers.shift) { 0.2 } else { 1.0 };
        let delta = -response.drag_delta().y / 150.0 * (max - min) * fine;
        *value = (*value + delta).clamp(min, max);
        response.mark_changed();
    }
    if enabled && response.double_clicked() {
        *value = default;
    }
    let painter = ui.painter();
    let centre = rect.center();
    let radius = size / 2.0 - 2.0;
    let cap = if enabled { colour } else { Color32::from_rgb(0x8a, 0x86, 0x7c) };
    painter.circle_filled(centre, radius + 2.0, Color32::from_rgb(0x1b, 0x1b, 0x1c));
    painter.circle_filled(centre, radius, cap);
    // From -135 to +135 degrees, 0 at the top.
    let fraction = (*value - min) / (max - min);
    let angle = (-135.0 + 270.0 * fraction).to_radians();
    let direction = vec2(angle.sin(), -angle.cos());
    painter.line_segment(
        [centre + direction * (radius * 0.3), centre + direction * (radius * 0.9)],
        Stroke::new(2.5, Color32::from_rgb(0x2a, 0x26, 0x22)),
    );
    response
}

/// Which part of a concentric knob a drag started on.
#[derive(Clone, Copy, PartialEq)]
enum EqPart {
    Gain,
    Freq,
}

/// A concentric EQ knob as on the 388: the inner knob is gain (±15 dB), the outer ring the
/// frequency. A drag works on the part it started on, which is outlined while hovered or
/// dragged; double-click sets that part back (gain to 0 dB, frequency to its default).
fn eq_knob(ui: &mut Ui, band: &mut BandEntry, range: &std::ops::RangeInclusive<f32>, default_freq: f32, size: f32) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let centre = rect.center();
    let outer = size / 2.0 - 1.0;
    let inner = outer * 0.62;
    let part_at = |pos: egui::Pos2| if pos.distance(centre) <= inner { EqPart::Gain } else { EqPart::Freq };
    let id = response.id;
    if response.drag_started()
        && let Some(pos) = response.interact_pointer_pos()
    {
        ui.data_mut(|d| d.insert_temp(id, part_at(pos)));
    }
    if response.dragged() {
        let part = ui.data(|d| d.get_temp::<EqPart>(id)).unwrap_or(EqPart::Gain);
        let fine = if ui.input(|i| i.modifiers.shift) { 0.2 } else { 1.0 };
        let delta = -response.drag_delta().y / 150.0 * fine;
        match part {
            EqPart::Gain => band.gain_db = (band.gain_db + delta * 30.0).clamp(-15.0, 15.0),
            EqPart::Freq => band.freq_hz = fraction_freq(freq_fraction(band.freq_hz, range) + delta, range),
        }
        response.mark_changed();
    }
    if response.double_clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        match part_at(pos) {
            EqPart::Gain => band.gain_db = 0.0,
            EqPart::Freq => band.freq_hz = default_freq,
        }
        response.mark_changed();
    }
    let active = if response.dragged() { ui.data(|d| d.get_temp::<EqPart>(id)) } else { response.hover_pos().map(part_at) };
    let painter = ui.painter();
    let pointer = |fraction: f32| {
        let angle = (-135.0 + 270.0 * fraction).to_radians();
        vec2(angle.sin(), -angle.cos())
    };
    // The ring: grey, with a light tick for the frequency.
    painter.circle_filled(centre, outer + 1.0, Color32::from_rgb(0x1b, 0x1b, 0x1c));
    painter.circle_filled(centre, outer, Color32::from_rgb(0x6e, 0x6a, 0x62));
    let f = pointer(freq_fraction(band.freq_hz, range));
    painter.line_segment([centre + f * (inner + 1.0), centre + f * outer], Stroke::new(2.0, palette::CREAM));
    // The gain knob: orange, with a dark pointer.
    painter.circle_filled(centre, inner + 1.0, Color32::from_rgb(0x1b, 0x1b, 0x1c));
    painter.circle_filled(centre, inner, palette::ORANGE);
    let g = pointer((band.gain_db + 15.0) / 30.0);
    painter.line_segment([centre + g * (inner * 0.25), centre + g * (inner * 0.9)], Stroke::new(2.0, Color32::from_rgb(0x2a, 0x26, 0x22)));
    if let Some(part) = active {
        painter.circle_stroke(centre, if part == EqPart::Gain { inner } else { outer }, Stroke::new(1.5, palette::CREAM));
    }
    response
}

fn format_freq(hz: f32) -> String {
    if hz >= 1000.0 { format!("{:.1} kHz", hz / 1000.0) } else { format!("{hz:.0} Hz") }
}

/// A small horizontal peak meter on the same scale and colours as [`meter`].
fn level_bar(ui: &mut Ui, db: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 6.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 1.0, Color32::BLACK);
    let level = if db > MIN_DB { scale_position(&METER_TAPER, db) } else { 0.0 };
    let colour = if db > 0.0 {
        palette::LED
    } else if db > -6.0 {
        palette::ORANGE
    } else {
        palette::GREEN
    };
    painter.rect_filled(egui::Rect::from_min_size(rect.min, vec2(rect.width() * level, rect.height())), 1.0, colour);
}

/// Fader travel in points; long enough for fine work around 0 dB.
const FADER_HEIGHT: f32 = 260.0;
/// Room above and below the travel for the cap.
const FADER_MARGIN: f32 = 10.0;

/// The fader taper: (position from 0 at the bottom to 1 at the top, dB). Modelled on Logic Pro's
/// faders: +6 dB at the top, 0 dB at about three quarters, most of the travel between -10 and
/// +6 dB and the quiet end squeezed; the bottom position is off.
const TAPER: [(f32, f32); 11] = [
    (0.0, MIN_DB),
    (0.08, -40.0),
    (0.12, -30.0),
    (0.21, -20.0),
    (0.29, -15.0),
    (0.40, -10.0),
    (0.48, -6.0),
    (0.61, -3.0),
    (0.74, 0.0),
    (0.87, 3.0),
    (1.0, MAX_DB),
];

/// Scale marks printed next to the faders.
const FADER_MARKS: [(f32, &str); 11] = [
    (6.0, "6"),
    (3.0, "3"),
    (0.0, "0"),
    (-3.0, "-3"),
    (-6.0, "-6"),
    (-10.0, "-10"),
    (-15.0, "-15"),
    (-20.0, "-20"),
    (-30.0, "-30"),
    (-40.0, "-40"),
    (MIN_DB, "∞"),
];

/// The meter scale, as in Logic: (position from 0 at the bottom to 1 at the top, dBFS). Even
/// steps from 0 to -24 dB, compressed below that.
const METER_TAPER: [(f32, f32); 3] = [(0.0, MIN_DB), (0.47, -24.0), (1.0, 0.0)];

/// Position (0 to 1) of a value on a piecewise linear scale.
fn scale_position(taper: &[(f32, f32)], value: f32) -> f32 {
    let (first, last) = (taper[0], taper[taper.len() - 1]);
    let value = value.clamp(first.1, last.1);
    for pair in taper.windows(2) {
        let ((p0, d0), (p1, d1)) = (pair[0], pair[1]);
        if value <= d1 {
            return p0 + (value - d0) / (d1 - d0) * (p1 - p0);
        }
    }
    last.0
}

/// Position (0 to 1) of a level in dB on the fader.
fn fader_position(db: f32) -> f32 {
    scale_position(&TAPER, db)
}

/// Level in dB at a fader position (0 to 1).
fn fader_db(position: f32) -> f32 {
    let position = position.clamp(0.0, 1.0);
    for pair in TAPER.windows(2) {
        let ((p0, d0), (p1, d1)) = (pair[0], pair[1]);
        if position <= p1 {
            return d0 + (position - p0) / (p1 - p0) * (d1 - d0);
        }
    }
    MAX_DB
}

/// A fader level as text, "-∞" when closed.
fn format_db(db: f32) -> String {
    if db <= MIN_DB { "-∞ dB".into() } else { format!("{db:+.1} dB") }
}

/// A vertical fader in dB with a console taper and a printed scale. Shift while dragging moves it
/// five times finer; double-click resets it to 0 dB.
fn fader(ui: &mut Ui, db: &mut f32, cap: Color32) -> egui::Response {
    let height = FADER_HEIGHT + 2.0 * FADER_MARGIN;
    // As wide left of the slot (for the scale) as right of it, so a centred fader has its slot in the middle.
    let (rect, mut response) = ui.allocate_exact_size(vec2(74.0, height), Sense::click_and_drag());
    let travel = FADER_HEIGHT;
    let slot_x = rect.center().x;
    let to_y = |value: f32| rect.bottom() - FADER_MARGIN - fader_position(value) * travel;
    if response.dragged() {
        let fine = if ui.input(|i| i.modifiers.shift) { 0.2 } else { 1.0 };
        let position = fader_position(*db) - response.drag_delta().y / travel * fine;
        *db = fader_db(position);
        response.mark_changed();
    }
    if response.double_clicked() {
        *db = 0.0;
    }
    let painter = ui.painter();
    let font = egui::FontId::proportional(9.0);
    for (value, label) in FADER_MARKS {
        let y = to_y(value);
        let colour = if value == 0.0 { palette::CREAM } else { palette::TEXT_DIM };
        painter.line_segment([egui::pos2(slot_x - 19.0, y), egui::pos2(slot_x - 15.0, y)], Stroke::new(1.0, colour));
        painter.text(egui::pos2(slot_x - 20.0, y), egui::Align2::RIGHT_CENTER, label, font.clone(), colour);
    }
    let slot = egui::Rect::from_center_size(egui::pos2(slot_x, rect.center().y), vec2(4.0, travel + 6.0));
    painter.rect_filled(slot, 2.0, Color32::BLACK);
    let knob = egui::Rect::from_center_size(egui::pos2(slot_x, to_y(*db)), vec2(28.0, 16.0));
    painter.rect_filled(knob, 3.0, cap);
    painter.line_segment(
        [egui::pos2(knob.left() + 3.0, knob.center().y), egui::pos2(knob.right() - 3.0, knob.center().y)],
        Stroke::new(1.5, Color32::from_rgb(0x2a, 0x26, 0x22)),
    );
    response.on_hover_text("Drag: level, shift: fine, double-click: 0 dB")
}

fn apply_style(ctx: &egui::Context) {
    skin::fonts::install(ctx);
    // The zoom follows the window (`skin::zoom`); Cmd and +/- belong to the drawer.
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = palette::PANEL;
    visuals.window_fill = palette::PANEL;
    visuals.extreme_bg_color = palette::PANEL_DARK;
    visuals.override_text_color = Some(palette::TEXT);
    // Combo boxes and fields (input, BPM, metre, device): a dark field with a beige edge.
    visuals.widgets.inactive.weak_bg_fill = palette::PANEL_DARK;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, palette::KEY_BEIGE_SHADE);
    visuals.widgets.hovered.weak_bg_fill = palette::PANEL_DARK;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette::KEY_BEIGE);
    visuals.widgets.active.weak_bg_fill = palette::PANEL_DARK;
    visuals.selection.bg_fill = palette::KEY_BEIGE_SHADE;
    ctx.set_visuals(visuals);
    // One size for the text of egui's own widgets (combo boxes, fields, menus): that of the
    // console's labels, so "in 1", the tempo and the metre look like the rest.
    ctx.all_styles_mut(|style| {
        use egui::{FontId, TextStyle};
        style.text_styles.insert(TextStyle::Body, FontId::proportional(11.0));
        style.text_styles.insert(TextStyle::Button, FontId::proportional(11.0));
        style.text_styles.insert(TextStyle::Small, FontId::proportional(9.0));
        style.text_styles.insert(TextStyle::Monospace, FontId::monospace(11.0));
    });
}

impl DawApp {
    /// Draw one frame. Called by the platform's `eframe::App::ui`.
    pub fn ui(&mut self, ui: &mut Ui) {
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        self.now = ui.input(|i| i.time);
        // The window's size at zoom 1, whatever the zoom is now.
        let ctx = ui.ctx().clone();
        let window = ui.input(|i| i.viewport_rect().size()) * ctx.zoom_factor();
        if self.needed.y > 0.0 {
            let zoom = skin::zoom::zoom_for(window, self.needed);
            if (zoom - ctx.zoom_factor()).abs() > 1e-3 {
                ctx.set_zoom_factor(zoom);
            }
        }
        self.fit_window(&ctx, window);
        self.watch_output();
        self.poll_loads();
        self.poll_takes();
        self.poll_composed();
        self.poll_renders();
        self.poll_export();
        self.sync_transport();
        self.update_meters(dt);
        self.handle_file_shortcuts(ui);
        let mut switched = false;
        if self.pending.is_none() {
            self.handle_keys(ui);
            self.handle_drawer_keys(ui);
            // Tab switches between the MIXER and SPOREN views, unless a text field has the keyboard.
            if !ctx.text_edit_focused() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
                self.toggle_view();
                switched = true;
            }
        }
        self.autosave();
        self.handle_close(ui);
        self.update_title(ui);

        let header = egui::Panel::top("header")
            .frame(Frame::new().fill(palette::PANEL_DARK).inner_margin(Margin::symmetric(14, 5)))
            .show(ui, |ui| self.header(ui));
        // Two views, switched with the armrest under them: the MIXER (the whole console) and
        // SPOREN (the tracks, under the meter bridge and the transport).
        egui::Panel::bottom("armrest")
            .resizable(false)
            .exact_size(drawer::ARMREST)
            .frame(Frame::new().fill(palette::LEATHER))
            .show(ui, |ui| self.armrest(ui));
        let chrome = header.response.rect.height() + drawer::ARMREST;
        if self.editor.settings.open {
            // No strips in this view: the TRK meters stand side by side.
            self.strip_centres = [0.0; TRACKS];
            let deck =
                egui::Panel::top("deck").frame(Frame::new().fill(palette::PANEL).inner_margin(Margin::symmetric(12, 10))).show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        self.meter_bridge(ui);
                        ui.add_space(16.0);
                        ui.vertical(|ui| {
                            self.led_counter(ui);
                            ui.add_space(6.0);
                            self.transport_keys(ui);
                        });
                    })
                    .response
                    .rect
                    .size()
                });
            egui::CentralPanel::default().frame(Frame::new().fill(palette::LANE)).show(ui, |ui| self.tracks_view(ui));
            let bar = deck.inner;
            self.needed = egui::vec2(bar.x + 24.0, bar.y + 20.0 + chrome + drawer::LANES_MIN);
        } else {
            self.mixer_view(ui, chrome);
        }

        self.settings_window(&ctx);
        self.tape_window(&ctx);
        self.export_window(&ctx);
        self.unsaved_dialog(&ctx);
        self.confirm_simple_dialog(&ctx);
        // egui moves the focus to the next widget on Tab; after a switch nothing should keep it,
        // or the space bar and the other keys would stop working.
        if switched {
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        if self.may_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }

    /// Make the window as wide as the console at startup and after a switch between Eenvoudig and
    /// 388, so no empty strip is left at the side; after that the window is the user's to size.
    fn fit_window(&mut self, ctx: &egui::Context, window: egui::Vec2) {
        if self.editor.settings.open {
            return;
        }
        if self.fitted_for != Some(self.mixer.mode) {
            // After a switch the controls keep their size and the window follows the console;
            // at startup the window's height sets the zoom.
            self.fit_zoom = if self.fitted_for.is_some() { ctx.zoom_factor() } else { 0.0 };
            self.fitted_for = Some(self.mixer.mode);
            self.fit_in = 3;
        }
        if self.fit_in == 0 {
            return;
        }
        self.fit_in -= 1;
        let (full, maximized, monitor) = ctx.input(|i| {
            let viewport = i.viewport();
            (viewport.fullscreen.unwrap_or(false), viewport.maximized.unwrap_or(false), viewport.monitor_size)
        });
        if self.fit_in == 0 && self.needed.x > 0.0 && !full && !maximized {
            // Sizes here are at zoom 1 (as `window`); the viewport takes egui points at the
            // current zoom, and so does the monitor size.
            let current = ctx.zoom_factor();
            let zoom = if self.fit_zoom > 0.0 { self.fit_zoom } else { skin::zoom::zoom_for(egui::vec2(f32::MAX, window.y), self.needed) };
            let mut size = self.needed * zoom;
            // Never past the screen (minus the menu and title bars): then the zoom gives way.
            if let Some(screen) = monitor.map(|m| m * current - egui::vec2(0.0, 90.0)) {
                size *= (screen.x / size.x).min(screen.y / size.y).min(1.0);
            }
            if (size - window).length() > 2.0 {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize((size / current).ceil()));
            }
        }
    }

    /// The MIXER view: the whole console, scaled to fit the window.
    fn mixer_view(&mut self, ui: &mut Ui, chrome: f32) {
        let mut console = egui::Vec2::ZERO;
        egui::CentralPanel::default().frame(Frame::new().fill(palette::BACKGROUND).inner_margin(Margin::same(16))).show(ui, |ui| {
            // Centred, so space the console does not need stays even on both sides (full screen).
            ui.vertical_centered(|ui| {
                Frame::new().fill(palette::WOOD).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(18, 18)).show(
                    ui,
                    |ui| {
                        Frame::new()
                            .fill(palette::PANEL)
                            .corner_radius(CornerRadius::same(3))
                            .inner_margin(Margin::same(14))
                            .stroke(Stroke::new(2.0, palette::EDGE))
                            .show(ui, |ui| {
                                console = egui::ScrollArea::both()
                                    .show(ui, |ui| {
                                        let top = ui.cursor().min.y;
                                        self.meter_bridge(ui);
                                        ui.add_space(12.0);
                                        // The bottom of what the sections hold: the separators reach down to the
                                        // bottom of the panel and would make the console look as tall as the window.
                                        let mut bottom = top;
                                        let row = ui.horizontal_top(|ui| {
                                            // As far in as the meter bridge's margin, so TRK 1 can stand above strip 1.
                                            ui.add_space(12.0);
                                            for index in 0..TRACKS {
                                                let left = ui.cursor().min.x;
                                                bottom = bottom.max(ui.scope(|ui| self.channel_strip(ui, index)).response.rect.bottom());
                                                self.strip_centres[index] = (left + ui.cursor().min.x - ui.spacing().item_spacing.x) / 2.0;
                                                ui.separator();
                                            }
                                            // The master section in two columns, as on the 388.
                                            bottom = bottom.max(ui.scope(|ui| self.return_monitor_column(ui)).response.rect.bottom());
                                            ui.separator();
                                            bottom = bottom.max(ui.scope(|ui| self.master_column(ui)).response.rect.bottom());
                                            ui.separator();
                                            ui.add_space(16.0);
                                            let deck = ui.vertical(|ui| {
                                                self.tape_deck(ui);
                                                ui.add_space(12.0);
                                                self.transport(ui);
                                            });
                                            // The empty space under the transport closed with a blanking plate.
                                            // Both plates end on one line, just under the names of the strips.
                                            let end = bottom - 6.0;
                                            let below = deck.response.rect;
                                            if end - below.bottom() > 40.0 {
                                                let field = egui::Rect::from_min_max(
                                                    egui::pos2(below.left(), below.bottom() + 12.0),
                                                    egui::pos2(below.right(), end),
                                                );
                                                keys::blank_plate(ui.painter(), field);
                                            }
                                            if let Some(start) = self.return_plate.take() {
                                                let field = egui::Rect::from_min_max(
                                                    start.min + vec2(6.0, 6.0),
                                                    egui::pos2(start.max.x - 6.0, end),
                                                );
                                                keys::blank_plate(ui.painter(), field);
                                            }
                                            bottom = bottom.max(deck.response.rect.bottom());
                                        });
                                        // The width of the strips and sections, not of the bridge: the bridge
                                        // takes its width from this, so it must not feed back into it.
                                        egui::vec2(row.response.rect.width(), bottom - top)
                                    })
                                    .inner;
                            });
                    },
                );
            });
        });
        // The console plus its frames (16, 18 and 14 points a side and the panel's edge), the
        // header and the armrest.
        self.needed = egui::vec2(console.x + 100.0, console.y + 100.0 + chrome);
    }

    /// Save automatically some time after a change, once the project has a folder.
    fn autosave(&mut self) {
        if self.dirty && self.project_dir.is_some() && self.autosave_at.is_some_and(|at| self.now >= at) {
            self.save(Backup::WhenDue);
        }
    }

    /// Ask before closing the window with unsaved changes.
    fn handle_close(&mut self, ui: &Ui) {
        if ui.input(|i| i.viewport().close_requested()) && self.dirty && !self.may_close {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }
    }

    fn update_title(&mut self, ui: &Ui) {
        let title = format!("{}{} - DAW 388", self.project_name(), if self.dirty { " *" } else { "" });
        if title != self.title {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A platform without audio hardware whose dialogs give fixed answers. Its devices can be
    /// plugged in, pulled out and made to fail.
    struct FakePlatform {
        save_location: Option<PathBuf>,
        /// Keeps the audio side alive so commands are accepted; tests can render the latest.
        engines: std::rc::Rc<std::cell::RefCell<Vec<daw388_engine::Engine>>>,
        /// Devices that can be opened; the first is the default.
        devices: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
        lost: std::rc::Rc<std::cell::Cell<bool>>,
        opened: std::rc::Rc<std::cell::RefCell<Vec<OutputRequest>>>,
    }

    impl FakePlatform {
        fn new(save_location: Option<PathBuf>) -> Self {
            Self {
                save_location,
                engines: std::rc::Rc::default(),
                devices: std::rc::Rc::new(std::cell::RefCell::new(vec!["Built-in".into()])),
                lost: std::rc::Rc::default(),
                opened: std::rc::Rc::default(),
            }
        }
    }

    impl Platform for FakePlatform {
        fn output_devices(&mut self) -> Vec<DeviceInfo> {
            self.devices
                .borrow()
                .iter()
                .map(|name| DeviceInfo {
                    name: name.clone(),
                    sample_rates: vec![44_100, 48_000],
                    buffer_range: Some((32, 4096)),
                    channels: 2,
                })
                .collect()
        }
        fn default_output_device(&mut self) -> Option<String> {
            self.devices.borrow().first().cloned()
        }
        fn open_output(&mut self, request: &OutputRequest) -> Result<(EngineHandle, OutputInfo), String> {
            self.opened.borrow_mut().push(request.clone());
            self.lost.set(false);
            let devices = self.devices.borrow();
            let name = match &request.device {
                Some(name) if devices.contains(name) => name.clone(),
                Some(name) => return Err(format!("{name} not found")),
                None => devices.first().cloned().ok_or("no output device found")?,
            };
            let rate = request.sample_rate.unwrap_or(48_000);
            let (handle, engine) = daw388_engine::create(rate);
            self.engines.borrow_mut().push(engine);
            Ok((
                handle,
                OutputInfo {
                    device: name,
                    sample_rate: rate,
                    buffer_frames: request.buffer_frames,
                    channels: 4,
                    input: None,
                    input_error: None,
                },
            ))
        }
        fn open_silent(&mut self, sample_rate: u32) -> EngineHandle {
            let (handle, engine) = daw388_engine::create(sample_rate);
            self.engines.borrow_mut().push(engine);
            handle
        }
        fn output_lost(&mut self) -> bool {
            self.lost.get()
        }
        fn preferences_path(&self) -> Option<PathBuf> {
            None
        }
        fn pick_audio_files(&mut self, _: bool) -> Vec<PathBuf> {
            Vec::new()
        }
        fn pick_project_to_open(&mut self) -> Option<PathBuf> {
            None
        }
        fn pick_project_location(&mut self, _: &str) -> Option<PathBuf> {
            self.save_location.clone()
        }
        fn spawn(&mut self, job: Box<dyn FnOnce() + Send>) {
            // Run at once, so the tests only need to poll.
            job();
        }
        fn spawn_service(&mut self, _job: Box<dyn FnOnce() + Send>) {
            // The writer would run for as long as the engine: the tests drive takes directly.
        }
        fn temp_dir(&self) -> PathBuf {
            std::env::temp_dir().join("daw388-test-takes")
        }
        fn input_devices(&mut self) -> Vec<DeviceInfo> {
            Vec::new()
        }
    }

    /// Let the engine that feeds the current output process one block.
    fn render_latest(engines: &std::cell::RefCell<Vec<daw388_engine::Engine>>) {
        if let Some(engine) = engines.borrow_mut().last_mut() {
            let mut out = vec![0.0; 2 * 64];
            engine.render(&mut out, 2);
        }
    }

    #[test]
    fn a_new_project_plays_through_the_388_tape() {
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        assert_eq!(app.tape.mode, TapeMode::Machine);
        app.tape = TapeSettings { speed: SpeedEntry::Ips15, kind: KindEntry::T911, noise: 0.5, dbx: [true, false], ..app.tape };
        let params = tape_params(&app.tape);
        assert!(params.on && params.speed == Speed::Ips15 && params.kind == TapeKind::T911 && params.noise == 0.5);
        assert_eq!(params.dbx, [true, false]);
        assert_eq!(tape_label(&app.tape), "TAPE: 15 IPS 911");
        app.new_project();
        assert_eq!(app.tape, new_tape());
        assert!(!tape_params(&TapeSettings::default()).on, "older projects: Clean");
    }

    #[test]
    fn the_armrest_switches_between_mixer_and_tracks() {
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        assert!(!app.editor.settings.open, "a new project shows the mixer");
        app.toggle_view();
        assert!(app.editor.settings.open && app.dirty, "the view is kept with the project");
        app.toggle_view();
        assert!(!app.editor.settings.open);
    }

    #[test]
    fn the_needles_rest_without_sound() {
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        app.mixer.meter = MeterSwitch::AuxEff;
        app.update_meters(0.5);
        assert_eq!(app.big_vu[0].position(), 0.0, "no AUX without sound");
        assert!(app.tracks.iter().all(|t| t.vu.position() == 0.0));
    }

    #[test]
    fn a_new_project_opens_the_effect_returns() {
        let ctx = egui::Context::default();
        let app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        assert!(app.mixer.effects.rtn.iter().all(|r| r.gain_db == 0.0));
        let mut mixer = app.mixer;
        mixer.effects.echo.sync = EchoSyncEntry::DottedEighth;
        mixer.effects.rtn[1].gain_db = MIN_DB;
        let params = mixer_params(&mixer, &Preferences::default(), 2);
        assert_eq!((params.echo.sync, params.rtn[0].gain, params.rtn[1].gain), (EchoSync::DottedEighth, 1.0, 0.0));
    }

    #[test]
    fn aux_and_eff_outputs_skip_the_main_pair_and_missing_channels() {
        assert_eq!(send_output(Some(3), 8), Some(2));
        assert_eq!(send_output(Some(8), 8), Some(7));
        assert_eq!(send_output(Some(9), 8), None, "not on this device");
        assert_eq!(send_output(Some(2), 8), None, "1 and 2 are the main output");
        assert_eq!(send_output(None, 8), None);
        let mixer = MixerEntry { listen: ListenSwitch::Eff, phones_db: MIN_DB, ..MixerEntry::default() };
        let preferences = Preferences { aux_output: Some(4), ..Preferences::default() };
        let params = mixer_params(&mixer, &preferences, 2);
        assert_eq!((params.aux_out, params.listen, params.phones), (None, Listen::Eff, 0.0));
        assert_eq!(mixer_params(&mixer, &preferences, 4).aux_out, Some(3));
    }

    #[test]
    fn a_new_channel_has_its_sends_closed() {
        let track = TrackState { channel: ChannelEntry::simple(ChannelSource::Line), ..TrackState::default() };
        let params = channel_params(&track);
        assert_eq!((params.aux, params.eff), (0.0, 0.0));
    }

    #[test]
    fn a_new_project_is_simple() {
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        assert_eq!(app.mixer.mode, Routing::Simple);
        assert!(app.tracks.iter().all(|t| t.channel == ChannelEntry::simple(ChannelSource::Line)));
        app.new_project();
        assert_eq!(app.mixer.mode, Routing::Simple);
        assert!(app.tracks.iter().all(|t| t.channel.auto));
    }

    #[test]
    fn switching_to_simple_resets_the_388_routing_after_asking() {
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        app.set_routing(Routing::Classic);
        assert_eq!(app.mixer.mode, Routing::Classic);
        assert!(app.tracks[0].channel.auto, "to 388 changes nothing in the routing");
        app.set_routing(Routing::Simple);
        assert!(!app.confirm_simple, "nothing to reset: no question");
        assert_eq!(app.mixer.mode, Routing::Simple);

        app.set_routing(Routing::Classic);
        let track = &mut app.tracks[2];
        track.channel = ChannelEntry { source: ChannelSource::Mic, assign: [true, false, false, false], ..ChannelEntry::default() };
        track.channel.eq.mid.gain_db = 6.0;
        track.channel.trim_db = 12.0;
        track.fader_db = -4.0;
        track.channel.aux_db = -10.0;
        app.tracks[5].channel.source = ChannelSource::Rmx;
        app.tracks[5].channel.auto = false;
        app.mixer.phones_db = -6.0;
        app.mixer.odd_db = -3.0;
        app.set_routing(Routing::Simple);
        assert!(app.confirm_simple, "the 388 routing would be lost: ask first");
        assert_eq!(app.mixer.mode, Routing::Classic);
        app.apply_simple();
        assert_eq!(app.mixer.mode, Routing::Simple);
        let channel = app.tracks[2].channel;
        assert!(channel.auto && channel.direct && channel.lr && channel.assign == [false; 4]);
        assert_eq!(channel.source, ChannelSource::Mic, "MIC stays MIC");
        assert_eq!((channel.eq.mid.gain_db, channel.trim_db, app.tracks[2].fader_db), (6.0, 12.0, -4.0), "the sound stays");
        assert_eq!(app.tracks[5].channel.source, ChannelSource::Line, "RMX becomes LINE");
        assert_eq!((app.mixer.odd_db, app.mixer.phones_db), (0.0, -6.0), "the buses go back, PHONES stays");
        assert_eq!(app.tracks[2].channel.aux_db, -10.0, "the sends stay");
    }

    #[test]
    fn simple_rec_warns_without_an_input() {
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        app.tracks[1].input = None;
        app.toggle_arm(1);
        assert!(app.status.contains("choose an input"), "{}", app.status);
    }

    #[test]
    fn eq_frequencies_turn_logarithmically() {
        let range = 50.0..=1000.0;
        assert!(freq_fraction(50.0, &range).abs() < 1e-6);
        assert!((freq_fraction(1000.0, &range) - 1.0).abs() < 1e-6);
        for f in [50.0, 100.0, 333.0, 1000.0] {
            assert!((fraction_freq(freq_fraction(f, &range), &range) - f).abs() < 0.01, "{f}");
        }
        assert!((fraction_freq(0.5, &range) - (50.0f32 * 1000.0).sqrt()).abs() < 0.01, "the middle is the geometric mean");
    }

    #[test]
    fn the_channel_eq_reaches_the_engine() {
        let mut track = TrackState::default();
        track.channel.eq.mid = BandEntry { freq_hz: 2500.0, gain_db: -4.5 };
        let params = channel_params(&track);
        assert_eq!(params.eq[1], EqBand { freq: 2500.0, gain_db: -4.5 });
        assert_eq!(params.eq[0], EQ_DEFAULTS[0]);
    }

    #[test]
    fn ol_lights_above_minus_3_dbfs() {
        assert!(!overload(0.70));
        assert!(overload(0.71));
    }

    #[test]
    fn a_missing_chosen_device_falls_back_and_comes_back() {
        let ctx = egui::Context::default();
        let platform = FakePlatform::new(None);
        let (devices, lost) = (std::rc::Rc::clone(&platform.devices), std::rc::Rc::clone(&platform.lost));
        let engines = std::rc::Rc::clone(&platform.engines);
        devices.borrow_mut().push("Scarlett".into());
        let mut app = DawApp::new(&ctx, Box::new(platform));
        app.set_output(Preferences {
            output_device: Some("Scarlett".into()),
            sample_rate: Some(44_100),
            buffer_frames: Some(128),
            ..Preferences::default()
        });
        assert_eq!(app.output.as_ref().map(|o| o.device.as_str()), Some("Scarlett"));
        assert_eq!(app.engine_rate(), 44_100);

        // Pulled out while playing: straight to the default device, at the same place.
        app.play();
        render_latest(&engines);
        assert!(app.engine.as_ref().is_some_and(EngineHandle::is_playing));
        devices.borrow_mut().retain(|d| d != "Scarlett");
        lost.set(true);
        app.watch_output();
        render_latest(&engines);
        assert!(app.engine.as_ref().is_some_and(EngineHandle::is_playing), "it keeps playing on the new device");
        assert_eq!(app.output.as_ref().map(|o| o.device.as_str()), Some("Built-in"));
        assert!(app.status.contains("went away") && app.status.contains("now through Built-in"), "{}", app.status);
        assert!(app.on_fallback());

        // Plugged back in: the next check returns to it.
        devices.borrow_mut().push("Scarlett".into());
        app.now += DEVICE_CHECK_EVERY + 0.1;
        app.watch_output();
        assert_eq!(app.output.as_ref().map(|o| o.device.as_str()), Some("Scarlett"));
        assert!(!app.on_fallback());
    }

    #[test]
    fn without_any_device_the_silent_clock_runs_until_one_appears() {
        let ctx = egui::Context::default();
        let platform = FakePlatform::new(None);
        let devices = std::rc::Rc::clone(&platform.devices);
        devices.borrow_mut().clear();
        let mut app = DawApp::new(&ctx, Box::new(platform));
        assert!(app.output.is_none());
        assert!(app.engine.is_some(), "the transport still has an engine");
        assert!(app.status.contains("without sound"), "{}", app.status);
        devices.borrow_mut().push("Built-in".into());
        app.now += DEVICE_CHECK_EVERY + 0.1;
        app.watch_output();
        assert_eq!(app.output.as_ref().map(|o| o.device.as_str()), Some("Built-in"));
    }

    fn write_wav(path: &Path) {
        let spec = hound::WavSpec { channels: 1, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut writer = hound::WavWriter::create(path, spec).unwrap_or_else(|e| panic!("{e}"));
        for n in 0..441 {
            writer.write_sample((n * 10) as i16).unwrap_or_else(|e| panic!("{e}"));
        }
        writer.finalize().unwrap_or_else(|e| panic!("{e}"));
    }

    /// A mono 48 kHz WAV of `seconds` long with a constant level.
    fn write_wav_of(path: &Path, seconds: f64, value: f32) {
        let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut writer = hound::WavWriter::create(path, spec).unwrap_or_else(|e| panic!("{e}"));
        for _ in 0..(seconds * 48_000.0).round() as usize {
            writer.write_sample(value).unwrap_or_else(|e| panic!("{e}"));
        }
        writer.finalize().unwrap_or_else(|e| panic!("{e}"));
    }

    /// Let every background result that is waiting arrive (spawn runs at once in the tests).
    fn settle(app: &mut DawApp) {
        for _ in 0..4 {
            app.poll_loads();
            app.poll_takes();
            app.poll_composed();
            app.poll_renders();
        }
    }

    /// (start, offset, length) of a track's regions in milliseconds.
    fn regions_ms(app: &DawApp, track: usize) -> Vec<(i64, i64, i64)> {
        let ms = |s: f64| (s * 1000.0).round() as i64;
        app.tracks[track].regions.iter().map(|r| (ms(r.start), ms(r.offset), ms(r.length))).collect()
    }

    #[test]
    fn export_writes_the_mix_and_the_stems_into_the_project() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let kick = files.path().join("01_Kick.wav");
        write_wav(&kick);
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("Demo");
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(Some(dir.clone()))));
        app.start_export();
        assert!(app.status.contains("Save the project first"), "{}", app.status);
        app.load_paths(vec![kick]);
        app.poll_loads();
        assert!(app.save(Backup::Always));
        app.export = ExportChoices { stems: true, stem_kind: StemKind::Dry, ..ExportChoices::default() };
        app.start_export();
        app.poll_export();
        assert!(app.status.contains("2 file(s)"), "{}", app.status);
        assert!(dir.join("export").join("Demo_mix.wav").exists());
        assert!(dir.join("export").join("Demo_01_01_Kick.wav").exists());
    }

    #[test]
    fn save_and_reopen_a_project() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let kick = files.path().join("01_Kick.wav");
        let bass = files.path().join("03_Bas.wav");
        write_wav(&kick);
        write_wav(&bass);
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("Demo");

        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(Some(dir.clone()))));
        app.load_paths(vec![kick, bass]);
        settle(&mut app);
        assert!(app.dirty);
        app.tracks[1].fader_db = -6.0;
        app.tracks[1].pan = -0.5;
        app.tracks[1].mute = true;
        app.tracks[1].channel.assign[1] = true;
        app.tracks[1].monitor.gain_db = -4.0;
        app.mixer.mon = MonSwitch::Pgm;
        app.mixer.mode = Routing::Classic;
        app.tracks[1].solo = true;
        app.master_db = -2.0;
        app.meter_post = true;
        assert!(app.save(Backup::Always), "{}", app.status);
        assert!(!app.dirty);
        assert!(app.saved_label.as_deref().is_some_and(|l| l.starts_with("Saved")));
        assert_eq!(app.project_name(), "Demo");
        let source = app.tracks[0].regions[0].source;
        assert!(app.sources.path(source).is_some_and(|s| s.starts_with(&dir)), "audio now lives in the project");

        app.new_project();
        assert!(app.tracks.iter().all(|t| t.regions.is_empty()));
        app.open_arguments(vec![dir.clone()]);
        settle(&mut app);
        assert_eq!(app.project_name(), "Demo");
        assert!(!app.dirty);
        assert_eq!(regions_ms(&app, 0), vec![(0, 0, 10)], "441 samples at 44.1 kHz");
        assert_eq!((app.tracks[0].name.as_str(), app.tracks[1].name.as_str()), ("01_Kick", "03_Bas"));
        assert!(app.tracks[0].rendered.is_some(), "and the engine plays it");
        assert_eq!(app.tracks[1].fader_db, -6.0);
        assert_eq!(app.tracks[1].pan, -0.5);
        assert!(app.tracks[1].mute);
        assert!(app.tracks[1].channel.assign[1]);
        assert_eq!(app.tracks[1].monitor.gain_db, -4.0);
        assert_eq!(app.mixer.mon, MonSwitch::Pgm);
        assert_eq!(app.mixer.mode, Routing::Classic, "a new project is simple, so this came from the file");
        assert!(!app.tracks[1].solo, "SOLO is for listening only and is not saved");
        assert_eq!(app.master_db, -2.0);
        assert!(app.meter_post);
        assert!(app.tracks[2].regions.is_empty());
    }

    #[test]
    fn a_missing_file_is_shown_and_kept() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let kick = files.path().join("kick.wav");
        write_wav(&kick);
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("Demo");
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(Some(dir.clone()))));
        app.load_paths(vec![kick]);
        settle(&mut app);
        assert!(app.save(Backup::Always));
        std::fs::remove_file(dir.join("audio/kick.wav")).unwrap_or_else(|e| panic!("{e}"));

        app.open_project(&dir);
        settle(&mut app);
        assert_eq!(app.track_missing(0).len(), 1);
        assert!(app.status.contains("missing on track 1"), "{}", app.status);
        // Saving again keeps the reference, so putting the file back repairs the project.
        app.mark_dirty();
        assert!(app.save(Backup::Always));
        let opened = session::open(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(opened.file.tracks[0].regions[0].file, "audio/kick.wav");
    }

    #[test]
    fn transport_settings_are_saved_and_restored() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("Demo");
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(Some(dir.clone()))));
        app.zero_seconds = 2.0;
        app.cue_seconds = Some(12.5);
        app.repeat_mode = RepeatMode::Cycle;
        app.pitch_percent = 3.0;
        app.vari = true;
        app.mark_dirty();
        assert!(app.save(Backup::Always));
        app.new_project();
        assert_eq!((app.zero_seconds, app.cue_seconds, app.repeat_mode, app.vari), (0.0, None, RepeatMode::Tape, false));
        app.open_project(&dir);
        assert_eq!(app.zero_seconds, 2.0);
        assert_eq!(app.cue_seconds, Some(12.5));
        assert_eq!(app.repeat_mode, RepeatMode::Cycle);
        assert_eq!(app.pitch_percent, 3.0);
        assert!(app.vari);
        assert!((app.pitch_ratio() - 1.03).abs() < 1e-9);
        app.vari = false;
        assert_eq!(app.pitch_ratio(), 1.0, "FIX plays at normal speed");
    }

    #[test]
    fn transport_keys_switch_repeat_off_in_388_mode_only() {
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        app.set_repeat(true);
        app.play();
        app.rtz();
        assert!(app.repeat, "PLAY and RTZ leave REPEAT on");
        app.fast_forward();
        assert!(!app.repeat, "any other transport key switches it off, as on the 388");
        app.set_repeat_mode(RepeatMode::Cycle);
        app.set_repeat(true);
        app.stop();
        app.rewind();
        assert!(app.repeat, "in cycle mode REPEAT stays on");
    }

    #[test]
    fn a_late_load_for_a_cleared_track_is_ignored() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let kick = files.path().join("kick.wav");
        write_wav(&kick);
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        // The fake platform loads at once; the result waits until polled.
        app.load_paths(vec![kick]);
        assert!(app.is_loading());
        app.clear_track(0);
        settle(&mut app);
        assert!(app.tracks[0].regions.is_empty() && app.tracks[0].unresolved.is_none(), "the old load must not come back after Clear");
        assert!(!app.is_loading());
    }

    fn write_take(path: &Path, value: f32, frames: usize) {
        let spec = hound::WavSpec { channels: 1, sample_rate: 48_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut writer = hound::WavWriter::create(path, spec).unwrap_or_else(|e| panic!("{e}"));
        for _ in 0..frames {
            writer.write_sample(value).unwrap_or_else(|e| panic!("{e}"));
        }
        writer.finalize().unwrap_or_else(|e| panic!("{e}"));
    }

    /// A project in `dir` with `01_Kick.wav` (1 s at 0.25) on track 1, saved.
    fn project_with_kick(files: &Path, dir: &Path) -> DawApp {
        let kick = files.join("01_Kick.wav");
        write_wav_of(&kick, 1.0, 0.25);
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(Some(dir.to_path_buf()))));
        app.load_paths(vec![kick]);
        settle(&mut app);
        assert!(app.save(Backup::Always));
        app
    }

    fn send_take(app: &DawApp, files: &Path, track: usize, start: f64, frames: usize, rate: u32) -> PathBuf {
        let take = files.join(format!("take-{track}-{start}.wav"));
        write_take(&take, 0.5, frames);
        let _ = app.takes_tx.send(TakeDone { track, start, path: take.clone(), frames, rate, speed: 1.0 });
        take
    }

    #[test]
    fn a_take_becomes_a_region_and_can_be_undone() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("Demo");
        let mut app = project_with_kick(files.path(), &dir);

        let take = send_take(&app, files.path(), 0, 4_800.0, 4_800, 48_000);
        settle(&mut app);
        assert_eq!(regions_ms(&app, 0), vec![(0, 0, 100), (100, 0, 100), (200, 200, 800)]);
        let source = app.sources.path(app.tracks[0].regions[1].source).map(Path::to_path_buf).unwrap_or_default();
        assert!(source.starts_with(dir.join("audio")), "{}", source.display());
        assert_eq!(source.file_name().and_then(|n| n.to_str()), Some("01_Kick_take1.wav"));
        assert!(source.exists() && !take.exists());
        assert_eq!(app.history.undo_name(), Some("Take on track 1"));
        assert!(app.dirty);
        assert_eq!(app.tracks[0].rendered, Some((48_000, 48_000)), "the engine plays the new track");

        app.undo();
        assert_eq!(regions_ms(&app, 0), vec![(0, 0, 1000)]);
        assert!(source.exists(), "undo keeps the take's file in audio/");
        app.redo();
        assert_eq!(app.tracks[0].regions.len(), 3);
    }

    #[test]
    fn takes_on_one_track_are_laid_down_in_order() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(Some(root.path().join("Demo")))));
        assert!(app.save(Backup::Always));
        send_take(&app, files.path(), 2, 4_800.0, 9_600, 48_000);
        send_take(&app, files.path(), 2, 9_600.0, 9_600, 48_000);
        settle(&mut app);
        assert_eq!(regions_ms(&app, 2), vec![(100, 0, 100), (200, 0, 200)], "the second take is laid over the first");
        assert_eq!(app.history.len(), 2);
        assert_eq!(app.tracks[2].name, "03_Track3_take1");
    }

    #[test]
    fn a_take_before_the_start_loses_that_part() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let mut app = project_with_kick(files.path(), &root.path().join("Demo"));
        send_take(&app, files.path(), 1, -480.0, 4_800, 48_000);
        settle(&mut app);
        assert_eq!(regions_ms(&app, 1), vec![(0, 10, 90)]);
    }

    #[test]
    fn a_take_after_wis_is_its_own_step() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let mut app = project_with_kick(files.path(), &root.path().join("Demo"));
        send_take(&app, files.path(), 0, 4_800.0, 4_800, 48_000);
        app.poll_takes();
        app.clear_track(0);
        settle(&mut app);
        assert_eq!(regions_ms(&app, 0), vec![(100, 0, 100)]);
        assert_eq!(app.history.undo_name(), Some("Take on track 1"));
        app.undo();
        assert_eq!(app.history.undo_name(), Some("Clear"));
    }

    #[test]
    fn a_take_from_another_project_is_not_placed() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let mut app = project_with_kick(files.path(), &root.path().join("Demo"));
        send_take(&app, files.path(), 1, 0.0, 4_800, 48_000);
        app.poll_takes();
        app.new_project();
        settle(&mut app);
        assert!(app.tracks.iter().all(|t| t.regions.is_empty()));
        assert!(app.status.contains("another project"), "{}", app.status);
    }

    #[test]
    fn a_take_at_another_rate_lands_in_the_right_place() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let mut app = project_with_kick(files.path(), &root.path().join("Demo"));
        app.set_output(Preferences { sample_rate: Some(44_100), ..Preferences::default() });
        assert_eq!(app.engine_rate(), 44_100);
        send_take(&app, files.path(), 1, 48_000.0, 4_800, 48_000);
        settle(&mut app);
        assert_eq!(regions_ms(&app, 1), vec![(1000, 0, 100)]);
        assert_eq!(app.tracks[1].rendered, Some((44_100, 48_510)), "rendered at the engine's rate");
    }

    #[test]
    fn loading_files_is_one_step_and_resolves_to_regions() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (kick, bass) = (files.path().join("01_Kick.wav"), files.path().join("02_Bas.wav"));
        write_wav_of(&kick, 1.0, 0.25);
        write_wav_of(&bass, 0.5, 0.25);
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        app.load_paths(vec![bass, kick]);
        settle(&mut app);
        assert_eq!(regions_ms(&app, 0), vec![(0, 0, 1000)]);
        assert_eq!(regions_ms(&app, 1), vec![(0, 0, 500)]);
        assert!(app.tracks[0].unresolved.is_none());
        assert_eq!(app.history.undo_name(), Some("Load"));
        app.undo();
        assert!(app.tracks[0].regions.is_empty() && app.tracks[1].regions.is_empty());
        assert_eq!(app.history.undo_name(), None);
    }

    #[test]
    fn an_import_lands_at_the_playhead() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let platform = FakePlatform::new(Some(root.path().join("Demo")));
        let engines = std::rc::Rc::clone(&platform.engines);
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(platform));
        let kick = files.path().join("01_Kick.wav");
        write_wav_of(&kick, 1.0, 0.25);
        app.load_paths(vec![kick]);
        settle(&mut app);
        app.engine_send(Command::Locate(24_000.0));
        render_latest(&engines);
        let snare = files.path().join("Snare.wav");
        write_wav_of(&snare, 0.25, 0.5);
        app.import_path(0, &snare);
        settle(&mut app);
        assert_eq!(regions_ms(&app, 0), vec![(0, 0, 500), (500, 0, 250), (750, 750, 250)]);
        assert_eq!(app.history.undo_name(), Some("Import"));
        app.undo();
        assert_eq!(regions_ms(&app, 0), vec![(0, 0, 1000)]);
        app.redo();
        assert_eq!(app.tracks[0].regions.len(), 3);
    }

    #[test]
    fn an_unresolved_track_refuses_rec_and_import() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("Oud");
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        let v2 = r#"{"format":"daw388-project","version":2,"app_version":"0.16.0","tracks":[{"number":1,"file":"audio/weg.wav"}]}"#;
        std::fs::write(dir.join(PROJECT_FILE), v2).unwrap_or_else(|e| panic!("{e}"));
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        app.open_project(&dir);
        settle(&mut app);
        assert!(app.tracks[0].unresolved.is_some());
        app.toggle_arm(0);
        assert!(!app.tracks[0].armed);
        assert!(app.status.contains("clear the track"), "{}", app.status);
        app.clear_track(0);
        app.toggle_arm(0);
        assert!(app.tracks[0].armed);
    }

    #[test]
    fn the_undo_stack_holds_ten_steps() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let kick = files.path().join("kick.wav");
        write_wav_of(&kick, 0.1, 0.25);
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        for _ in 0..6 {
            app.load_paths(vec![kick.clone()]);
            settle(&mut app);
            app.clear_track(0);
        }
        assert_eq!(app.history.len(), 10);
        for _ in 0..10 {
            app.undo();
        }
        app.undo();
        assert_eq!(app.status, "Nothing to undo");
    }

    #[test]
    fn a_device_change_renders_again_at_the_new_rate() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let kick = files.path().join("kick.wav");
        write_wav_of(&kick, 1.0, 0.25);
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        app.load_paths(vec![kick]);
        settle(&mut app);
        assert_eq!(app.tracks[0].rendered, Some((48_000, 48_000)));
        app.set_output(Preferences { sample_rate: Some(44_100), ..Preferences::default() });
        settle(&mut app);
        assert_eq!(app.tracks[0].rendered, Some((44_100, 44_100)));
    }

    #[test]
    fn a_loaded_source_has_peaks_and_keeps_them_on_a_resample() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let kick = files.path().join("kick.wav");
        write_wav_of(&kick, 1.0, 0.25);
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        app.load_paths(vec![kick]);
        settle(&mut app);
        let id = app.tracks[0].regions[0].source;
        let peaks = app.sources.get(id).and_then(|s| s.peaks.clone()).unwrap_or_else(|| panic!("no peaks"));
        assert_eq!(peaks.blocks.len(), 48_000usize.div_ceil(256));
        app.set_output(Preferences { sample_rate: Some(44_100), ..Preferences::default() });
        settle(&mut app);
        assert!(app.sources.get(id).and_then(|s| s.peaks.as_ref()).is_some_and(|p| Arc::ptr_eq(p, &peaks)));
    }

    #[test]
    fn the_length_of_a_missing_source_is_what_its_regions_reach() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("Weg");
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        let json = r#"{"format":"daw388-project","version":3,"app_version":"0.17.0","tracks":[{"number":1,"regions":[
            {"file":"audio/weg.wav","start":0.0,"offset":0.5,"length":1.0,"name":"a"},
            {"file":"audio/weg.wav","start":2.0,"offset":1.5,"length":1.0,"name":"b"}]}]}"#;
        std::fs::write(dir.join(PROJECT_FILE), json).unwrap_or_else(|e| panic!("{e}"));
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        app.open_project(&dir);
        settle(&mut app);
        let id = app.tracks[0].regions[0].source;
        assert_eq!(app.source_length(id), 2.5);
    }

    /// An app with a 1 s file on each of the first `tracks` tracks.
    fn app_with_tracks(files: &Path, tracks: usize) -> (DawApp, std::rc::Rc<std::cell::RefCell<Vec<daw388_engine::Engine>>>) {
        let platform = FakePlatform::new(None);
        let engines = std::rc::Rc::clone(&platform.engines);
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(platform));
        let paths: Vec<PathBuf> = (0..tracks)
            .map(|t| {
                let path = files.join(format!("{:02}_track.wav", t + 1));
                write_wav_of(&path, 1.0, 0.25);
                path
            })
            .collect();
        app.load_paths(paths);
        settle(&mut app);
        (app, engines)
    }

    /// Put the tape at `seconds` (48 kHz).
    fn locate(app: &mut DawApp, engines: &std::cell::RefCell<Vec<daw388_engine::Engine>>, seconds: f64) {
        app.engine_send(Command::Locate(seconds * 48_000.0));
        render_latest(engines);
    }

    #[test]
    fn split_selected_or_everything_at_the_playhead() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (mut app, engines) = app_with_tracks(files.path(), 2);
        locate(&mut app, &engines, 0.5);
        app.split();
        assert_eq!(regions_ms(&app, 0), vec![(0, 0, 500), (500, 500, 500)]);
        assert_eq!(regions_ms(&app, 1).len(), 2);
        assert_eq!(app.history.undo_name(), Some("Split"));
        app.undo();
        app.select_region(0, 0, false);
        app.split();
        assert_eq!(regions_ms(&app, 0).len(), 2);
        assert_eq!(regions_ms(&app, 1).len(), 1, "only the selected region");
    }

    #[test]
    fn cut_and_paste_regions() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (mut app, engines) = app_with_tracks(files.path(), 1);
        app.select_region(0, 0, false);
        app.cut();
        assert!(app.tracks[0].regions.is_empty());
        assert_eq!(app.history.undo_name(), Some("Cut"));
        locate(&mut app, &engines, 2.0);
        app.paste();
        assert_eq!(regions_ms(&app, 0), vec![(2000, 0, 1000)]);
        assert_eq!(app.history.undo_name(), Some("Paste"));
        assert_eq!(app.editor.selection, vec![(0, 0)], "the pasted region is selected");
    }

    #[test]
    fn a_splice_and_an_insert() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (mut app, engines) = app_with_tracks(files.path(), 2);
        app.select_range(0.25, 0.5);
        app.cut();
        for track in 0..2 {
            assert_eq!(regions_ms(&app, track), vec![(0, 0, 250), (250, 500, 500)]);
        }
        assert_eq!(app.history.undo_name(), Some("Splice"));
        assert!(app.editor.range.is_none());
        locate(&mut app, &engines, 0.25);
        app.paste();
        assert_eq!(regions_ms(&app, 0), vec![(0, 0, 250), (250, 250, 250), (500, 500, 500)]);
        assert_eq!(app.history.undo_name(), Some("Insert"));
    }

    #[test]
    fn delete_a_range_leaves_a_gap() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (mut app, _engines) = app_with_tracks(files.path(), 1);
        app.select_range(0.5, 0.25);
        app.delete();
        assert_eq!(regions_ms(&app, 0), vec![(0, 0, 250), (500, 500, 500)]);
        assert_eq!(app.history.undo_name(), Some("Erase"));
    }

    #[test]
    fn moving_onto_a_waiting_track_is_refused() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (mut app, _engines) = app_with_tracks(files.path(), 1);
        app.tracks[1].unresolved =
            Some(Unresolved { source: app.sources.add(files.path().join("weg.wav"), sources::SourceState::Missing), name: "weg".into() });
        app.select_region(0, 0, false);
        app.move_selection(1, 0.0);
        assert_eq!(regions_ms(&app, 0), vec![(0, 0, 1000)]);
        assert!(app.status.contains("clear the track"), "{}", app.status);
    }

    #[test]
    fn moving_keeps_the_selection() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (mut app, _engines) = app_with_tracks(files.path(), 1);
        app.select_region(0, 0, false);
        app.move_selection(2, 0.5);
        assert!(app.tracks[0].regions.is_empty());
        assert_eq!(regions_ms(&app, 2), vec![(500, 0, 1000)]);
        assert_eq!(app.editor.selection, vec![(2, 0)]);
        assert_eq!(app.history.undo_name(), Some("Move"));
        app.move_selection(0, -5.0);
        assert_eq!(regions_ms(&app, 2), vec![(0, 0, 1000)], "not before 0:00");
    }

    #[test]
    fn trimming_stops_at_the_source() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (mut app, _engines) = app_with_tracks(files.path(), 1);
        app.trim(0, 0, timeline::Edge::End, 5.0);
        assert_eq!(regions_ms(&app, 0), vec![(0, 0, 1000)]);
        app.trim(0, 0, timeline::Edge::Start, 0.25);
        assert_eq!(regions_ms(&app, 0), vec![(250, 250, 750)]);
        assert_eq!(app.history.undo_name(), Some("Trim"));
        assert_eq!(app.editor.selection, vec![(0, 0)]);
        app.trim(0, 0, timeline::Edge::Start, 0.0);
        assert_eq!(regions_ms(&app, 0), vec![(0, 0, 1000)], "pulled back");
    }

    #[test]
    fn renaming_is_a_step() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (mut app, _engines) = app_with_tracks(files.path(), 1);
        app.rename(0, " Kick ".into());
        assert_eq!(app.tracks[0].name, "Kick");
        assert_eq!(app.history.undo_name(), Some("Rename"));
        app.undo();
        assert_eq!(app.tracks[0].name, "01_track");
    }

    #[test]
    fn dropped_files_are_one_step() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        let mut paths: Vec<PathBuf> = (0..3)
            .map(|n| {
                let path = files.path().join(format!("{n}.wav"));
                write_wav_of(&path, 0.5, 0.25);
                path
            })
            .collect();
        let text = files.path().join("notities.txt");
        std::fs::write(&text, "no audio").unwrap_or_else(|e| panic!("{e}"));
        paths.push(text);
        app.drop_files(6, 1.0, paths);
        settle(&mut app);
        assert_eq!(regions_ms(&app, 6), vec![(1000, 0, 500)]);
        assert_eq!(regions_ms(&app, 7), vec![(1000, 0, 500)]);
        assert!(app.status.contains("2 file(s) not placed"), "{}", app.status);
        assert_eq!(app.history.len(), 1);
        app.undo();
        assert!(app.tracks[6].regions.is_empty() && app.tracks[7].regions.is_empty());
    }

    #[test]
    fn undo_clears_the_selection() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let (mut app, _engines) = app_with_tracks(files.path(), 2);
        app.clear_track(1);
        app.select_region(0, 0, false);
        app.undo();
        assert!(app.editor.selection.is_empty());
    }

    #[test]
    fn rec_explains_what_is_missing() {
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        app.record();
        assert!(app.status.contains("Save the project first"), "{}", app.status);
        app.set_routing(Routing::Classic);
        app.tracks[3].channel = ChannelEntry::default();
        app.toggle_arm(3);
        assert!(app.tracks[3].armed);
        assert!(app.status.contains("receives nothing"), "RMX without ASSIGN: {}", app.status);
        app.tracks[4].channel = ChannelEntry { source: ChannelSource::Line, direct: true, ..ChannelEntry::default() };
        app.status.clear();
        app.toggle_arm(4);
        assert!(app.status.is_empty(), "LINE with DIRECT and an input: {}", app.status);
    }

    #[test]
    fn tap_tempo_follows_the_presses() {
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        app.now = 10.0;
        assert_eq!(app.tap(), None, "one press is not a tempo yet");
        app.now = 10.5;
        assert_eq!(app.tap(), Some(120.0));
        app.now = 11.0;
        app.tap();
        app.now = 11.6;
        let bpm = app.tap().unwrap_or_default();
        assert!((bpm - 112.5).abs() < 0.01, "the average of the presses: {bpm}");
        app.now = 20.0;
        assert_eq!(app.tap(), None, "after a pause the count starts again");
        app.metronome.unit = 8;
        app.now = 20.25;
        assert_eq!(app.tap(), Some(120.0), "in 6/8 a tap is an eighth note");
    }

    #[test]
    fn the_metronome_is_saved_with_the_project() {
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("Demo");
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(Some(dir.clone()))));
        let metronome = MetronomeSettings { click: true, always: false, bpm: 87.0, beats: 3, unit: 4, volume_db: -10.0, count_in: 1 };
        app.set_metronome(metronome);
        assert!(app.dirty);
        assert!(app.save(Backup::Always));
        app.new_project();
        assert_eq!(app.metronome, MetronomeSettings::default());
        app.open_project(&dir);
        assert_eq!(app.metronome, metronome);
    }

    #[test]
    fn unsaved_changes_ask_before_new() {
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        app.request(Pending::New);
        assert!(app.pending.is_none(), "nothing to lose, so no question");
        app.mark_dirty();
        app.request(Pending::New);
        assert_eq!(app.pending, Some(Pending::New));
    }

    #[test]
    fn closing_the_project_empties_it_and_quitting_asks_first() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let kick = files.path().join("kick.wav");
        write_wav(&kick);
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(Some(root.path().join("Demo")))));
        app.load_paths(vec![kick]);
        app.poll_loads();
        app.request(Pending::Quit);
        assert_eq!(app.pending, Some(Pending::Quit), "unsaved work: ask before quitting");
        assert!(!app.may_close);
        app.pending = None;
        assert!(app.save(Backup::Always));
        app.request(Pending::Close);
        assert!(app.tracks[0].regions.is_empty());
        assert_eq!(app.project_name(), "Untitled");
        assert!(app.status.contains("Demo closed"), "{}", app.status);
        app.request(Pending::Quit);
        assert!(app.may_close, "nothing unsaved: quit straight away");
    }

    #[test]
    fn counter_shows_minutes_seconds_and_tenths() {
        assert_eq!(format_counter(0.0), "00:00.0");
        assert_eq!(format_counter(61.25), "01:01.2");
        assert_eq!(format_counter(-3.0), "-00:03.0");
        assert_eq!(format_counter(-0.04), "00:00.0");
    }

    #[test]
    fn fader_taper_gives_most_travel_around_zero() {
        assert_eq!(fader_position(MIN_DB), 0.0);
        assert_eq!(fader_position(MAX_DB), 1.0);
        assert!((fader_position(0.0) - 0.74).abs() < 1e-6);
        for db in [-55.0, -35.0, -12.5, -1.0, 0.0, 3.3, 5.5] {
            assert!((fader_db(fader_position(db)) - db).abs() < 1e-4, "{db}");
        }
        // Between -10 and +5 dB there is far more travel per dB than at the quiet end.
        let near_zero = (fader_position(5.0) - fader_position(-10.0)) / 15.0;
        let quiet = (fader_position(-40.0) - fader_position(MIN_DB)) / 20.0;
        assert!(near_zero > 3.0 * quiet);
        // About as fine as Logic around 0 dB: at least 10 points of travel per dB.
        let around_zero = (fader_position(3.0) - fader_position(-3.0)) / 6.0;
        assert!(around_zero * FADER_HEIGHT >= 10.0, "{}", around_zero * FADER_HEIGHT);
        // The meter scale runs from -60 at the bottom to 0 dBFS at the top.
        assert_eq!(scale_position(&METER_TAPER, MIN_DB), 0.0);
        assert_eq!(scale_position(&METER_TAPER, 0.0), 1.0);
        assert_eq!(scale_position(&METER_TAPER, 3.0), 1.0);
        assert_eq!(format_db(MIN_DB), "-∞ dB");
        assert_eq!(format_db(-3.0), "-3.0 dB");
    }

    #[test]
    fn db_and_gain_round_trip() {
        assert_eq!(db_to_gain(MIN_DB), 0.0);
        assert!((db_to_gain(0.0) - 1.0).abs() < 1e-6);
        assert!((gain_to_db(db_to_gain(-12.0)) + 12.0).abs() < 1e-4);
    }
}
