//! The egui interface of DAW 388. Platform-specific parts (audio devices, file dialogs) come in
//! through the [`Platform`] trait, so the same UI can later run in the browser.

mod export;
mod recording;

use daw388_engine::{
    ChannelParams, Command, EQ_DEFAULTS, EchoParams, EchoSync, EngineHandle, EqBand, Listen, Meter, MixerParams, MonitorMode,
    MonitorParams, Motion, PITCH_RANGE, PlateParams, RecMode, RtnParams, Source, Speed, TRACKS, TapeKind, TapeParams, TrackAudio,
    TransportStyle,
};
use daw388_project::export::{BitDepth, ExportFormat};
use daw388_project::preferences::{self, Preferences};
use daw388_project::session::{self, PROJECT_FILE};
use daw388_project::{
    Backup, BandEntry, ChannelEntry, ChannelSource, EchoSyncEntry, EqEntry, KindEntry, ListenSwitch, MeterSwitch, MetronomeSettings,
    MixerEntry, MonSwitch, MonitorEntry, MonoClip, ProjectSettings, RepeatMode, Routing, SpeedEntry, TapeMode, TapeSettings, TrackToSave,
    TransportSettings,
};
use egui::{Align, Color32, CornerRadius, Frame, Key, KeyboardShortcut, Layout, Margin, Modifiers, RichText, Sense, Stroke, Ui, vec2};
use export::{ExportJob, Progress, StemKind};
use recording::{Composed, TakeDone, Underneath};
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

/// Recordings that can be undone with Cmd+Z.
const UNDO_STEPS: usize = 10;

/// What a track held before a recording, so the recording can be undone.
struct UndoEntry {
    track: usize,
    source: Option<PathBuf>,
    clip: Option<Arc<MonoClip>>,
    audio: Option<(Arc<TrackAudio>, u32)>,
}

