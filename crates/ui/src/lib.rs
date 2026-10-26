//! The egui interface of DAW 388. Platform-specific parts (audio devices, file dialogs) come in
//! through the [`Platform`] trait, so the same UI can later run in the browser.

use daw388_engine::{Command, EngineHandle, TRACKS, TrackAudio};
use daw388_project::session::{self, PROJECT_FILE};
use daw388_project::{Backup, MonoClip, TrackToSave};
use egui::{Align, Color32, CornerRadius, Frame, Key, KeyboardShortcut, Layout, Margin, Modifiers, RichText, Sense, Stroke, Ui, vec2};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What the UI needs from the platform it runs on.
pub trait Platform {
    /// Names of the available audio output devices.
    fn output_devices(&mut self) -> Vec<String>;
    /// Name of the default output device, if there is one.
    fn default_output_device(&mut self) -> Option<String>;
    /// Open an output device (`None`: the default one) and return the engine that now feeds it,
    /// plus the name of the device that was actually opened.
    fn open_output(&mut self, device: Option<&str>) -> Result<(EngineHandle, String), String>;
    /// Ask the user for one or more audio files.
    fn pick_audio_files(&mut self, multiple: bool) -> Vec<PathBuf>;
    /// Ask the user for a project to open (its folder or its `project.json`).
    fn pick_project_to_open(&mut self) -> Option<PathBuf>;
    /// Ask the user where to save a project. Returns the project folder to create or reuse.
    fn pick_project_location(&mut self, suggested_name: &str) -> Option<PathBuf>;
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
    pub const KEY: Color32 = Color32::from_rgb(0x9a, 0x9a, 0x9c);
}

const MIN_DB: f32 = -60.0;
const MAX_DB: f32 = 10.0;

fn db_to_gain(db: f32) -> f32 {
    if db <= MIN_DB { 0.0 } else { 10f32.powf(db / 20.0) }
}

fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 { MIN_DB } else { (20.0 * gain.log10()).max(MIN_DB) }
}

/// Counter text, minutes and seconds with tenths.
pub fn format_counter(seconds: f64) -> String {
    let tenths = (seconds.max(0.0) * 10.0).floor() as u64;
    format!("{:02}:{:02}.{}", tenths / 600, (tenths / 10) % 60, tenths % 10)
}

#[derive(Default)]
struct TrackState {
    clip: Option<MonoClip>,
    /// The file the clip came from; after saving, its copy in the project's `audio/` folder.
    source: Option<PathBuf>,
    /// The project refers to a file that could not be loaded.
    missing: bool,
    fader_db: f32,
    pan: f32,
    mute: bool,
    /// Displayed meter level in dB, with a slow fall.
    meter_db: f32,
}

pub struct DawApp {
    platform: Box<dyn Platform>,
    engine: Option<EngineHandle>,
    devices: Vec<String>,
    device: Option<String>,
    tracks: [TrackState; TRACKS],
    master_db: f32,
    master_meter_db: [f32; 2],
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
}

impl DawApp {
    pub fn new(ctx: &egui::Context, mut platform: Box<dyn Platform>) -> Self {
        apply_style(ctx);
        let devices = platform.output_devices();
        let mut app = Self {
            platform,
            engine: None,
            devices,
            device: None,
            tracks: Default::default(),
            master_db: 0.0,
            master_meter_db: [MIN_DB; 2],
            status: String::new(),
            project_dir: None,
            dirty: false,
            saved_label: None,
            autosave_at: None,
            now: 0.0,
            pending: None,
            may_close: false,
            title: String::new(),
        };
        for track in &mut app.tracks {
            track.meter_db = MIN_DB;
        }
        app.open_device(None);
        app
    }

    /// Open a device and bring the new engine up to date with the current tracks and mix.
    fn open_device(&mut self, name: Option<String>) {
        let position = self.engine.as_ref().map_or(0.0, EngineHandle::position_seconds);
        // Drop the old stream first: some systems allow only one open stream per device.
        self.engine = None;
        match self.platform.open_output(name.as_deref()) {
            Ok((mut engine, opened)) => {
                self.status = format!("{opened}, {} Hz", engine.sample_rate());
                for (index, track) in self.tracks.iter().enumerate() {
                    send_track(&mut engine, index, track.clip.as_ref());
                    engine.send(Command::SetGain(index, db_to_gain(track.fader_db)));
                    engine.send(Command::SetPan(index, track.pan));
                    engine.send(Command::SetMute(index, track.mute));
                }
                engine.send(Command::SetMaster(db_to_gain(self.master_db)));
                engine.locate_seconds(position);
                self.device = Some(opened);
                self.engine = Some(engine);
            }
            Err(e) => {
                self.status = format!("Geen audio-uitvoer: {e}");
                self.device = name;
            }
        }
    }

