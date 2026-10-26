//! The egui interface of DAW 388. Platform-specific parts (audio devices, file dialogs) come in
//! through the [`Platform`] trait, so the same UI can later run in the browser.

use daw388_engine::{Command, EngineHandle, TRACKS, TrackAudio};
use daw388_project::MonoClip;
use egui::{Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, Ui, vec2};
use std::path::PathBuf;
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

    fn load_into(&mut self, index: usize, path: &std::path::Path) {
        match daw388_project::load_wav(path) {
            Ok(clip) => {
                if let Some(engine) = &mut self.engine {
                    send_track(engine, index, Some(&clip));
                }
                self.status = format!("Spoor {}: {} geladen ({:.1} s)", index + 1, clip.name, clip.duration_seconds());
                self.tracks[index].clip = Some(clip);
            }
            Err(e) => self.status = format!("{}: {e}", path.display()),
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
            self.load_into(index, path);
        }
        if paths.len() > TRACKS {
            self.status = format!("{} bestanden gekozen, alleen de eerste {TRACKS} zijn geladen", paths.len());
        }
    }

    fn clear_track(&mut self, index: usize) {
        self.tracks[index].clip = None;
        if let Some(engine) = &mut self.engine {
            engine.send(Command::SetTrack(index, None));
        }
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
            ui.separator();
            if ui.button("8 WAV-bestanden laden").clicked() {
                self.load_all();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let colour = if self.engine.is_some() { palette::GREEN } else { palette::LED };
                ui.label(RichText::new(&self.status).color(colour).size(13.0));
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
            let name = track.clip.as_ref().map_or("leeg".to_string(), |c| c.name.clone());
            ui.add(egui::Label::new(RichText::new(name).size(11.0).color(palette::TEXT_DIM)).truncate());
            ui.horizontal(|ui| {
                if ui.small_button("Laad").on_hover_text("WAV-bestand op dit spoor laden").clicked() {
                    load = true;
                }
                if track.clip.is_some() && ui.small_button("Wis").clicked() {
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
        for command in commands {
            self.engine_send(command);
        }
        if load && let Some(path) = self.platform.pick_audio_files(false).first() {
            self.load_into(index, path);
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
        self.update_meters(dt);
        self.handle_keys(ui);

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

        ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