/// A take laid over its track, back from the background job.
struct ComposeResult {
    track: usize,
    generation: u64,
    outcome: Result<Composed, String>,
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

/// A finished background load or resample for one track.
struct LoadResult {
    index: usize,
    generation: u64,
    /// The track's audio as decoded and, for the engine, at `rate`.
    outcome: Result<(Arc<MonoClip>, Arc<TrackAudio>), String>,
    rate: u32,
    /// The file came from a project being opened: keep the reference when it is missing.
    from_project: bool,
    /// A new file (not a resample of audio already on the track).
    fresh: bool,
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

/// Colours from mockup A2.
mod palette {
    use egui::Color32;
    pub const BACKGROUND: Color32 = Color32::from_rgb(0x1a, 0x17, 0x16);
    pub const WOOD: Color32 = Color32::from_rgb(0x3a, 0x1d, 0x1a);
    pub const PANEL: Color32 = Color32::from_rgb(0x2f, 0x2f, 0x31);
    pub const PANEL_DARK: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x21);
    pub const EDGE: Color32 = Color32::from_rgb(0x1b, 0x1b, 0x1c);
    pub const TEXT: Color32 = Color32::from_rgb(0xd9, 0xd3, 0xc6);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0xb9, 0xb2, 0xa4);
    pub const CREAM: Color32 = Color32::from_rgb(0xef, 0xe6, 0xcc);
    pub const LED: Color32 = Color32::from_rgb(0xff, 0x5a, 0x3c);
    pub const RED: Color32 = Color32::from_rgb(0xc8, 0x40, 0x2a);
    pub const ORANGE: Color32 = Color32::from_rgb(0xe5, 0x83, 0x2c);
    pub const GREEN: Color32 = Color32::from_rgb(0x9f, 0xd3, 0x9a);
    pub const BLUE: Color32 = Color32::from_rgb(0x7f, 0xa8, 0xcc);
    pub const KEY: Color32 = Color32::from_rgb(0x9a, 0x9a, 0x9c);
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
    clip: Option<Arc<MonoClip>>,
    /// A file is being decoded for this track.
    loading: bool,
    /// Identifies the latest load request, so a late result for an older one is ignored.
    generation: u64,
    /// The file the clip came from; after saving, its copy in the project's `audio/` folder.
    source: Option<PathBuf>,
    /// The project refers to a file that could not be loaded.
    missing: bool,
    fader_db: f32,
    pan: f32,
    mute: bool,
    /// Displayed meter level in dB, with a slow fall.
    meter_db: f32,
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
    /// What the engine plays for this track, and at which rate (a take is laid over this).
    audio: Option<(Arc<TrackAudio>, u32)>,
    /// Changes whenever the track gets other content (load, Wis, undo), but not on a resample;
    /// a take finished for older content is not laid over newer content.
    version: u64,
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
    master_meter_db: [f32; 2],
    /// Displayed AUX and EFF meter levels in dB (the METER switch).
    send_meter_db: [f32; 2],
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
    /// Background loads report back here.
    loads_tx: Sender<LoadResult>,
    loads_rx: Receiver<LoadResult>,
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
    undo: Vec<UndoEntry>,
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
            master_meter_db: [MIN_DB; 2],
            send_meter_db: [MIN_DB; 2],
            status: String::new(),
            project_dir: None,
            dirty: false,
            saved_label: None,
            autosave_at: None,
            now: 0.0,
            pending: None,
            may_close: false,
            title: String::new(),
            loads_tx,
            loads_rx,
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
            undo: Vec::new(),
            metronome: MetronomeSettings::default(),
            taps: Vec::new(),
        };
        for (index, track) in app.tracks.iter_mut().enumerate() {
            track.meter_db = MIN_DB;
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
                    status = format!("{status}. Ingang: {error}");
                }
                (engine, Some(info), status)
            }
            Err(first) => {
                let fallback = OutputRequest { device: None, ..request.clone() };
                match request.device.is_some().then(|| self.platform.open_output(&fallback)) {
                    Some(Ok((engine, info))) => {
                        let status = format!("{first}, nu via {} ({})", info.device, describe_output(&info));
                        (engine, Some(info), status)
                    }
                    other => {
                        let reason = match other {
                            Some(Err(second)) => second,
                            _ => first,
                        };
                        let engine = self.platform.open_silent(request.sample_rate.unwrap_or(SILENT_RATE));
                        (engine, None, format!("Geen audio-uitvoer ({reason}): het transport loopt door zonder geluid"))
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
        for index in 0..TRACKS {
            self.request_resample(index);
        }
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
            self.status = format!("{gone} is weggevallen. {}", self.status);
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

    /// Start loading a WAV or AIFF file onto a track in the background; [`Self::poll_loads`]
    /// puts it on the track when it is ready.
    fn request_load(&mut self, index: usize, path: &Path, from_project: bool) {
        let generation = self.next_generation;
        self.next_generation += 1;
        let track = &mut self.tracks[index];
        track.generation = generation;
        track.version = generation;
        track.loading = true;
        track.missing = false;
        track.source = Some(path.to_path_buf());
        let (path, rate, tx, ctx) = (path.to_path_buf(), self.engine_rate(), self.loads_tx.clone(), self.ctx.clone());
        self.platform.spawn(Box::new(move || {
            let outcome = daw388_project::load_audio(&path)
                .map(|clip| {
                    let audio = Arc::new(TrackAudio { name: clip.name.clone(), samples: clip.at_rate(rate) });
                    (Arc::new(clip), audio)
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(LoadResult { index, generation, outcome, rate, from_project, fresh: true });
            ctx.request_repaint();
        }));
    }

    /// Resample a track's audio for the current device in the background.
    fn request_resample(&mut self, index: usize) {
        let Some(clip) = self.tracks[index].clip.clone() else { return };
        let generation = self.next_generation;
        self.next_generation += 1;
        self.tracks[index].generation = generation;
        let (rate, tx, ctx) = (self.engine_rate(), self.loads_tx.clone(), self.ctx.clone());
        self.platform.spawn(Box::new(move || {
            let audio = Arc::new(TrackAudio { name: clip.name.clone(), samples: clip.at_rate(rate) });
            let _ = tx.send(LoadResult { index, generation, outcome: Ok((clip, audio)), rate, from_project: false, fresh: false });
            ctx.request_repaint();
        }));
    }

    /// Put finished background loads on their tracks.
    pub fn poll_loads(&mut self) {
        while let Ok(result) = self.loads_rx.try_recv() {
            let index = result.index;
            if self.tracks.get(index).is_none_or(|t| t.generation != result.generation) {
                continue;
            }
            self.tracks[index].loading = false;
            match result.outcome {
                Ok((clip, audio)) => {
                    if result.fresh && !result.from_project {
                        self.status = format!("Spoor {}: {} geladen ({:.1} s)", index + 1, clip.name, clip.duration_seconds());
                    }
                    self.tracks[index].clip = Some(clip);
                    if result.rate == self.engine_rate() {
                        self.tracks[index].audio = Some((Arc::clone(&audio), result.rate));
                        self.engine_send(Command::SetTrack(index, Some(audio)));
                    } else {
                        // The device changed while this was loading.
                        self.request_resample(index);
                    }
                }
                Err(e) if result.from_project => {
                    log::warn!("track {}: {e}", index + 1);
                    self.tracks[index].missing = true;
                    let missing: Vec<String> =
                        self.tracks.iter().enumerate().filter(|(_, t)| t.missing).map(|(i, _)| (i + 1).to_string()).collect();
                    self.status = format!("{}: audio ontbreekt op spoor {}", self.project_name(), missing.join(", "));
                }
                Err(e) => {
                    let name = self.tracks[index].source.take().map(|p| p.display().to_string()).unwrap_or_default();
                    self.status = format!("{name}: {e}");
                }
            }
        }
    }

    /// Whether any track is still loading.
    pub fn is_loading(&self) -> bool {
        self.tracks.iter().any(|t| t.loading)
    }

    /// Load up to eight files onto the tracks, starting at track 1.
    fn load_all(&mut self) {
        let paths = self.platform.pick_audio_files(true);
        self.load_paths(paths);
    }

    /// Load files onto tracks 1 to 8 in name order (for example `01_Kick.wav`, `02_Snare.wav`).
    pub fn load_paths(&mut self, mut paths: Vec<PathBuf>) {
        paths.sort();
        for (index, path) in paths.iter().take(TRACKS).enumerate() {
            self.request_load(index, path, false);
            self.mark_dirty();
        }
        if paths.len() > TRACKS {
            self.status = format!("{} bestanden gekozen, alleen de eerste {TRACKS} zijn geladen", paths.len());
        }
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

    fn clear_track(&mut self, index: usize) {
        let generation = self.next_generation;
        self.next_generation += 1;
        let track = &mut self.tracks[index];
        track.clip = None;
        track.audio = None;
        track.source = None;
        track.missing = false;
        track.loading = false;
        // A load still running for this track is dropped when it arrives.
        track.generation = generation;
        track.version = generation;
        if let Some(engine) = &mut self.engine {
            engine.send(Command::SetTrack(index, None));
        }
        self.mark_dirty();
    }

    fn mark_dirty(&mut self) {
        if !self.dirty {
            self.dirty = true;
            self.autosave_at = Some(self.now + AUTOSAVE_AFTER);
        }
    }

    fn project_name(&self) -> String {
        self.project_dir.as_deref().map_or_else(|| "Naamloos".into(), session::project_name)
    }

    /// Put every track and the mix back to an empty project.
    fn reset(&mut self) {
        for index in 0..TRACKS {
            let meter_db = self.tracks[index].meter_db;
            // A fresh generation drops any load still running for the old project.
            let generation = self.next_generation;
            self.next_generation += 1;
            self.tracks[index] = TrackState {
                meter_db,
                trk_meter_db: MIN_DB,
                generation,
                version: generation,
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
        self.undo.clear();
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
        self.status = "Nieuw project".into();
    }

    fn open_project(&mut self, path: &Path) {
        let opened = match session::open(path) {
            Ok(opened) => opened,
            Err(e) => {
                self.status = format!("Openen mislukt: {e}");
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
            if let Some(source) = opened.track_source(entry) {
                self.request_load(index, &source, true);
            }
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
        self.send_mix();
        self.send_transport();
        self.project_dir = Some(opened.dir);
        self.dirty = false;
        self.autosave_at = None;
        self.status = format!("{} geopend", self.project_name());
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
                source: t.source.clone(),
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
        };
        match session::save(&dir, &settings, &tracks, backup) {
            Ok(saved) => {
                for (track, source) in self.tracks.iter_mut().zip(saved.sources) {
                    track.source = source;
                }
                self.dirty = false;
                self.autosave_at = None;
                let what = if backup == Backup::Always { "Opgeslagen" } else { "Automatisch opgeslagen" };
                self.saved_label = Some(format!("{what} {}", session::clock_time()));
                true
            }
            Err(e) => {
                self.status = format!("Opslaan mislukt: {e}");
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
            self.status = format!(
                "Project opgeslagen in {}",
                self.project_dir.as_deref().map(Path::display).map(|d| d.to_string()).unwrap_or_default()
            );
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
                self.status = format!("{name} gesloten");
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
        let response = egui::Modal::new(egui::Id::unique("confirm-simple")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("Naar de eenvoudige stand?").strong().size(16.0));
            ui.add_space(6.0);
            ui.label(
                "De bussen, ASSIGN en de MONITOR-instellingen gaan terug naar de eenvoudige stand. Fader, PAN, EQ, TRIM en de ingang blijven staan.",
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Doorgaan").clicked() {
                    answer = Some(true);
                }
                if ui.button("Annuleren").clicked() {
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
        let response = egui::Modal::new(egui::Id::unique("unsaved-changes")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(RichText::new(format!("Wijzigingen in {} opslaan?", self.project_name())).strong().size(16.0));
            ui.add_space(6.0);
            ui.label("Als je niet opslaat, gaan de wijzigingen sinds de laatste keer opslaan verloren.");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Opslaan").clicked() {
                    answer = Some(true);
                }
                if ui.button("Niet opslaan").clicked() {
                    answer = Some(false);
                }
                if ui.button("Annuleren").clicked() {
                    self.pending = None;
                }
            });
        });
        if response.should_close() && answer.is_none() {
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
        if ui.input_mut(|i| i.consume_shortcut(&shortcut(Key::Z))) {
            self.undo_recording();
        }
    }

    fn edit_menu(&mut self, ui: &mut Ui) {
        let undo = ui.ctx().format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z));
        ui.menu_button("Bewerken", |ui| {
            let label = match self.undo.last() {
                Some(entry) => format!("Opname op spoor {} ongedaan maken", entry.track + 1),
                None => "Opname ongedaan maken".to_string(),
            };
            if ui.add_enabled(!self.undo.is_empty(), egui::Button::new(label).shortcut_text(undo)).clicked() {
                self.undo_recording();
            }
        });
    }

    fn file_menu(&mut self, ui: &mut Ui) {
        let command = |ui: &Ui, key| ui.ctx().format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, key));
        ui.menu_button("Bestand", |ui| {
            if ui.add(egui::Button::new("Nieuw").shortcut_text(command(ui, Key::N))).clicked() {
                self.request(Pending::New);
            }
            if ui.add(egui::Button::new("Openen...").shortcut_text(command(ui, Key::O))).clicked() {
                self.request(Pending::Open(None));
            }
            ui.separator();
            if ui.add(egui::Button::new("Opslaan").shortcut_text(command(ui, Key::S))).clicked() {
                self.save(Backup::Always);
            }
            let save_as = ui.ctx().format_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S));
            if ui.add(egui::Button::new("Opslaan als...").shortcut_text(save_as)).clicked() {
                self.save_as();
            }
            ui.separator();
            if ui.button("8 audiobestanden laden...").clicked() {
                self.load_all();
            }
            if ui.add(egui::Button::new("Instellingen...").shortcut_text(command(ui, Key::Comma))).clicked() {
                self.show_settings = true;
            }
            if ui.button("Tape-instellingen...").clicked() {
                self.show_tape = true;
            }
            if ui.button("Exporteren...").clicked() {
                self.show_export = true;
            }
            ui.separator();
            if ui.add(egui::Button::new("Project sluiten").shortcut_text(command(ui, Key::W))).clicked() {
                self.request(Pending::Close);
            }
            if ui.add(egui::Button::new("Stop DAW 388").shortcut_text(command(ui, Key::Q))).clicked() {
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
            self.status = "REPEAT loopt van het nulpunt naar het CUE-punt: zet eerst een CUE-punt na het nulpunt".into();
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
        self.tracks[index].armed = armed;
        self.engine_send(Command::SetArmed(index, armed));
        if armed && !self.feeds(index) && self.mixer.mode == Routing::Simple {
            self.status = format!("Spoor {} krijgt niets binnen: kies een ingang", index + 1);
        } else if armed && !self.feeds(index) {
            self.status = format!(
                "Spoor {} krijgt niets binnen: zet DIRECT aan met een ingang (MIC of LINE), of ASSIGN een kanaal naar bus {}",
                index + 1,
                index + 1
            );
        }
    }

    /// REC: check what recording needs, then start it.
    fn record(&mut self) {
        if self.vari && self.pitch_percent.abs() > 0.001 {
            self.status = "Opnemen gaat op normale snelheid: zet PITCH op FIX of op 0%".into();
            return;
        }
        if self.project_dir.is_none() {
            self.status = "Sla het project eerst op: de opname komt in de projectmap".into();
            if !self.save_as() {
                return;
            }
        }
        let silent: Vec<String> = (0..TRACKS).filter(|&i| self.tracks[i].armed && !self.feeds(i)).map(|i| (i + 1).to_string()).collect();
        if !silent.is_empty() {
            self.status = format!("Let op: spoor {} krijgt niets binnen en neemt stilte op", silent.join(", "));
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
            self.status = "AUTO PUNCH neemt op van het nulpunt tot het CUE-punt: zet eerst een CUE-punt na het nulpunt".into();
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
            self.status = "Let op: de opname haperde, er is audio verloren gegaan".into();
        }
    }

    /// Lay the next queued take over its track in the background (one at a time per track, so a
    /// take is always laid over the one before it).
    fn start_compose(&mut self, track: usize) {
        if self.composing[track] {
            return;
        }
        let Some(take) = self.queued_takes[track].pop_front() else { return };
        let Some(dir) = self.project_dir.clone() else {
            self.status = format!("Opname op spoor {} niet bewaard: het project heeft nog geen map", track + 1);
            return;
        };
        let audio_dir = dir.join(session::AUDIO_DIR);
        if let Err(e) = std::fs::create_dir_all(&audio_dir) {
            self.status = format!("{}: {e}", audio_dir.display());
            return;
        }
        let state = &self.tracks[track];
        let dest = daw388_project::take::take_path(&audio_dir, track + 1, state.clip.as_ref().map(|c| c.name.as_str()));
        let underneath = match (&state.audio, &state.clip) {
            (Some((audio, rate)), _) if *rate == take.rate => Underneath::Audio(Arc::clone(audio)),
            (_, Some(clip)) => Underneath::Clip(Arc::clone(clip)),
            _ => Underneath::Nothing,
        };
        let (generation, tx, ctx) = (state.version, self.composed_tx.clone(), self.ctx.clone());
        self.composing[track] = true;
        self.platform.spawn(Box::new(move || {
            let outcome = recording::compose(&take, &underneath, &dest);
            let _ = tx.send(ComposeResult { track, generation, outcome });
            ctx.request_repaint();
        }));
    }

    /// Put laid-down takes on their tracks.
    pub fn poll_composed(&mut self) {
        while let Ok(result) = self.composed_rx.try_recv() {
            let track = result.track;
            self.composing[track] = false;
            match result.outcome {
                Ok(composed) if self.tracks[track].version == result.generation => {
                    let state = &mut self.tracks[track];
                    self.undo.push(UndoEntry { track, source: state.source.clone(), clip: state.clip.clone(), audio: state.audio.clone() });
                    if self.undo.len() > UNDO_STEPS {
                        self.undo.remove(0);
                    }
                    let seconds = composed.clip.duration_seconds();
                    state.source = Some(composed.path);
                    state.clip = Some(composed.clip);
                    state.missing = false;
                    state.audio = Some((Arc::clone(&composed.audio), composed.rate));
                    if composed.rate == self.engine_rate() {
                        self.engine_send(Command::SetTrack(track, Some(composed.audio)));
                    } else {
                        self.request_resample(track);
                    }
                    self.status = format!("Spoor {}: opname vastgelegd ({seconds:.1} s)", track + 1);
                    self.mark_dirty();
                }
                Ok(composed) => {
                    self.status =
                        format!("Spoor {} kreeg intussen andere audio; de opname staat in {}", track + 1, composed.path.display());
                }
                Err(e) => self.status = format!("Opname op spoor {} niet bewaard: {e}", track + 1),
            }
            self.start_compose(track);
        }
    }

    /// Cmd+Z: put back what a track held before its last recording.
    fn undo_recording(&mut self) {
        let Some(entry) = self.undo.pop() else {
            self.status = "Er is geen opname om ongedaan te maken".into();
            return;
        };
        let track = entry.track;
        let version = self.next_generation;
        self.next_generation += 1;
        let rate = self.engine_rate();
        let state = &mut self.tracks[track];
        state.source = entry.source;
        state.clip = entry.clip;
        state.audio = entry.audio.clone();
        state.missing = false;
        state.version = version;
        match entry.audio {
            Some((audio, audio_rate)) if audio_rate == rate => {
                self.engine_send(Command::SetTrack(track, Some(audio)));
            }
            _ if self.tracks[track].clip.is_some() => self.request_resample(track),
            _ => {
                self.engine_send(Command::SetTrack(track, None));
            }
        }
        self.status = format!("Opname op spoor {} ongedaan gemaakt", track + 1);
        self.mark_dirty();
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
        ui.label(RichText::new("Metronoom").strong());
        ui.add_space(4.0);
        let mut metronome = self.metronome;
        egui::Grid::new("metronome-settings").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Volume");
            ui.add(egui::Slider::new(&mut metronome.volume_db, -40.0..=6.0).suffix(" dB").fixed_decimals(1));
            ui.end_row();
            ui.label("Aftellen");
            let text = |bars: u8| match bars {
                0 => "Uit".to_string(),
                1 => "1 maat".to_string(),
                n => format!("{n} maten"),
            };
            egui::ComboBox::from_id_salt("count-in").selected_text(text(metronome.count_in)).width(120.0).show_ui(ui, |ui| {
                for bars in [0, 1, 2] {
                    ui.selectable_value(&mut metronome.count_in, bars, text(bars));
                }
            });
            ui.end_row();
            ui.label("Klikken");
            ui.checkbox(&mut metronome.always, "Ook tijdens gewoon afspelen (anders alleen bij opnemen)");
            ui.end_row();
        });
        ui.label(
            RichText::new(
                "Maat 1 begint aan het begin van de band, dus RESET verschuift het ritme niet. De klik is alleen te horen \
                 en komt nooit in een opname. Aftellen gebeurt bij REC vanuit stop; de band wacht en start dan op de tel. \
                 Tempo, maatsoort en deze instellingen worden per project bewaard.",
            )
            .size(11.0)
            .color(palette::TEXT_DIM),
        );
        self.set_metronome(metronome);
    }

    fn update_meters(&mut self, dt: f32) {
        let Some(engine) = &mut self.engine else { return };
        engine.collect_garbage();
        // VU-like fall of about 20 dB per second.
        let fall = 20.0 * dt;
        let now = self.now;
        for (index, track) in self.tracks.iter_mut().enumerate() {
            let peaks = engine.take_track_peaks(index);
            // The PRE peak lies after the EQ: where the 388's OL looks.
            if overload(peaks.pre) {
                track.ol_until = now + OL_HOLD;
            }
            let peak = gain_to_db(if self.meter_post { peaks.post } else { peaks.pre });
            track.meter_db = peak.max(track.meter_db - fall);
            let trk = gain_to_db(engine.take_trk_peak(index));
            track.trk_meter_db = trk.max(track.trk_meter_db - fall);
        }
        let master = engine.take_master_peaks();
        for (shown, peak) in self.master_meter_db.iter_mut().zip(master) {
            *shown = gain_to_db(peak).max(*shown - fall);
        }
        let sends = engine.take_send_peaks();
        for (shown, peak) in self.send_meter_db.iter_mut().zip(sends) {
            *shown = gain_to_db(peak).max(*shown - fall);
        }
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

    fn header(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            self.file_menu(ui);
            self.edit_menu(ui);
            let name = format!("{}{}", self.project_name(), if self.dirty { " (gewijzigd)" } else { "" });
            ui.label(RichText::new(name).color(palette::TEXT_DIM).size(13.0));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (mode, label, tip) in [
                    (Routing::Simple, "Eenvoudig", "Eén strook per spoor: ingang, REC en fader, zoals in de meeste DAW's"),
                    (Routing::Classic, "388", "De routering van de 388: bussen, ASSIGN, DIRECT en de MONITOR-sectie"),
                ] {
                    let on = self.mixer.mode == mode;
                    let text = RichText::new(label).size(13.0).color(if on { Color32::BLACK } else { palette::TEXT });
                    let button = egui::Button::new(text).fill(if on { palette::CREAM } else { palette::PANEL_DARK });
                    if ui.add(button).on_hover_text(tip).clicked() {
                        self.set_routing(mode);
                    }
                }
            });
            ui.separator();
            ui.label(RichText::new("Uitvoer").color(palette::TEXT_DIM));
            if let Some(choice) = self.device_combo(ui, "output-device") {
                self.set_output(Preferences { output_device: choice, ..self.preferences.clone() });
            }
            if ui.button("Vernieuw").on_hover_text("Lijst met audioapparaten opnieuw ophalen").clicked() {
                self.devices = self.platform.output_devices();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(saved) = &self.saved_label
                    && !self.dirty
                {
                    ui.label(RichText::new(saved).color(palette::GREEN).size(13.0));
                    ui.separator();
                }
                let colour = if self.engine.is_some() { palette::TEXT_DIM } else { palette::LED };
                ui.add(egui::Label::new(RichText::new(&self.status).color(colour).size(13.0)).truncate());
            });
        });
    }

    fn channel_strip(&mut self, ui: &mut Ui, index: usize) {
        let mut load = false;
        let mut clear = false;
        let mut arm = false;
        let mut changed = false;
        let mut solo = false;
        let recording = self.engine.as_ref().is_some_and(|e| e.is_recording(index));
        let blink = (self.now * 2.0).fract() < 0.5;
        let now = self.now;
        let input_channels = self.output.as_ref().and_then(|o| o.input.as_ref()).map_or(0, |i| i.channels);
        // The simple mode shows one strip per track: input, EQ, PAN, REC and fader.
        let simple = self.mixer.mode == Routing::Simple;
        let track = &mut self.tracks[index];
        column(ui, 112.0, |ui| {
            ui.label(RichText::new(format!("{}", index + 1)).strong().size(18.0).color(palette::CREAM));
            let (name, colour) = match (&track.clip, &track.source) {
                (_, _) if track.loading => ("laden...".to_string(), palette::ORANGE),
                (Some(clip), _) => (clip.name.clone(), palette::TEXT_DIM),
                (None, Some(source)) if track.missing => {
                    (format!("ontbreekt: {}", source.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()), palette::LED)
                }
                _ => ("leeg".to_string(), palette::TEXT_DIM),
            };
            let label = ui.add(egui::Label::new(RichText::new(name).size(11.0).color(colour)).truncate());
            if track.missing
                && let Some(source) = &track.source
            {
                label.on_hover_text(format!("Niet gevonden: {}", source.display()));
            }
            ui.horizontal(|ui| {
                if ui.small_button("Laad").on_hover_text("WAV- of AIFF-bestand op dit spoor laden").clicked() {
                    load = true;
                }
                if (track.clip.is_some() || track.missing) && ui.small_button("Wis").clicked() {
                    clear = true;
                }
            });
            ui.add_space(4.0);
            // INPUT: MIC, LINE or RMX (the tape), the selector at the top of a 388 channel. The
            // simple mode plays the tape by itself, so it has no RMX.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (source, label, tip) in [
                    (ChannelSource::Mic, "MIC", "De ingang via de voorversterker, die boven ongeveer -6 dBFS zacht verzadigt"),
                    (ChannelSource::Line, "LINE", "De ingang, schoon"),
                    (ChannelSource::Rmx, "RMX", "De weergave van dit spoor"),
                ] {
                    if simple && source == ChannelSource::Rmx {
                        continue;
                    }
                    let on = track.channel.source == source;
                    if toggle(ui, label, on, palette::CREAM).on_hover_text(tip).clicked() && !on {
                        track.channel.source = source;
                        let range = trim_range(source);
                        track.channel.trim_db = track.channel.trim_db.clamp(*range.start(), *range.end());
                        changed = true;
                    }
                }
            });
            if track.channel.source != ChannelSource::Rmx || track.channel.auto {
                let text = |c: Option<u16>| c.map_or("geen ingang".to_string(), |c| format!("in {c}"));
                let mut input = track.input;
                egui::ComboBox::from_id_salt(("strip-input", index)).selected_text(text(input)).width(76.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut input, None, text(None));
                    for c in 1..=input_channels {
                        ui.selectable_value(&mut input, Some(c), text(Some(c)));
                    }
                });
                if input != track.input {
                    track.input = input;
                    changed = true;
                }
            }
            ui.horizontal(|ui| {
                let trim = knob(ui, &mut track.channel.trim_db, trim_range(track.channel.source), 0.0, palette::RED, 30.0, true)
                    .on_hover_text(if track.channel.auto {
                        format!("TRIM {:+.1} dB: alleen voor de ingang (dubbelklik: 0 dB)", track.channel.trim_db)
                    } else {
                        format!("TRIM {:+.1} dB (dubbelklik: 0 dB)", track.channel.trim_db)
                    });
                changed |= trim.changed() || trim.double_clicked();
                let lit = now < track.ol_until;
                let (dot, ol) = ui.allocate_exact_size(vec2(8.0, 30.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 3.0, if lit { palette::LED } else { palette::PANEL_DARK });
                ol.on_hover_text("OL: piek na de EQ boven -3 dBFS");
                let pan_tip = if simple {
                    format!("PAN {:+.2} (dubbelklik: midden)", track.pan)
                } else {
                    format!("PAN {:+.2}: links naar de oneven bus, rechts naar de even bus (dubbelklik: midden)", track.pan)
                };
                let pan = knob(ui, &mut track.pan, -1.0..=1.0, 0.0, palette::CREAM, 30.0, true).on_hover_text(pan_tip);
                changed |= pan.changed() || pan.double_clicked();
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("TRIM").size(9.0).color(palette::TEXT_DIM));
                ui.add_space(14.0);
                ui.label(RichText::new("PAN").size(9.0).color(palette::TEXT_DIM));
            });
            // EQ: HIGH, MID and LOW top to bottom on the 388; here LOW to HIGH from left to right.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for band in 0..3 {
                    let entry = match band {
                        0 => &mut track.channel.eq.low,
                        1 => &mut track.channel.eq.mid,
                        _ => &mut track.channel.eq.high,
                    };
                    let response = eq_knob(ui, entry, &EQ_RANGES[band], EQ_DEFAULTS[band].freq, 34.0);
                    let tip = format!(
                        "{} {} {:+.1} dB. Midden: gain, ring: frequentie. Dubbelklik zet terug.",
                        EQ_NAMES[band],
                        format_freq(entry.freq_hz),
                        entry.gain_db
                    );
                    changed |= response.on_hover_text(tip).changed();
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for name in EQ_NAMES {
                    ui.add_sized(vec2(34.0, 10.0), egui::Label::new(RichText::new(name).size(8.0).color(palette::TEXT_DIM)));
                }
            });
            // The sends: AUX (PRE or POST) and EFF, in both modes.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let channel = &mut track.channel;
                let aux = knob(ui, &mut channel.aux_db, MIN_DB..=MAX_DB, MIN_DB, palette::GREEN, 26.0, true).on_hover_text(format!(
                    "AUX {}: PRE voor EQ en fader, POST na de fader (dubbelklik: dicht)",
                    format_db(channel.aux_db)
                ));
                changed |= aux.changed() || aux.double_clicked();
                let tip = "PRE: de AUX-send hangt niet af van EQ, fader en MUTE (een koptelefoonmix)";
                if toggle(ui, "PRE", channel.aux_pre, palette::GREEN).on_hover_text(tip).clicked() {
                    channel.aux_pre = !channel.aux_pre;
                    changed = true;
                }
                let eff = knob(ui, &mut channel.eff_db, MIN_DB..=MAX_DB, MIN_DB, palette::BLUE, 26.0, true).on_hover_text(format!(
                    "EFF {}: na de fader. De effecten zelf komen in fase 6 (dubbelklik: dicht)",
                    format_db(channel.eff_db)
                ));
                changed |= eff.changed() || eff.double_clicked();
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("AUX").size(9.0).color(palette::TEXT_DIM));
                ui.add_space(42.0);
                ui.label(RichText::new("EFF").size(9.0).color(palette::TEXT_DIM));
            });
            if !simple {
                ui.label(RichText::new("ASSIGN").size(9.0).color(palette::TEXT_DIM));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for pair in 0..4 {
                        let (odd, even) = (2 * pair + 1, 2 * pair + 2);
                        let tip = format!("Naar PGM-bus {odd} (PAN links) en {even} (PAN rechts)");
                        if toggle(ui, &format!("{odd}{even}"), track.channel.assign[pair], palette::ORANGE).on_hover_text(tip).clicked() {
                            track.channel.assign[pair] = !track.channel.assign[pair];
                            changed = true;
                        }
                    }
                });
                ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let tip = "L-R: naar de stereobus. Staat er een kanaal op L-R, dan hoor je alleen de L-R-kanalen, niet de MONITOR";
                if toggle(ui, "L-R", track.channel.lr, palette::ORANGE).on_hover_text(tip).clicked() {
                    track.channel.lr = !track.channel.lr;
                    changed = true;
                }
                let tip = format!(
                    "DIRECT: spoor {} neemt dit kanaal op (na TRIM en EQ, voor de fader) in plaats van PGM-bus {}",
                    index + 1,
                    index + 1
                );
                if toggle(ui, "DIRECT", track.channel.direct, palette::GREEN).on_hover_text(tip).clicked() {
                    track.channel.direct = !track.channel.direct;
                    changed = true;
                }
                let tip = "AUTO: de band, en de ingang zodra dit spoor op REC staat (met INSERT vanaf het inprikpunt). De opname krijgt geen EQ";
                if toggle(ui, "AUTO", track.channel.auto, palette::GREEN).on_hover_text(tip).clicked() {
                    track.channel.auto = !track.channel.auto;
                    if track.channel.auto && track.channel.source == ChannelSource::Rmx {
                        track.channel.source = ChannelSource::Line;
                    }
                    changed = true;
                }
            });
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // Three keys side by side: tight padding keeps the strip at its width.
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.spacing_mut().button_padding = vec2(3.0, 1.0);
                // REC FUNCTION: blinks when ready, lit while recording, as on the 388.
                let lit = recording || (track.armed && blink);
                let text = RichText::new("REC").size(10.0).strong().color(if lit { Color32::WHITE } else { palette::LED });
                let rec =
                    ui.add(egui::Button::new(text).fill(if lit { palette::RED } else { palette::PANEL_DARK }).min_size(vec2(30.0, 18.0)));
                if rec.on_hover_text("REC FUNCTION: spoor klaar voor opname").clicked() {
                    arm = true;
                }
                let mute = ui.add(
                    egui::Button::new(RichText::new("MUTE").size(10.0).color(if track.mute { Color32::BLACK } else { palette::TEXT }))
                        .fill(if track.mute { palette::ORANGE } else { palette::PANEL_DARK })
                        .min_size(vec2(30.0, 18.0)),
                );
                if mute.clicked() {
                    track.mute = !track.mute;
                    changed = true;
                }
                // SOLO is a DAW habit, not a 388 control: it only changes what you hear.
                let text = RichText::new("SOLO").size(10.0).color(if track.solo { Color32::BLACK } else { palette::TEXT });
                let button =
                    egui::Button::new(text).fill(if track.solo { palette::CREAM } else { palette::PANEL_DARK }).min_size(vec2(30.0, 18.0));
                if ui.add(button).on_hover_text("SOLO: alleen de gesoloode sporen horen. Opnames en bussen merken er niets van").clicked() {
                    track.solo = !track.solo;
                    solo = true;
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let fader = fader(ui, &mut track.fader_db, palette::CREAM);
                if fader.changed() || fader.double_clicked() {
                    changed = true;
                }
                meter(ui, track.meter_db, true);
            });
            ui.label(RichText::new(format_db(track.fader_db)).size(11.0).color(palette::TEXT_DIM));
        });
        if arm {
            self.toggle_arm(index);
        }
        if solo {
            let on = self.tracks[index].solo;
            self.engine_send(Command::SetSolo(index, on));
        }
        if changed {
            self.send_channel(index);
            self.mark_dirty();
        }
        if load && let Some(path) = self.platform.pick_audio_files(false).first() {
            self.request_load(index, path, false);
            self.mark_dirty();
        }
        if clear {
            self.clear_track(index);
        }
    }

    /// PGM MASTERS: ODD for buses 1, 3, 5 and 7, EVEN for 2, 4, 6 and 8.
    fn pgm_masters(&mut self, ui: &mut Ui) {
        let mut changed = false;
        column(ui, 120.0, |ui| {
            ui.label(RichText::new("PGM").strong().size(14.0).color(palette::CREAM));
            ui.label(RichText::new("MASTERS").size(9.0).color(palette::TEXT_DIM));
            ui.horizontal(|ui| {
                for (label, db) in [("ODD", &mut self.mixer.odd_db), ("EVEN", &mut self.mixer.even_db)] {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(label).size(10.0).color(palette::TEXT_DIM));
                        let response = fader(ui, db, palette::ORANGE);
                        changed |= response.changed() || response.double_clicked();
                        ui.label(RichText::new(format_db(*db)).size(10.0).color(palette::TEXT_DIM));
                    });
                }
            });
        });
        if changed {
            self.send_mixer();
            self.mark_dirty();
        }
    }

    /// The MONITOR section: per track GAIN, PAN, mute and the TRK/PGM meter, plus the MON switch.
    fn monitor_section(&mut self, ui: &mut Ui) {
        let mut changed = Vec::new();
        let mut mixer_changed = false;
        column(ui, 136.0, |ui| {
            ui.label(RichText::new("MONITOR").strong().size(14.0).color(palette::CREAM));
            ui.label(RichText::new("GAIN   PAN    M   TRK/PGM").size(9.0).color(palette::TEXT_DIM));
            for (index, track) in self.tracks.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(RichText::new(format!("{}", index + 1)).size(11.0).color(palette::CREAM));
                    let monitor = &mut track.monitor;
                    let gain = knob(ui, &mut monitor.gain_db, MIN_DB..=MONITOR_MAX_DB, 0.0, palette::CREAM, 24.0, true)
                        .on_hover_text(format!("MONITOR GAIN spoor {}: {} (dubbelklik: 0 dB)", index + 1, format_db(monitor.gain_db)));
                    let pan = knob(ui, &mut monitor.pan, -1.0..=1.0, 0.0, palette::CREAM, 24.0, true)
                        .on_hover_text(format!("MONITOR PAN {:+.2} (dubbelklik: midden)", monitor.pan));
                    let mute = toggle(ui, "M", monitor.mute, palette::ORANGE);
                    if mute.clicked() {
                        monitor.mute = !monitor.mute;
                    }
                    if gain.changed() || gain.double_clicked() || pan.changed() || pan.double_clicked() || mute.clicked() {
                        changed.push(index);
                    }
                    level_bar(ui, track.trk_meter_db);
                });
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.label(RichText::new("MON").size(9.0).color(palette::TEXT_DIM));
                for (mon, label, tip) in [
                    (MonSwitch::Cue, "CUE", "Band of ingang, volgens REC FUNCTION en INSERT (zoals op de 388)"),
                    (MonSwitch::Pgm, "PGM", "Altijd de PGM-bussen"),
                ] {
                    let on = self.mixer.mon == mon;
                    if toggle(ui, label, on, palette::ORANGE).on_hover_text(tip).clicked() && !on {
                        self.mixer.mon = mon;
                        mixer_changed = true;
                    }
                }
            });
        });
        for index in &changed {
            self.send_monitor(*index);
        }
        if mixer_changed {
            self.send_mixer();
        }
        if !changed.is_empty() || mixer_changed {
            self.mark_dirty();
        }
    }

    /// The plate on AUX and the echo on EFF: a few knobs each and their EFFECT RTN.
    fn effects_controls(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        let quarter = 60.0 / self.metronome.bpm.clamp(20.0, 400.0) as f32;
        let effects = &mut self.mixer.effects;
        let labels = |ui: &mut Ui, names: &[&str]| {
            ui.horizontal(|ui| {
                for name in names {
                    ui.add_sized(vec2(34.0, 10.0), egui::Label::new(RichText::new(*name).size(8.0).color(palette::TEXT_DIM)));
                }
            });
        };
        let mut turn = |ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, default: f32, colour, enabled, tip: String| {
            let response = knob(ui, value, range, default, colour, 30.0, enabled).on_hover_text(tip);
            changed |= response.changed() || response.double_clicked();
        };

        ui.label(RichText::new("PLATE (AUX)").strong().size(12.0).color(palette::CREAM));
        let plate = &mut effects.plate;
        ui.horizontal(|ui| {
            let tip = format!("DECAY {:.1} s: hoe lang de galm duurt (dubbelklik: 2 s)", plate.decay_s);
            turn(ui, &mut plate.decay_s, 0.5..=6.0, 2.0, palette::GREEN, true, tip);
            let tip = format!("PRE-DELAY {:.0} ms: wachttijd voor de galm begint (dubbelklik: 20 ms)", plate.predelay_ms);
            turn(ui, &mut plate.predelay_ms, 0.0..=100.0, 20.0, palette::GREEN, true, tip);
            let tip = format!("TONE {:.0}%: van donker naar helder (dubbelklik: midden)", plate.tone * 100.0);
            turn(ui, &mut plate.tone, 0.0..=1.0, 0.5, palette::GREEN, true, tip);
        });
        labels(ui, &["DECAY", "PRE", "TONE"]);

        ui.label(RichText::new("ECHO (EFF)").strong().size(12.0).color(palette::CREAM));
        let echo = &mut effects.echo;
        let synced = echo.sync != EchoSyncEntry::Off;
        let sync_ms = 1000.0
            * match echo.sync {
                EchoSyncEntry::Off => echo.time_ms / 1000.0,
                EchoSyncEntry::Quarter => quarter,
                EchoSyncEntry::Eighth => quarter / 2.0,
                EchoSyncEntry::DottedEighth => quarter * 0.75,
            };
        ui.horizontal(|ui| {
            let tip = if synced {
                format!("TIME {:.0} ms, volgt het tempo (zet SYNC uit om zelf te kiezen)", sync_ms.min(2000.0))
            } else {
                format!("TIME {:.0} ms (dubbelklik: 300 ms)", echo.time_ms)
            };
            turn(ui, &mut echo.time_ms, 40.0..=800.0, 300.0, palette::BLUE, !synced, tip);
            let tip = format!("FEEDBACK {:.0}%: hoeveel herhalingen (dubbelklik: 35%)", echo.feedback * 100.0);
            turn(ui, &mut echo.feedback, 0.0..=0.9, 0.35, palette::BLUE, true, tip);
            let tip = format!("TONE {:.0}%: hoe donker de herhalingen worden (dubbelklik: midden)", echo.tone * 100.0);
            turn(ui, &mut echo.tone, 0.0..=1.0, 0.5, palette::BLUE, true, tip);
        });
        labels(ui, &["TIME", "FEEDB.", "TONE"]);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (sync, label, tip) in [
                (EchoSyncEntry::Off, "VRIJ", "TIME in milliseconden"),
                (EchoSyncEntry::Quarter, "1/4", "Een kwartnoot van het metronoomtempo"),
                (EchoSyncEntry::Eighth, "1/8", "Een achtste noot van het metronoomtempo"),
                (EchoSyncEntry::DottedEighth, "1/8.", "Een gepunteerde achtste van het metronoomtempo"),
            ] {
                let on = echo.sync == sync;
                if toggle(ui, label, on, palette::BLUE).on_hover_text(tip).clicked() && !on {
                    echo.sync = sync;
                    changed = true;
                }
            }
        });

        ui.label(RichText::new("EFFECT RTN").size(9.0).color(palette::TEXT_DIM));
        ui.horizontal(|ui| {
            for (index, name) in ["plate", "echo"].into_iter().enumerate() {
                let rtn = &mut effects.rtn[index];
                let tip = format!("RTN {} ({name}) {} (dubbelklik: 0 dB)", index + 1, format_db(rtn.gain_db));
                let response = knob(ui, &mut rtn.gain_db, MIN_DB..=MAX_DB, 0.0, palette::ORANGE, 26.0, true).on_hover_text(tip);
                changed |= response.changed() || response.double_clicked();
                let tip = format!("PAN RTN {} {:+.2} (dubbelklik: midden)", index + 1, rtn.pan);
                let response = knob(ui, &mut rtn.pan, -1.0..=1.0, 0.0, palette::CREAM, 26.0, true).on_hover_text(tip);
                changed |= response.changed() || response.double_clicked();
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for name in ["RTN 1", "PAN", "RTN 2", "PAN"] {
                ui.add_sized(vec2(28.0, 10.0), egui::Label::new(RichText::new(name).size(8.0).color(palette::TEXT_DIM)));
            }
        });
        changed
    }

    fn master_section(&mut self, ui: &mut Ui) {
        let mut mixer_changed = false;
        column(ui, 136.0, |ui| {
            mixer_changed |= self.effects_controls(ui);
            ui.add_space(6.0);
            // AUX MASTER and EFF MASTER, to their own interface outputs (Instellingen, Audio).
            ui.label(RichText::new("AUX / EFF").strong().size(14.0).color(palette::CREAM));
            ui.horizontal(|ui| {
                let aux =
                    knob(ui, &mut self.mixer.aux_master_db, MIN_DB..=MAX_DB, 0.0, palette::ORANGE, 30.0, true).on_hover_text(format!(
                        "AUX MASTER {} (dubbelklik: 0 dB). De uitgang kies je in Instellingen, Audio",
                        format_db(self.mixer.aux_master_db)
                    ));
                mixer_changed |= aux.changed() || aux.double_clicked();
                let eff =
                    knob(ui, &mut self.mixer.eff_master_db, MIN_DB..=MAX_DB, 0.0, palette::ORANGE, 30.0, true).on_hover_text(format!(
                        "EFF MASTER {} (dubbelklik: 0 dB). De uitgang kies je in Instellingen, Audio",
                        format_db(self.mixer.eff_master_db)
                    ));
                mixer_changed |= eff.changed() || eff.double_clicked();
                let phones = knob(ui, &mut self.mixer.phones_db, MIN_DB..=MAX_DB, 0.0, palette::CREAM, 30.0, true).on_hover_text(format!(
                    "MONITOR/PHONES {}: niveau van de hoofduitgang. De klik heeft zijn eigen volume (dubbelklik: 0 dB)",
                    format_db(self.mixer.phones_db)
                ));
                mixer_changed |= phones.changed() || phones.double_clicked();
            });
            ui.horizontal(|ui| {
                for name in ["AUX", "EFF", "PHONES"] {
                    ui.add_sized(vec2(34.0, 10.0), egui::Label::new(RichText::new(name).size(8.0).color(palette::TEXT_DIM)));
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (listen, label, tip) in [
                    (ListenSwitch::Stereo, "STEREO", "De hoofduitgang krijgt de STEREO MASTER"),
                    (ListenSwitch::Aux, "AUX", "De hoofduitgang krijgt AUX, mono op beide kanten"),
                    (ListenSwitch::Eff, "EFF", "De hoofduitgang krijgt EFF, mono op beide kanten"),
                ] {
                    let on = self.mixer.listen == listen;
                    if toggle(ui, label, on, palette::ORANGE).on_hover_text(tip).clicked() && !on {
                        self.mixer.listen = listen;
                        mixer_changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.label(RichText::new("METER").size(9.0).color(palette::TEXT_DIM));
                for (meter, label, tip) in [
                    (MeterSwitch::Lr, "L-R", "De stereometers tonen links en rechts"),
                    (MeterSwitch::AuxEff, "AUX-EFF", "De stereometers tonen AUX en EFF na hun master"),
                ] {
                    let on = self.mixer.meter == meter;
                    if toggle(ui, label, on, palette::ORANGE).on_hover_text(tip).clicked() && !on {
                        self.mixer.meter = meter;
                        mixer_changed = true;
                    }
                }
            });
            ui.add_space(8.0);
            ui.label(RichText::new("STEREO").strong().size(14.0).color(palette::CREAM));
            ui.add_space(4.0);
            ui.label(RichText::new("TRACK METERS").size(9.0).color(palette::TEXT_DIM));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (post, label, tip) in [
                    (false, "PRE", "Kanaalmeters tonen het niveau na TRIM, voor de fader (zoals op de 388)"),
                    (true, "POST", "Kanaalmeters tonen het niveau na fader en mute"),
                ] {
                    let lit = self.meter_post == post;
                    let text = RichText::new(label).size(10.0).color(if lit { Color32::BLACK } else { palette::TEXT });
                    let button =
                        egui::Button::new(text).fill(if lit { palette::ORANGE } else { palette::PANEL_DARK }).min_size(vec2(40.0, 18.0));
                    if ui.add(button).on_hover_text(tip).clicked() && !lit {
                        self.meter_post = post;
                        self.mark_dirty();
                    }
                }
            });
            ui.add_space(4.0);
            let mono = toggle(ui, "MONO", self.mixer.mono, palette::ORANGE).on_hover_text("Links en rechts samen op beide kanten");
            if mono.clicked() {
                self.mixer.mono = !self.mixer.mono;
                self.send_mixer();
                self.mark_dirty();
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let response = fader(ui, &mut self.master_db, palette::RED);
                if response.changed() || response.double_clicked() {
                    let gain = db_to_gain(self.master_db);
                    self.engine_send(Command::SetMaster(gain));
                    self.mark_dirty();
                }
                let shown = if self.mixer.meter == MeterSwitch::AuxEff { self.send_meter_db } else { self.master_meter_db };
                meter(ui, shown[0], true);
                meter(ui, shown[1], false);
            });
            ui.label(RichText::new(format_db(self.master_db)).size(11.0).color(palette::TEXT_DIM));
        });
        if mixer_changed {
            self.send_mixer();
            self.mark_dirty();
        }
    }

    fn transport(&mut self, ui: &mut Ui) {
        let (counter, motion) = self.engine.as_ref().map_or((0.0, Motion::Stopped), |e| (e.counter_seconds(), e.motion()));
        let rec_mode = self.engine.as_ref().map_or(RecMode::Off, EngineHandle::rec_mode);
        let blink = (self.now * 2.0).fract() < 0.5;
        let cue_counter = self.cue_seconds.map(|cue| cue - self.zero_seconds);
        let (tape, count_in) = self.engine.as_ref().map_or((0.0, 0.0), |e| (e.position_seconds(), e.count_in_seconds()));
        let meter = meter_of(&self.metronome);
        // The tape's bar grid; during the count-in it runs ahead of the waiting tape.
        let grid = tape - count_in;
        let (bar, beat) = meter.bar_and_beat(grid);
        let on_beat = (grid / meter.beat_seconds()).fract() < 0.15
            && (motion == Motion::CountingIn || (motion == Motion::Playing && self.metronome.click));
        column(ui, 262.0, |ui| {
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
                            format!("AFTELLEN  {}.{}", bar, beat)
                        } else {
                            format!("MAAT  {bar}.{beat}   {}/{}", self.metronome.beats, self.metronome.unit)
                        };
                        ui.label(RichText::new(bars).monospace().size(11.0).color(Color32::from_rgb(0xc8, 0x55, 0x3c)));
                    });
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let small = vec2(53.0, 30.0);
                if small_key(ui, "RESET", false, small).on_hover_text("Teller op 00:00 zetten: hier ligt het nulpunt").clicked() {
                    self.reset_zero();
                }
                if small_key(ui, "CUE", false, small).on_hover_text("De huidige plek als CUE-punt onthouden").clicked() {
                    self.mark_cue();
                }
                if small_key(ui, "STC", false, small).on_hover_text("Naar het CUE-punt (Search To Cue)").clicked() {
                    self.search_to_cue();
                }
                let tip = match self.repeat_mode {
                    RepeatMode::Tape => "Herhalen van nulpunt tot CUE-punt; de band spoelt steeds terug (C)",
                    RepeatMode::Cycle => "Herhalen van nulpunt tot CUE-punt; springt direct terug (C)",
                };
                if small_key(ui, "REPEAT", self.repeat, small).on_hover_text(tip).clicked() {
                    self.set_repeat(!self.repeat);
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let size = vec2(48.0, 40.0);
                if transport_key(ui, "REW", motion == Motion::Rewinding, size).on_hover_text("Terugspoelen (,)").clicked() {
                    self.rewind();
                }
                if transport_key(ui, "F FWD", motion == Motion::Forwarding, size).on_hover_text("Vooruitspoelen (.)").clicked() {
                    self.fast_forward();
                }
                if transport_key(ui, "STOP", motion == Motion::Stopped, size).on_hover_text("Stop (spatie)").clicked() {
                    self.stop();
                }
                if transport_key(ui, "PLAY", motion == Motion::Playing, size)
                    .on_hover_text("Afspelen; tijdens opnemen: punch-out (spatie)")
                    .clicked()
                {
                    self.play();
                }
                // REC: blinks when record-ready, lit while recording.
                let lit = rec_mode == RecMode::Recording || (rec_mode == RecMode::Ready && blink);
                let text = RichText::new("REC").strong().size(12.0).color(if lit { Color32::WHITE } else { palette::RED });
                let rec = egui::Button::new(text)
                    .fill(if lit { palette::RED } else { palette::KEY })
                    .min_size(size)
                    .corner_radius(CornerRadius::same(3));
                if ui.add(rec).on_hover_text("Opnemen op de sporen met REC FUNCTION (R); tijdens het afspelen: punch-in").clicked() {
                    self.record();
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let size = vec2(78.0, 22.0);
                if small_key(ui, "INSERT", self.insert, size)
                    .on_hover_text("Klaargezette sporen spelen de band tot de punch-in en schakelen dan naar de ingang")
                    .clicked()
                {
                    self.set_insert(!self.insert);
                }
                if small_key(ui, "AUTO PUNCH", self.auto_punch, size)
                    .on_hover_text("Na REC alleen opnemen van het nulpunt tot het CUE-punt (met REPEAT steeds opnieuw)")
                    .clicked()
                {
                    self.set_auto_punch(!self.auto_punch);
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let mut metronome = self.metronome;
                // Flashes on the beat while the tape runs or counts in; steady while stopped.
                let lit = if matches!(motion, Motion::Playing | Motion::CountingIn) { on_beat } else { metronome.click };
                let click = small_key(ui, "CLICK", lit, vec2(52.0, 22.0));
                if click.on_hover_text("Metronoom aan of uit (K); het lampje tikt mee").clicked() {
                    metronome.click = !metronome.click;
                }
                ui.add(egui::DragValue::new(&mut metronome.bpm).range(20.0..=400.0).speed(0.2).fixed_decimals(1).suffix(" BPM"))
                    .on_hover_text("Tempo in kwartnoten per minuut (slepen of klikken en typen)");
                if small_key(ui, "TAP", false, vec2(36.0, 22.0)).on_hover_text("Tik een paar keer in het tempo").clicked()
                    && let Some(bpm) = self.tap()
                {
                    metronome.bpm = bpm;
                }
                let signature = format!("{}/{}", metronome.beats, metronome.unit);
                egui::ComboBox::from_id_salt("time-signature").selected_text(signature).width(48.0).show_ui(ui, |ui| {
                    for (beats, unit) in TIME_SIGNATURES {
                        let chosen = (metronome.beats, metronome.unit) == (beats, unit);
                        if ui.selectable_label(chosen, format!("{beats}/{unit}")).clicked() {
                            (metronome.beats, metronome.unit) = (beats, unit);
                        }
                    }
                });
                self.set_metronome(metronome);
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let tape = small_key(ui, &tape_label(&self.tape), self.tape.mode == TapeMode::Machine, vec2(150.0, 22.0));
                if tape.on_hover_text("Het karakter van de band bij afspelen: snelheid, soort, saturatie, wow, ruis").clicked() {
                    self.show_tape = true;
                }
                let running = self.running_export.is_some();
                if small_key(ui, "EXPORT", running, vec2(70.0, 22.0))
                    .on_hover_text("Stereo bounce en losse sporen als WAV of AIFF")
                    .clicked()
                {
                    self.show_export = true;
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if transport_key(ui, "RTZ", false, vec2(53.0, 40.0)).on_hover_text("Terug naar het nulpunt (Home)").clicked() {
                    self.rtz();
                }
                ui.add_space(14.0);
                let knob = knob(ui, &mut self.pitch_percent, -PITCH_PERCENT..=PITCH_PERCENT, 0.0, palette::CREAM, 38.0, self.vari);
                let knob = knob.on_hover_text("PITCH: snelheid en toonhoogte, plus of min 15% (dubbelklik: 0%)");
                if knob.changed() || knob.double_clicked() {
                    self.send_pitch();
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new("PITCH").size(9.0).color(palette::TEXT_DIM));
                    for (vari, label, tip) in
                        [(true, "VARI", "De pitch-knop werkt"), (false, "FIX", "Normale snelheid, de pitch-knop staat vast")]
                    {
                        let lit = self.vari == vari;
                        let text = RichText::new(label).size(10.0).color(if lit { Color32::BLACK } else { palette::TEXT });
                        let button = egui::Button::new(text)
                            .fill(if lit { palette::ORANGE } else { palette::PANEL_DARK })
                            .min_size(vec2(40.0, 16.0));
                        if ui.add(button).on_hover_text(tip).clicked() && !lit {
                            self.vari = vari;
                            self.send_pitch();
                        }
                    }
                });
            });
        });
    }

    /// The output device chooser. Returns the new choice (`Some(None)`: the default device).
    fn device_combo(&self, ui: &mut Ui, id: &str) -> Option<Option<String>> {
        let selected = match (&self.output, &self.preferences.output_device) {
            (Some(output), None) => format!("Standaard ({})", output.device),
            (Some(output), Some(_)) => output.device.clone(),
            (None, _) => "Geen (stille klok)".into(),
        };
        let mut choice = None;
        egui::ComboBox::from_id_salt(id).selected_text(selected).width(260.0).show_ui(ui, |ui| {
            if ui.selectable_label(self.preferences.output_device.is_none(), "Standaardapparaat van het systeem").clicked() {
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
        ui.label(RichText::new("Audio").strong());
        ui.add_space(4.0);
        let mut wanted = self.preferences.clone();
        egui::Grid::new("audio-settings").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Uitvoer");
            if let Some(choice) = self.device_combo(ui, "settings-output-device") {
                wanted.output_device = choice;
            }
            ui.end_row();

            let info = self.chosen_device_info().cloned().unwrap_or_default();
            ui.label("Samplerate");
            let rate_text =
                |rate: Option<u32>| rate.map_or("Van het apparaat".to_string(), |r| format!("{:.1} kHz", f64::from(r) / 1000.0));
            egui::ComboBox::from_id_salt("sample-rate").selected_text(rate_text(wanted.sample_rate)).width(260.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut wanted.sample_rate, None, rate_text(None));
                for rate in &info.sample_rates {
                    ui.selectable_value(&mut wanted.sample_rate, Some(*rate), rate_text(Some(*rate)));
                }
            });
            ui.end_row();

            ui.label("Buffer");
            let buffer_text = |frames: Option<u32>| frames.map_or("Standaard van het apparaat".to_string(), |n| format!("{n} samples"));
            egui::ComboBox::from_id_salt("buffer-size").selected_text(buffer_text(wanted.buffer_frames)).width(260.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut wanted.buffer_frames, None, buffer_text(None));
                for frames in BUFFER_SIZES.iter().copied().filter(|n| info.buffer_range.is_none_or(|(min, max)| (min..=max).contains(n))) {
                    ui.selectable_value(&mut wanted.buffer_frames, Some(frames), buffer_text(Some(frames)));
                }
            });
            ui.end_row();

            ui.label("Ingang");
            let input_text = |name: &Option<String>| name.clone().unwrap_or_else(|| "Geen (niet opnemen)".into());
            egui::ComboBox::from_id_salt("input-device").selected_text(input_text(&wanted.input_device)).width(260.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut wanted.input_device, None, input_text(&None));
                for device in &self.input_devices {
                    let label = format!("{} ({} ingangen)", device.name, device.channels);
                    ui.selectable_value(&mut wanted.input_device, Some(device.name.clone()), label);
                }
            });
            ui.end_row();

            ui.label("Meeluisteren");
            ui.checkbox(&mut wanted.software_monitoring, "Via DAW 388 (uit: direct monitoring van de interface)");
            ui.end_row();

            // AUX and EFF to free outputs of the interface; 1 and 2 carry the main output.
            let output_channels = self.output.as_ref().map_or(0, |o| o.channels);
            let output_text = |c: Option<u16>| match c {
                None => "Geen".to_string(),
                Some(c) if c > output_channels => format!("Uitgang {c} (niet op dit apparaat)"),
                Some(c) => format!("Uitgang {c}"),
            };
            for (label, id, choice) in
                [("AUX-uitgang", "aux-output", &mut wanted.aux_output), ("EFF-uitgang", "eff-output", &mut wanted.eff_output)]
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
                ui.label(format!("automatisch {automatic:.1} ms, plus"));
                ui.add(egui::DragValue::new(&mut wanted.latency_offset_ms).range(-100.0..=100.0).speed(0.1).suffix(" ms"));
            });
            ui.end_row();
        });
        let channels = self.output.as_ref().and_then(|o| o.input.as_ref()).map_or(0, |i| i.channels);
        if channels > 0 {
            ui.add_space(4.0);
            ui.label(RichText::new("Ingang per kanaal (MIC en LINE)").size(12.0));
            let mut changed = Vec::new();
            egui::Grid::new("track-inputs").num_columns(4).spacing([10.0, 4.0]).show(ui, |ui| {
                for (index, track) in self.tracks.iter().enumerate() {
                    let mut input = track.input;
                    let text = |c: Option<u16>| c.map_or("geen".to_string(), |c| format!("in {c}"));
                    ui.label(format!("Kanaal {}", index + 1));
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
                    (Some(input), _) => format!("; ingang {} ({} kanalen)", input.device, input.channels),
                    (None, Some(error)) => format!("; ingang: {error}"),
                    (None, None) => String::new(),
                };
                format!("Nu: {}, {}{input}", output.device, describe_output(output))
            }
            None => "Nu: geen audio-uitvoer, het transport loopt door zonder geluid".into(),
        };
        ui.label(RichText::new(now).size(11.0).color(palette::TEXT_DIM));
        ui.label(
            RichText::new(
                "Een kleinere buffer geeft minder vertraging maar vraagt meer van de computer. Valt het apparaat weg, \
                 dan schakelt DAW 388 over op het standaardapparaat en terug zodra het er weer is. Deze instellingen gelden \
                 voor de app, niet per project.",
            )
            .size(11.0)
            .color(palette::TEXT_DIM),
        );
        self.set_output(wanted);
    }

    /// The export as the app stands now, or why it cannot start.
    fn export_job(&self) -> Result<ExportJob, String> {
        let Some(dir) = &self.project_dir else {
            return Err("Sla het project eerst op: de export komt in de map export van het project".into());
        };
        let choices = &self.export;
        let clips: [Option<Arc<MonoClip>>; TRACKS] = std::array::from_fn(|i| self.tracks[i].clip.clone());
        let longest = clips.iter().flatten().map(|c| c.duration_seconds()).fold(0.0, f64::max);
        if longest <= 0.0 {
            return Err("Er staat nog niets op de band om te exporteren".into());
        }
        let range = if choices.to_cue {
            let end = self.cue_seconds.filter(|cue| *cue > self.zero_seconds).unwrap_or(longest);
            (self.zero_seconds, end)
        } else {
            (0.0, longest)
        };
        let stems: Vec<(usize, StemKind)> = if choices.stems {
            (0..TRACKS).filter(|i| choices.stem_tracks[*i] && clips[*i].is_some()).map(|i| (i, choices.stem_kind)).collect()
        } else {
            Vec::new()
        };
        if !choices.stereo && stems.is_empty() {
            return Err("Kies een stereo bounce of losse sporen".into());
        }
        Ok(ExportJob {
            rate: choices.rate,
            clips,
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
        self.status = "Exporteren...".into();
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
                format!("{} bestand(en) geëxporteerd naar {dir}", files.len())
            }
            Err(e) => e,
        };
    }

    fn export_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_export;
        let mut choices = self.export.clone();
        let mut start = false;
        let names: [String; TRACKS] =
            std::array::from_fn(|i| self.tracks[i].clip.as_ref().map_or_else(|| "leeg".to_string(), |c| c.name.clone()));
        let running = self.running_export.as_ref().map(|r| Arc::clone(&r.progress));
        egui::Window::new("Export").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.add_enabled_ui(running.is_none(), |ui| {
                ui.checkbox(&mut choices.stereo, "Stereo bounce (de mix zoals je hem hoort)");
                ui.add_enabled_ui(choices.stereo && self.tape.mode == TapeMode::Machine, |ui| {
                    ui.checkbox(&mut choices.stereo_tape, "met tape-karakter");
                });
                ui.add_space(4.0);
                ui.checkbox(&mut choices.stems, "Losse sporen");
                ui.add_enabled_ui(choices.stems, |ui| {
                    egui::Grid::new("export-stems").num_columns(4).show(ui, |ui| {
                        for (index, name) in names.iter().enumerate() {
                            let has = self.tracks[index].clip.is_some();
                            ui.add_enabled(has, egui::Checkbox::new(&mut choices.stem_tracks[index], format!("{} {name}", index + 1)));
                            if index % 4 == 3 {
                                ui.end_row();
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        for (kind, label, tip) in [
                            (StemKind::Mixed, "met mixer en tape", "Stereo, zoals in de mix: EQ, fader, pan, en de eigen galm en echo"),
                            (StemKind::TapeOnly, "alleen tape", "Mono, het spoor zoals het van de band komt"),
                            (StemKind::Dry, "droog", "Mono, de opname zelf"),
                        ] {
                            ui.selectable_value(&mut choices.stem_kind, kind, label).on_hover_text(tip);
                        }
                    });
                });
                ui.add_space(4.0);
                ui.checkbox(&mut choices.to_cue, "Alleen van het nulpunt tot het CUE-punt");
                ui.add_enabled_ui(choices.to_cue, |ui| {
                    ui.checkbox(&mut choices.from_zero, "Alle bestanden beginnen op 0:00");
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut choices.format, ExportFormat::Wav, "WAV");
                    ui.selectable_value(&mut choices.format, ExportFormat::Aiff, "AIFF");
                    ui.separator();
                    ui.selectable_value(&mut choices.depth, BitDepth::Int16, "16 bit");
                    ui.selectable_value(&mut choices.depth, BitDepth::Int24, "24 bit");
                    ui.selectable_value(&mut choices.depth, BitDepth::Float32, "32 bit float");
                });
                ui.horizontal(|ui| {
                    for rate in [44_100, 48_000, 96_000] {
                        ui.selectable_value(&mut choices.rate, rate, format!("{},{} kHz", rate / 1000, (rate % 1000) / 100));
                    }
                });
                ui.add_space(6.0);
                let place =
                    self.project_dir.as_ref().map_or("Sla het project eerst op".to_string(), |d| d.join("export").display().to_string());
                ui.label(RichText::new(format!("Map: {place}")).size(11.0).color(palette::TEXT_DIM));
                ui.add_space(6.0);
                if ui.button("Exporteren").clicked() {
                    start = true;
                }
            });
            if let Some(progress) = &running {
                let done = progress.done.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0;
                ui.add(egui::ProgressBar::new(done).show_percentage());
                if ui.button("Annuleren").clicked() {
                    progress.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                ctx.request_repaint();
            }
        });
        self.show_export = open;
        self.export = choices;
        if start {
            self.start_export();
        }
    }

    /// Clean or 388, tape speed and kind, and four amounts from 0 to 200%.
    fn tape_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_tape;
        let mut tape = self.tape;
        egui::Window::new("Tape").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label("Het karakter van de band bij afspelen. Opnames via de ingang blijven schoon.");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut tape.mode, TapeMode::Clean, "Clean").on_hover_text("De band gaat ongemoeid door");
                ui.selectable_value(&mut tape.mode, TapeMode::Machine, "388").on_hover_text("Het karakter van de 388");
            });
            ui.add_space(6.0);
            ui.add_enabled_ui(tape.mode == TapeMode::Machine, |ui| {
                egui::Grid::new("tape-grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    ui.label("Snelheid");
                    ui.horizontal(|ui| {
                        for (speed, label) in
                            [(SpeedEntry::Ips3_75, "3¾ IPS"), (SpeedEntry::Ips7_5, "7½ IPS"), (SpeedEntry::Ips15, "15 IPS")]
                        {
                            ui.selectable_value(&mut tape.speed, speed, label);
                        }
                    });
                    ui.end_row();
                    ui.label("Tapesoort");
                    ui.horizontal(|ui| {
                        for (kind, label, tip) in [
                            (KindEntry::T456, "456", "Veel ruimte, de standaard"),
                            (KindEntry::T250, "250", "Warm, verzadigt eerder"),
                            (KindEntry::T911, "911", "Iets helderder"),
                        ] {
                            ui.selectable_value(&mut tape.kind, kind, label).on_hover_text(tip);
                        }
                    });
                    ui.end_row();
                    ui.label("dbx");
                    ui.horizontal(|ui| {
                        let tip = "Ruisonderdrukking: de ruis zakt weg in stiltes en ademt mee met de muziek";
                        ui.checkbox(&mut tape.dbx[0], "sporen 1-4").on_hover_text(tip);
                        ui.checkbox(&mut tape.dbx[1], "sporen 5-8").on_hover_text(tip);
                    });
                    ui.end_row();
                    for (label, amount) in [
                        ("Saturatie", &mut tape.saturation),
                        ("Wow & flutter", &mut tape.wow_flutter),
                        ("Ruis", &mut tape.noise),
                        ("Crosstalk", &mut tape.crosstalk),
                    ] {
                        ui.label(label);
                        let slider = egui::Slider::new(amount, 0.0..=2.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0));
                        if ui.add(slider).on_hover_text("100% is de machine zoals hij is (dubbelklik: 100%)").double_clicked() {
                            *amount = 1.0;
                        }
                        ui.end_row();
                    }
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
        egui::Window::new("Instellingen").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.set_width(440.0);
            self.audio_settings(ui);
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            self.metronome_settings(ui);
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(RichText::new("REPEAT, RTZ en STC").strong());
            ui.add_space(4.0);
            let mut mode = self.repeat_mode;
            ui.radio_value(&mut mode, RepeatMode::Tape, "388: de band spoelt in de tijd terug naar het nulpunt");
            ui.radio_value(&mut mode, RepeatMode::Cycle, "Cycle: springt direct terug, zoals in Logic");
            self.set_repeat_mode(mode);
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "REPEAT herhaalt van het nulpunt (RESET) tot het CUE-punt. In de 388-modus zet elke transporttoets \
                     behalve PLAY en RTZ REPEAT uit; in de cycle-modus blijft REPEAT aan. Deze instelling wordt per project bewaard.",
                )
                .size(11.0)
                .color(palette::TEXT_DIM),
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
        None => format!("{rate}, standaardbuffer"),
    }
}