    /// Load a WAV file onto a track. Returns `false` (with a status message) when it cannot be read.
    fn load_into(&mut self, index: usize, path: &Path) -> bool {
        match daw388_project::load_wav(path) {
            Ok(clip) => {
                if let Some(engine) = &mut self.engine {
                    send_track(engine, index, Some(&clip));
                }
                self.status = format!("Spoor {}: {} geladen ({:.1} s)", index + 1, clip.name, clip.duration_seconds());
                let track = &mut self.tracks[index];
                track.clip = Some(clip);
                track.source = Some(path.to_path_buf());
                track.missing = false;
                true
            }
            Err(e) => {
                self.status = format!("{}: {e}", path.display());
                false
            }
        }
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
            if self.load_into(index, path) {
                self.mark_dirty();
            }
        }
        if paths.len() > TRACKS {
            self.status = format!("{} bestanden gekozen, alleen de eerste {TRACKS} zijn geladen", paths.len());
        }
    }

    /// Handle command-line arguments: a project folder or `project.json` opens that project,
    /// WAV files go onto the tracks.
    pub fn open_arguments(&mut self, paths: Vec<PathBuf>) {
        match paths.as_slice() {
            [] => {}
            [one] if one.is_dir() || one.file_name().is_some_and(|n| n == PROJECT_FILE) => self.open_project(one),
            _ => self.load_paths(paths),
        }
    }

    fn clear_track(&mut self, index: usize) {
        let track = &mut self.tracks[index];
        track.clip = None;
        track.source = None;
        track.missing = false;
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
            self.tracks[index] = TrackState { meter_db, ..TrackState::default() };
            if let Some(engine) = &mut self.engine {
                engine.send(Command::SetTrack(index, None));
                engine.send(Command::SetGain(index, 1.0));
                engine.send(Command::SetPan(index, 0.0));
                engine.send(Command::SetMute(index, false));
            }
        }
        self.master_db = 0.0;
        if let Some(engine) = &mut self.engine {
            engine.stop();
            engine.rtz();
            engine.send(Command::SetMaster(1.0));
        }
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
        let mut missing = Vec::new();
        for entry in &opened.file.tracks {
            let index = usize::from(entry.number.saturating_sub(1));
            if index >= TRACKS {
                continue;
            }
            let track = &mut self.tracks[index];
            track.fader_db = entry.fader_db.clamp(MIN_DB, MAX_DB);
            track.pan = entry.pan.clamp(-1.0, 1.0);
            track.mute = entry.mute;
            if let Some(engine) = &mut self.engine {
                engine.send(Command::SetGain(index, db_to_gain(track.fader_db)));
                engine.send(Command::SetPan(index, track.pan));
                engine.send(Command::SetMute(index, track.mute));
            }
            if let Some(source) = opened.track_source(entry)
                && !self.load_into(index, &source)
            {
                let track = &mut self.tracks[index];
                track.source = Some(source);
                track.missing = true;
                missing.push(index + 1);
            }
        }
        self.master_db = opened.file.master_db.clamp(MIN_DB, MAX_DB);
        self.engine_send(Command::SetMaster(db_to_gain(self.master_db)));
        self.project_dir = Some(opened.dir);
        self.dirty = false;
        self.autosave_at = None;
        self.status = if missing.is_empty() {
            format!("{} geopend", self.project_name())
        } else {
            let list: Vec<String> = missing.iter().map(ToString::to_string).collect();
            format!("{} geopend, audio ontbreekt op spoor {}", self.project_name(), list.join(", "))
        };
    }

    /// Save to the project folder, asking for one first when there is none. Returns `true` when saved.
    fn save(&mut self, backup: Backup) -> bool {
        let Some(dir) = self.project_dir.clone() else { return self.save_as() };
        let tracks: Vec<TrackToSave> =
            self.tracks.iter().map(|t| TrackToSave { source: t.source.clone(), fader_db: t.fader_db, pan: t.pan, mute: t.mute }).collect();
        match session::save(&dir, self.master_db, &tracks, backup) {
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
        let (close, quit) = ui.input_mut(|i| (i.consume_shortcut(&shortcut(Key::W)), i.consume_shortcut(&shortcut(Key::Q))));
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
            if ui.button("8 WAV-bestanden laden...").clicked() {
                self.load_all();
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

    fn play(&mut self) {
        if let Some(engine) = &mut self.engine {
            engine.play();
        }
    }

    fn stop(&mut self) {
        if let Some(engine) = &mut self.engine {
            engine.stop();
        }
    }

    fn rtz(&mut self) {
        if let Some(engine) = &mut self.engine {
            engine.rtz();
        }
    }

    fn update_meters(&mut self, dt: f32) {
        let Some(engine) = &mut self.engine else { return };
        engine.collect_garbage();
        // VU-like fall of about 20 dB per second.
        let fall = 20.0 * dt;
        for (index, track) in self.tracks.iter_mut().enumerate() {
            let peak = gain_to_db(engine.take_track_peak(index));
            track.meter_db = peak.max(track.meter_db - fall);
        }
        let master = engine.take_master_peaks();
        for (shown, peak) in self.master_meter_db.iter_mut().zip(master) {
            *shown = gain_to_db(peak).max(*shown - fall);
        }
    }

    fn handle_keys(&mut self, ui: &Ui) {
        if ui.memory(|m| m.focused().is_some()) {
            return;
        }
        let (space, home) = ui.input(|i| (i.key_pressed(egui::Key::Space), i.key_pressed(egui::Key::Home)));
        if space {
            if self.engine.as_ref().is_some_and(EngineHandle::is_playing) { self.stop() } else { self.play() }
        }
        if home {
            self.rtz();
        }
    }

    fn header(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            self.file_menu(ui);
            let name = format!("{}{}", self.project_name(), if self.dirty { " (gewijzigd)" } else { "" });
            ui.label(RichText::new(name).color(palette::TEXT_DIM).size(13.0));
            ui.separator();
            ui.label(RichText::new("Uitvoer").color(palette::TEXT_DIM));
            let selected = self.device.clone().unwrap_or_else(|| "Geen apparaat".into());
            let mut choice: Option<Option<String>> = None;
            egui::ComboBox::from_id_salt("output-device").selected_text(selected).width(260.0).show_ui(ui, |ui| {
                if ui.selectable_label(false, "Standaardapparaat").clicked() {
                    choice = Some(None);
                }
                for name in &self.devices {
                    if ui.selectable_label(self.device.as_deref() == Some(name), name).clicked() {
                        choice = Some(Some(name.clone()));
                    }
                }
            });
            if ui.button("Vernieuw").on_hover_text("Lijst met audioapparaten opnieuw ophalen").clicked() {
                self.devices = self.platform.output_devices();
            }
            if let Some(choice) = choice {
                self.open_device(choice);
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
        let mut commands = Vec::new();
        let track = &mut self.tracks[index];
        column(ui, 76.0, |ui| {
            ui.label(RichText::new(format!("{}", index + 1)).strong().size(18.0).color(palette::CREAM));
            let (name, colour) = match (&track.clip, &track.source) {
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
                if ui.small_button("Laad").on_hover_text("WAV-bestand op dit spoor laden").clicked() {
                    load = true;
                }
                if (track.clip.is_some() || track.missing) && ui.small_button("Wis").clicked() {
                    clear = true;
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("PAN").size(10.0).color(palette::TEXT_DIM));
            let pan = ui.add(egui::Slider::new(&mut track.pan, -1.0..=1.0).show_value(false));
            if pan.double_clicked() {
                track.pan = 0.0;
            }
            if pan.changed() || pan.double_clicked() {
                commands.push(Command::SetPan(index, track.pan));
            }
            let mute = ui.add(
                egui::Button::new(RichText::new("MUTE").size(11.0).color(if track.mute { Color32::BLACK } else { palette::TEXT }))
                    .fill(if track.mute { palette::ORANGE } else { palette::PANEL_DARK }),
            );
            if mute.clicked() {
                track.mute = !track.mute;
                commands.push(Command::SetMute(index, track.mute));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                meter(ui, track.meter_db, 180.0);
                let fader = fader(ui, &mut track.fader_db, palette::CREAM);
                if fader.changed() || fader.double_clicked() {
                    commands.push(Command::SetGain(index, db_to_gain(track.fader_db)));
                }
            });
            ui.label(RichText::new(format!("{:+.1} dB", track.fader_db)).size(11.0).color(palette::TEXT_DIM));
        });
        if !commands.is_empty() {
            self.mark_dirty();
        }
        for command in commands {
            self.engine_send(command);
        }
        if load
            && let Some(path) = self.platform.pick_audio_files(false).first()
            && self.load_into(index, path)
        {
            self.mark_dirty();
        }
        if clear {
            self.clear_track(index);
        }
    }

    fn master_section(&mut self, ui: &mut Ui) {
        column(ui, 90.0, |ui| {
            ui.label(RichText::new("STEREO").strong().size(14.0).color(palette::CREAM));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                meter(ui, self.master_meter_db[0], 240.0);
                meter(ui, self.master_meter_db[1], 240.0);
                let response = fader(ui, &mut self.master_db, palette::RED);
                if response.changed() || response.double_clicked() {
                    let gain = db_to_gain(self.master_db);
                    self.engine_send(Command::SetMaster(gain));
                    self.mark_dirty();
                }
            });
            ui.label(RichText::new(format!("{:+.1} dB", self.master_db)).size(11.0).color(palette::TEXT_DIM));
        });
    }

    fn transport(&mut self, ui: &mut Ui) {
        let (seconds, playing) = self.engine.as_ref().map_or((0.0, false), |e| (e.position_seconds(), e.is_playing()));
        column(ui, 220.0, |ui| {
            Frame::new()
                .fill(Color32::from_rgb(0x12, 0x08, 0x07))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(14, 6))
                .stroke(Stroke::new(2.0, palette::EDGE))
                .show(ui, |ui| {
                    ui.label(RichText::new(format_counter(seconds)).monospace().size(34.0).color(palette::LED));
                });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if transport_key(ui, "RTZ", false).on_hover_text("Terug naar nul (Home)").clicked() {
                    self.rtz();
                }
                if transport_key(ui, "STOP", !playing).on_hover_text("Stop (spatie)").clicked() {
                    self.stop();
                }
                if transport_key(ui, "PLAY", playing).on_hover_text("Afspelen (spatie)").clicked() {
                    self.play();
                }
            });
        });
    }
}