/// A top-down column of a fixed width with centred contents.
fn column(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Center), |ui| {
        ui.set_width(width);
        add(ui);
    });
}

fn transport_key(ui: &mut Ui, label: &str, lit: bool, size: egui::Vec2) -> egui::Response {
    let text = RichText::new(label).strong().size(12.0).color(if lit { palette::LED } else { Color32::from_rgb(0x2a, 0x26, 0x22) });
    ui.add(egui::Button::new(text).fill(palette::KEY).min_size(size).corner_radius(CornerRadius::same(3)))
}

/// A memory key (RESET, CUE, STC, REPEAT); `lit` shows its LED.
fn small_key(ui: &mut Ui, label: &str, lit: bool, size: egui::Vec2) -> egui::Response {
    let text = RichText::new(label).strong().size(10.0).color(if lit { palette::LED } else { Color32::from_rgb(0x2a, 0x26, 0x22) });
    ui.add(egui::Button::new(text).fill(Color32::from_rgb(0x7e, 0x7e, 0x80)).min_size(size).corner_radius(CornerRadius::same(3)))
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
    if hz >= 1000.0 { format!("{:.1} kHz", hz / 1000.0).replace('.', ",") } else { format!("{hz:.0} Hz") }
}

/// A small latching switch, lit in `colour` when on.
fn toggle(ui: &mut Ui, label: &str, on: bool, colour: Color32) -> egui::Response {
    let text = RichText::new(label).size(9.0).strong().color(if on { Color32::BLACK } else { palette::TEXT });
    ui.add(egui::Button::new(text).fill(if on { colour } else { palette::PANEL_DARK }).min_size(vec2(20.0, 16.0)))
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

/// Scale marks printed next to the meters (dB below full scale).
const METER_MARKS: [(f32, &str); 9] =
    [(0.0, "0"), (-6.0, "6"), (-12.0, "12"), (-18.0, "18"), (-24.0, "24"), (-30.0, "30"), (-40.0, "40"), (-50.0, "50"), (MIN_DB, "60")];

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
    let (rect, mut response) = ui.allocate_exact_size(vec2(54.0, height), Sense::click_and_drag());
    let travel = FADER_HEIGHT;
    let slot_x = rect.right() - 17.0;
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
    response.on_hover_text("Slepen: niveau, shift: fijn, dubbelklik: 0 dB")
}