/// A top-down column of a fixed width with centred contents.
fn column(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Center), |ui| {
        ui.set_width(width);
        add(ui);
    });
}

fn send_track(engine: &mut EngineHandle, index: usize, clip: Option<&MonoClip>) {
    let audio = clip.map(|c| Arc::new(TrackAudio { name: c.name.clone(), samples: c.at_rate(engine.sample_rate()) }));
    engine.send(Command::SetTrack(index, audio));
}

fn transport_key(ui: &mut Ui, label: &str, lit: bool) -> egui::Response {
    let text = RichText::new(label).strong().size(13.0).color(if lit { palette::LED } else { Color32::from_rgb(0x2a, 0x26, 0x22) });
    ui.add(egui::Button::new(text).fill(palette::KEY).min_size(vec2(64.0, 40.0)).corner_radius(CornerRadius::same(3)))
}

/// A vertical fader in dB; double-click resets it to 0 dB.
fn fader(ui: &mut Ui, db: &mut f32, cap: Color32) -> egui::Response {
    let height = 180.0;
    let (rect, response) = ui.allocate_exact_size(vec2(34.0, height), Sense::click_and_drag());
    let to_y = |value: f32| rect.bottom() - 10.0 - (value - MIN_DB) / (MAX_DB - MIN_DB) * (height - 20.0);
    let mut response = response;
    if response.dragged() {
        let delta = -response.drag_delta().y / (height - 20.0) * (MAX_DB - MIN_DB);
        let fine = if ui.input(|i| i.modifiers.shift) { 0.2 } else { 1.0 };
        *db = (*db + delta * fine).clamp(MIN_DB, MAX_DB);
        response.mark_changed();
    }
    if response.double_clicked() {
        *db = 0.0;
    }
    let painter = ui.painter();
    let slot = egui::Rect::from_center_size(rect.center(), vec2(4.0, height - 16.0));
    painter.rect_filled(slot, 2.0, Color32::BLACK);
    let unity = to_y(0.0);
    painter
        .line_segment([egui::pos2(rect.left() + 4.0, unity), egui::pos2(rect.right() - 4.0, unity)], Stroke::new(1.0, palette::TEXT_DIM));
    let knob = egui::Rect::from_center_size(egui::pos2(rect.center().x, to_y(*db)), vec2(28.0, 16.0));
    painter.rect_filled(knob, 3.0, cap);
    painter.line_segment(
        [egui::pos2(knob.left() + 3.0, knob.center().y), egui::pos2(knob.right() - 3.0, knob.center().y)],
        Stroke::new(1.5, Color32::from_rgb(0x2a, 0x26, 0x22)),
    );
    response.on_hover_text("Slepen: niveau, shift: fijn, dubbelklik: 0 dB")
}