/// A vertical peak meter on Logic's scale (0 dBFS at the top, 60 below at the bottom), green
/// below -6 dB, amber up to 0 and red when it clips. With `scale`, the numbers are printed on
/// its left.
fn meter(ui: &mut Ui, db: f32, scale: bool) {
    let width = if scale { 26.0 } else { 9.0 };
    let (rect, _) = ui.allocate_exact_size(vec2(width, FADER_HEIGHT + 2.0 * FADER_MARGIN), Sense::hover());
    let bar = egui::Rect::from_min_max(
        egui::pos2(rect.right() - 9.0, rect.top() + FADER_MARGIN),
        egui::pos2(rect.right(), rect.bottom() - FADER_MARGIN),
    );
    let painter = ui.painter();
    if scale {
        let font = egui::FontId::proportional(8.5);
        for (value, label) in METER_MARKS {
            let y = bar.bottom() - scale_position(&METER_TAPER, value) * bar.height();
            painter.line_segment([egui::pos2(bar.left() - 4.0, y), egui::pos2(bar.left() - 1.0, y)], Stroke::new(1.0, palette::TEXT_DIM));
            painter.text(egui::pos2(bar.left() - 5.0, y), egui::Align2::RIGHT_CENTER, label, font.clone(), palette::TEXT_DIM);
        }
    }
    painter.rect_filled(bar, 2.0, Color32::BLACK);
    let level = if db > MIN_DB { scale_position(&METER_TAPER, db) } else { 0.0 };
    let lit = egui::Rect::from_min_max(
        egui::pos2(bar.left() + 1.0, bar.bottom() - level * bar.height()),
        egui::pos2(bar.right() - 1.0, bar.bottom()),
    );
    let colour = if db > 0.0 {
        palette::LED
    } else if db > -6.0 {
        palette::ORANGE
    } else {
        palette::GREEN
    };
    painter.rect_filled(lit, 1.0, colour);
}