/// A vertical peak meter from -60 to +6 dBFS-ish, amber above -6, red above 0.
fn meter(ui: &mut Ui, db: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, Color32::BLACK);
    let span = 66.0;
    let level = ((db - MIN_DB) / span).clamp(0.0, 1.0);
    let lit = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 1.0, rect.bottom() - level * rect.height()),
        egui::pos2(rect.right() - 1.0, rect.bottom()),
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
                        egui::ScrollArea::horizontal().show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                for index in 0..TRACKS {
                                    self.channel_strip(ui, index);
                                    ui.separator();
                                }
                                ui.add_space(8.0);
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
        self.unsaved_dialog(&ctx);
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

    /// A platform without audio hardware whose dialogs give fixed answers.
    struct FakePlatform {
        save_location: Option<PathBuf>,
        /// Keeps the audio side alive so commands are accepted.
        engines: Vec<daw388_engine::Engine>,
    }

    impl Platform for FakePlatform {
        fn output_devices(&mut self) -> Vec<String> {
            vec!["Test".into()]
        }
        fn default_output_device(&mut self) -> Option<String> {
            Some("Test".into())
        }
        fn open_output(&mut self, _: Option<&str>) -> Result<(EngineHandle, String), String> {
            let (handle, engine) = daw388_engine::create(48_000);
            self.engines.push(engine);
            Ok((handle, "Test".into()))
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
    fn save_and_reopen_a_project() {
        let files = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let kick = files.path().join("01_Kick.wav");
        let bass = files.path().join("03_Bas.wav");
        write_wav(&kick);
        write_wav(&bass);
        let root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dir = root.path().join("Demo");

        let ctx = egui::Context::default();
        let platform = FakePlatform { save_location: Some(dir.clone()), engines: Vec::new() };
        let mut app = DawApp::new(&ctx, Box::new(platform));
        app.load_paths(vec![kick, bass]);
        assert!(app.dirty);
        app.tracks[1].fader_db = -6.0;
        app.tracks[1].pan = -0.5;
        app.tracks[1].mute = true;
        app.master_db = -2.0;
        assert!(app.save(Backup::Always), "{}", app.status);
        assert!(!app.dirty);
        assert!(app.saved_label.as_deref().is_some_and(|l| l.starts_with("Opgeslagen")));
        assert_eq!(app.project_name(), "Demo");
        assert!(app.tracks[0].source.as_ref().is_some_and(|s| s.starts_with(&dir)), "audio now lives in the project");

        app.new_project();
        assert!(app.tracks.iter().all(|t| t.clip.is_none()));
        app.open_arguments(vec![dir.clone()]);
        assert_eq!(app.project_name(), "Demo");
        assert!(!app.dirty);
        assert!(app.tracks[0].clip.is_some() && app.tracks[1].clip.is_some());
        assert_eq!(app.tracks[1].fader_db, -6.0);
        assert_eq!(app.tracks[1].pan, -0.5);
        assert!(app.tracks[1].mute);
        assert_eq!(app.master_db, -2.0);
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
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform { save_location: Some(dir.clone()), engines: Vec::new() }));
        app.load_paths(vec![kick]);
        assert!(app.save(Backup::Always));
        std::fs::remove_file(dir.join("audio/kick.wav")).unwrap_or_else(|e| panic!("{e}"));

        app.open_project(&dir);
        assert!(app.tracks[0].missing);
        assert!(app.status.contains("ontbreekt op spoor 1"), "{}", app.status);
        // Saving again keeps the reference, so putting the file back repairs the project.
        app.mark_dirty();
        assert!(app.save(Backup::Always));
        let opened = session::open(&dir).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(opened.file.tracks[0].file.as_deref(), Some("audio/kick.wav"));
    }

    #[test]
    fn unsaved_changes_ask_before_new() {
        let ctx = egui::Context::default();
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform { save_location: None, engines: Vec::new() }));
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
        let mut app = DawApp::new(&ctx, Box::new(FakePlatform { save_location: Some(root.path().join("Demo")), engines: Vec::new() }));
        app.load_paths(vec![kick]);
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
        assert_eq!(format_counter(-3.0), "00:00.0");
    }

    #[test]
    fn db_and_gain_round_trip() {
        assert_eq!(db_to_gain(MIN_DB), 0.0);
        assert!((db_to_gain(0.0) - 1.0).abs() < 1e-6);
        assert!((gain_to_db(db_to_gain(-12.0)) + 12.0).abs() < 1e-4);
    }
}