fn apply_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = palette::PANEL;
    visuals.window_fill = palette::PANEL;
    visuals.extreme_bg_color = palette::PANEL_DARK;
    visuals.override_text_color = Some(palette::TEXT);
    ctx.set_visuals(visuals);
}

impl DawApp {
    /// Draw one frame. Called by the platform's `eframe::App::ui`.
    pub fn ui(&mut self, ui: &mut Ui) {
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        self.now = ui.input(|i| i.time);
        self.watch_output();
        self.poll_loads();
        self.poll_takes();
        self.poll_composed();
        self.poll_export();
        self.sync_transport();
        self.update_meters(dt);
        self.handle_file_shortcuts(ui);
        if self.pending.is_none() {
            self.handle_keys(ui);
        }
        self.autosave();
        self.handle_close(ui);
        self.update_title(ui);

        egui::Panel::top("header")
            .frame(Frame::new().fill(Color32::from_rgb(0x12, 0x10, 0x11)).inner_margin(Margin::symmetric(16, 8)))
            .show(ui, |ui| self.header(ui));
        egui::CentralPanel::default().frame(Frame::new().fill(palette::BACKGROUND).inner_margin(Margin::same(16))).show(ui, |ui| {
            Frame::new().fill(palette::WOOD).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(18, 18)).show(ui, |ui| {
                Frame::new()
                    .fill(palette::PANEL)
                    .corner_radius(CornerRadius::same(3))
                    .inner_margin(Margin::same(14))
                    .stroke(Stroke::new(2.0, palette::EDGE))
                    .show(ui, |ui| {
                        egui::ScrollArea::both().show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                for index in 0..TRACKS {
                                    self.channel_strip(ui, index);
                                    ui.separator();
                                }
                                ui.add_space(8.0);
                                // The buses and the MONITOR section belong to the 388 routing.
                                if self.mixer.mode == Routing::Classic {
                                    self.pgm_masters(ui);
                                    ui.separator();
                                    self.monitor_section(ui);
                                    ui.separator();
                                }
                                self.master_section(ui);
                                ui.add_space(24.0);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new("DAW 388").strong().size(26.0).color(palette::CREAM));
                                    ui.label(RichText::new("STUDIO 8").size(13.0).color(palette::TEXT_DIM));
                                    ui.add_space(24.0);
                                    self.transport(ui);
                                });
                            });
                        });
                    });
            });
        });

        let ctx = ui.ctx().clone();
        self.settings_window(&ctx);
        self.tape_window(&ctx);
        self.export_window(&ctx);
        self.unsaved_dialog(&ctx);
        self.confirm_simple_dialog(&ctx);
        if self.may_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
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
                devices: std::rc::Rc::new(std::cell::RefCell::new(vec!["Ingebouwd".into()])),
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
                Some(name) => return Err(format!("{name} niet gevonden")),
                None => devices.first().cloned().ok_or("geen uitvoerapparaat gevonden")?,
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
            std::env::temp_dir().join("daw388-test-opnames")
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
        assert!(app.status.contains("kies een ingang"), "{}", app.status);
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
        assert_eq!(app.output.as_ref().map(|o| o.device.as_str()), Some("Ingebouwd"));
        assert!(app.status.contains("weggevallen") && app.status.contains("nu via Ingebouwd"), "{}", app.status);
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
        assert!(app.status.contains("zonder geluid"), "{}", app.status);
        devices.borrow_mut().push("Ingebouwd".into());
        app.now += DEVICE_CHECK_EVERY + 0.1;
        app.watch_output();
        assert_eq!(app.output.as_ref().map(|o| o.device.as_str()), Some("Ingebouwd"));
    }

    fn write_wav(path: &Path) {
        let spec = hound::WavSpec { channels: 1, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut writer = hound::WavWriter::create(path, spec).unwrap_or_else(|e| panic!("{e}"));
        for n in 0..441 {
            writer.write_sample((n * 10) as i16).unwrap_or_else(|e| panic!("{e}"));
        }
        writer.finalize().unwrap_or_else(|e| panic!("{e}"));
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
        assert!(app.status.contains("Sla het project eerst op"), "{}", app.status);
        app.load_paths(vec![kick]);
        app.poll_loads();
        assert!(app.save(Backup::Always));
        app.export = ExportChoices { stems: true, stem_kind: StemKind::Dry, ..ExportChoices::default() };
        app.start_export();
        app.poll_export();
        assert!(app.status.contains("2 bestand(en)"), "{}", app.status);
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
        app.poll_loads();
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
        assert!(app.saved_label.as_deref().is_some_and(|l| l.starts_with("Opgeslagen")));
        assert_eq!(app.project_name(), "Demo");
        assert!(app.tracks[0].source.as_ref().is_some_and(|s| s.starts_with(&dir)), "audio now lives in the project");

        app.new_project();
        assert!(app.tracks.iter().all(|t| t.clip.is_none()));
        app.open_arguments(vec![dir.clone()]);
        app.poll_loads();
        assert_eq!(app.project_name(), "Demo");
        assert!(!app.dirty);
        assert!(app.tracks[0].clip.is_some() && app.tracks[1].clip.is_some());
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
        assert!(app.tracks[2].clip.is_none());
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
        app.poll_loads();
        assert!(app.save(Backup::Always));
        std::fs::remove_file(dir.join("audio/kick.wav")).unwrap_or_else(|e| panic!("{e}"));

        app.open_project(&dir);
        app.poll_loads();
        assert!(app.tracks[0].missing);
        assert!(app.status.contains("ontbreekt op spoor 1"), "{}", app.status);
        // Saving again keeps the reference, so putting the file back repairs the project.
        app.mark_dirty();
        assert!(app.save(Backup::Always));
        let opened = session::open(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(opened.file.tracks[0].file.as_deref(), Some("audio/kick.wav"));
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
        app.poll_loads();
        assert!(app.tracks[0].clip.is_none(), "the old load must not come back after Wis");
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

    #[test]
    fn a_take_is_laid_over_the_track_saved_in_the_project_and_can_be_undone() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let kick = files.path().join("01_Kick.wav");
        write_wav(&kick);
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("Demo");
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(Some(dir.clone()))));
        app.load_paths(vec![kick]);
        app.poll_loads();
        assert!(app.save(Backup::Always));
        let before = app.tracks[0].source.clone();
        let length = app.tracks[0].audio.as_ref().map_or(0, |(a, _)| a.samples.len());

        let take = files.path().join("take.wav");
        write_take(&take, 0.5, 100);
        let _ = app.takes_tx.send(TakeDone { track: 0, start: 10.0, path: take.clone(), frames: 100, rate: 48_000 });
        app.poll_takes();
        app.poll_composed();
        let state = &app.tracks[0];
        let source = state.source.clone().unwrap_or_default();
        assert!(source.starts_with(dir.join("audio")), "{}", source.display());
        assert_eq!(source.file_name().and_then(|n| n.to_str()), Some("01_Kick_opname1.wav"));
        assert!(source.exists() && !take.exists());
        let samples = &state.audio.as_ref().unwrap_or_else(|| panic!("no audio")).0.samples;
        assert_eq!(samples.len(), length);
        assert!((samples[10] - 0.5).abs() < 1e-5 && (samples[109] - 0.5).abs() < 1e-5);
        assert!((samples[110] - 0.5).abs() > 1e-3, "after the punch-out the old take plays");
        assert!(app.dirty);

        app.undo_recording();
        assert_eq!(app.tracks[0].source, before);
        assert!(app.undo.is_empty());
        assert!(source.exists(), "undo keeps the take's file in audio/");
    }

    #[test]
    fn takes_on_one_track_are_laid_down_in_order() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(Some(root.path().join("Demo")))));
        assert!(app.save(Backup::Always));
        for (n, value) in [(1, 0.25f32), (2, 0.75)] {
            let take = files.path().join(format!("take{n}.wav"));
            write_take(&take, value, 50);
            let _ = app.takes_tx.send(TakeDone { track: 2, start: f64::from(n) * 100.0, path: take, frames: 50, rate: 48_000 });
        }
        app.poll_takes();
        app.poll_composed();
        app.poll_composed();
        let samples = &app.tracks[2].audio.as_ref().unwrap_or_else(|| panic!("no audio")).0.samples;
        assert_eq!(samples.len(), 250);
        assert!((samples[120] - 0.25).abs() < 1e-5, "the first take stays");
        assert!((samples[220] - 0.75).abs() < 1e-5, "the second is laid over it");
        assert_eq!(app.undo.len(), 2);
    }

    #[test]
    fn rec_explains_what_is_missing() {
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform::new(None)));
        app.record();
        assert!(app.status.contains("Sla het project eerst op"), "{}", app.status);
        app.set_routing(Routing::Classic);
        app.tracks[3].channel = ChannelEntry::default();
        app.toggle_arm(3);
        assert!(app.tracks[3].armed);
        assert!(app.status.contains("krijgt niets binnen"), "RMX without ASSIGN: {}", app.status);
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
        assert!(app.tracks[0].clip.is_none());
        assert_eq!(app.project_name(), "Naamloos");
        assert!(app.status.contains("Demo gesloten"), "{}", app.status);
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
