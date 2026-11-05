//! The mixdown deck: a second, two-track machine after the STEREO MASTER. MIX REC records the
//! mix live while the tape plays; each stretch from PLAY to STOP becomes a mix in the project's
//! `mixes/`, which plays back, is saved as a file or printed through another machine.

use crate::export::peak_of;
use crate::recording::MixDone;
use crate::{DawApp, export_peak_note, palette};
use daw388_engine::{Command, Machine, MixdownDeck, TrackAudio};
use daw388_project::export::{resample_hq, unused_path, write_audio};
use daw388_project::session::{MIXES_DIR, MachineEntry, MixEntry};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Receiver;

/// A mix shorter than this is an accidental PLAY and STOP: it is not kept.
pub(crate) const SHORTEST_MIX: f64 = 0.5;

pub(crate) fn machine_of(entry: MachineEntry) -> Machine {
    match entry {
        MachineEntry::Direct => Machine::Direct,
        MachineEntry::Quarter15 => Machine::Quarter15,
        MachineEntry::Cassette => Machine::Cassette,
    }
}

pub(crate) fn entry_of(machine: Machine) -> MachineEntry {
    match machine {
        Machine::Direct => MachineEntry::Direct,
        Machine::Quarter15 => MachineEntry::Quarter15,
        Machine::Cassette => MachineEntry::Cassette,
    }
}

/// One line on what a machine does to the mix.
pub(crate) fn machine_tip(machine: Machine) -> &'static str {
    match machine {
        Machine::Quarter15 => "A 1/4\" two-track at 15 IPS: warm and fairly transparent, a little hiss",
        Machine::Cassette => "A cassette: more hiss, fewer highs, more wow and flutter. The demo sound",
        Machine::Direct => "Straight to digital: clean, no extra tape",
    }
}

/// "3:42" for a length in seconds.
pub(crate) fn minutes(seconds: f64) -> String {
    let whole = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// The number of a mix called "Mix N", if it is.
fn number_of(name: &str) -> Option<u32> {
    name.strip_prefix("Mix ")?.trim().parse().ok()
}

/// The name of the next mix: one more than the highest "Mix N" there is (also among the file
/// names, so a renamed mix does not give its number away).
pub(crate) fn next_mix_name(mixes: &[MixEntry]) -> String {
    let file_number = |m: &MixEntry| Path::new(&m.file).file_stem().and_then(|s| number_of(&s.to_string_lossy()));
    let highest = mixes.iter().flat_map(|m| [number_of(&m.name), file_number(m)]).flatten().max().unwrap_or(0);
    format!("Mix {}", highest + 1)
}

/// Move a file, also across volumes (the temporary folder may be on another disk).
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)
}

/// A stereo file as two sides at `rate`.
pub(crate) fn load_sides(path: &Path, rate: u32) -> Result<[Vec<f32>; 2], String> {
    let (clip, sides) = daw388_project::load_audio_sides(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let [left, right] = sides.unwrap_or_else(|| [clip.samples.clone(), clip.samples.clone()]);
    if clip.sample_rate == rate {
        return Ok([left, right]);
    }
    Ok([resample_hq(&left, clip.sample_rate, rate), resample_hq(&right, clip.sample_rate, rate)])
}

/// A mix being printed through another machine.
pub(crate) struct Printing {
    pub progress: Arc<AtomicU32>,
    pub result: Receiver<Result<MixEntry, String>>,
}

/// Run the clean copy `clean` through `machine` into `dir`: a new deck file and a copy of the
/// clean one, so it can be printed again. Progress in per mille.
pub(crate) fn print(clean: &Path, dir: &Path, name: &str, machine: Machine, progress: &AtomicU32) -> Result<MixEntry, String> {
    let (clip, sides) = daw388_project::load_audio_sides(clean).map_err(|e| format!("{}: {e}", clean.display()))?;
    let [left, right] = sides.unwrap_or_else(|| [clip.samples.clone(), clip.samples.clone()]);
    let mut deck = MixdownDeck::new(clip.sample_rate);
    deck.set(machine);
    let (mut out_l, mut out_r) = (Vec::with_capacity(left.len()), Vec::with_capacity(right.len()));
    for (i, (&l, &r)) in left.iter().zip(&right).enumerate() {
        let (a, b) = deck.process(l, r);
        out_l.push(a);
        out_r.push(b);
        if i % 65_536 == 0 {
            progress.store((i as u64 * 1000 / left.len().max(1) as u64) as u32, Ordering::Relaxed);
        }
    }
    let mixes = dir.join(MIXES_DIR);
    std::fs::create_dir_all(&mixes).map_err(|e| format!("{}: {e}", mixes.display()))?;
    let path = unused_path(&mixes, name, "wav");
    let clean_path = unused_path(&mixes, &format!("{name} clean"), "wav");
    let write = |path: &Path, channels: &[Vec<f32>]| {
        daw388_project::export::write_audio(
            path,
            channels,
            clip.sample_rate,
            daw388_project::export::ExportFormat::Wav,
            daw388_project::export::BitDepth::Float32,
        )
        .map_err(|e| format!("{}: {e}", path.display()))
    };
    write(&path, &[out_l, out_r])?;
    write(&clean_path, &[left.clone(), right])?;
    progress.store(1000, Ordering::Relaxed);
    Ok(MixEntry {
        name: name.to_string(),
        file: relative(dir, &path),
        clean: Some(relative(dir, &clean_path)),
        machine: entry_of(machine),
        seconds: left.len() as f64 / f64::from(clip.sample_rate.max(1)),
    })
}

/// `path` relative to the project folder, with forward slashes as in `project.json`.
fn relative(dir: &Path, path: &Path) -> String {
    path.strip_prefix(dir).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

impl DawApp {
    /// Tell the engine the deck's machine, MIX REC and the clean copy.
    pub(crate) fn send_deck(&mut self) {
        let (machine, rec, clean) = (machine_of(self.deck.machine), self.mix_rec, self.deck.keep_clean);
        self.engine_send(Command::SetDeck { machine, rec, clean });
    }

    /// MIX REC on or off. Mixes go into the project folder, so it needs one.
    pub(crate) fn set_mix_rec(&mut self, on: bool) {
        if on && self.project_dir.is_none() {
            self.status = "Save the project first: the mix goes into the project folder".into();
            return;
        }
        if on {
            self.stop_mix();
        }
        self.mix_rec = on;
        self.send_deck();
    }

    pub(crate) fn set_deck_machine(&mut self, machine: Machine) {
        self.deck.machine = entry_of(machine);
        self.send_deck();
        self.mark_dirty();
    }

    pub(crate) fn set_keep_clean(&mut self, on: bool) {
        self.deck.keep_clean = on;
        self.send_deck();
        self.mark_dirty();
    }

    /// The mixes as they are, for undo.
    fn remember_mixes(&mut self, name: &str) {
        let before = self.mixes.clone();
        self.history.push_mixes(name, before);
    }

    /// A mix the writer finished: into `mixes/` and at the top of the list.
    pub(crate) fn mix_done(&mut self, done: MixDone) {
        let seconds = done.frames as f64 / f64::from(done.rate.max(1));
        let remove = |done: &MixDone| {
            for path in std::iter::once(&done.path).chain(&done.clean) {
                let _ = std::fs::remove_file(path);
            }
        };
        if seconds < SHORTEST_MIX {
            remove(&done);
            if done.frames > 0 {
                self.status = "Mix too short to keep".into();
            }
            return;
        }
        let Some(dir) = self.project_dir.clone() else {
            remove(&done);
            self.status = "Mix not kept: the project has no folder yet".into();
            return;
        };
        let mixes = dir.join(MIXES_DIR);
        if let Err(e) = std::fs::create_dir_all(&mixes) {
            remove(&done);
            self.status = format!("{}: {e}", mixes.display());
            return;
        }
        let name = next_mix_name(&self.mixes);
        let path = unused_path(&mixes, &name, "wav");
        if let Err(e) = move_file(&done.path, &path) {
            remove(&done);
            self.status = format!("Mix not kept: {e}");
            return;
        }
        let clean = done.clean.as_ref().and_then(|from| {
            let to = unused_path(&mixes, &format!("{name} clean"), "wav");
            match move_file(from, &to) {
                Ok(()) => Some(relative(&dir, &to)),
                Err(e) => {
                    log::warn!("clean copy of {name} not kept: {e}");
                    let _ = std::fs::remove_file(from);
                    None
                }
            }
        });
        let machine = self.deck.machine;
        self.remember_mixes(&format!("Record {name}"));
        self.mixes.insert(0, MixEntry { name: name.clone(), file: relative(&dir, &path), clean, machine, seconds });
        self.status = format!("{name} recorded ({}, {})", minutes(seconds), machine_of(machine).label());
        self.mark_dirty();
    }

    pub(crate) fn rename_mix(&mut self, index: usize, name: &str) {
        let name = name.trim();
        if let Some(mix) = self.mixes.get_mut(index)
            && !name.is_empty()
            && mix.name != name
        {
            mix.name = name.to_string();
            self.mark_dirty();
        }
    }

    /// Off the list; its files go to the trash when the project is left, and undo brings it back.
    pub(crate) fn remove_mix(&mut self, index: usize) {
        let Some(mix) = self.mixes.get(index) else { return };
        let name = mix.name.clone();
        if self.playing_mix.as_deref() == Some(mix.file.as_str()) {
            self.stop_mix();
        }
        self.remember_mixes(&format!("Remove {name}"));
        self.mixes.remove(index);
        self.status = format!("{name} removed (Cmd+Z brings it back)");
        self.mark_dirty();
    }

    /// Put the mixes of an undo or redo step back.
    pub(crate) fn set_mixes(&mut self, mixes: Vec<MixEntry>) {
        if self.playing_mix.as_ref().is_some_and(|f| !mixes.iter().any(|m| &m.file == f)) {
            self.stop_mix();
        }
        self.mixes = mixes;
    }

    fn mix_path(&self, file: &str) -> Option<PathBuf> {
        self.project_dir.as_ref().map(|d| d.join(file))
    }

    /// Play a mix on the main output; the tape stops.
    pub(crate) fn play_mix(&mut self, index: usize) {
        let Some(mix) = self.mixes.get(index).cloned() else { return };
        if self.mix_rec {
            self.status = "Not while MIX REC is on".into();
            return;
        }
        let rate = self.engine_rate();
        let cached = self.mix_audio.get(&mix.file).filter(|(r, _)| *r == rate).map(|(_, pair)| pair.clone());
        let pair = match cached {
            Some(pair) => pair,
            None => {
                let Some(path) = self.mix_path(&mix.file) else { return };
                match load_sides(&path, rate) {
                    Ok([l, r]) => {
                        let pair = [
                            Arc::new(TrackAudio { name: mix.name.clone(), samples: l }),
                            Arc::new(TrackAudio { name: mix.name.clone(), samples: r }),
                        ];
                        self.mix_audio.insert(mix.file.clone(), (rate, pair.clone()));
                        pair
                    }
                    Err(e) => {
                        self.status = format!("Cannot play {}: {e}", mix.name);
                        return;
                    }
                }
            }
        };
        self.engine_send(Command::PlayMix(Some(pair)));
        self.playing_mix = Some(mix.file);
        self.mix_started = true;
    }

    pub(crate) fn stop_mix(&mut self) {
        if self.playing_mix.take().is_some() {
            self.engine_send(Command::PlayMix(None));
        }
    }

    /// Once a frame: a mix that played to its end, or that the transport stopped, is done.
    pub(crate) fn follow_mix(&mut self) {
        let playing = self.engine.as_ref().and_then(|e| e.mix_position_seconds()).is_some();
        if self.engine.is_none() {
            self.playing_mix = None;
        } else if playing {
            self.mix_started = false;
        } else if self.playing_mix.is_some() && !self.mix_started {
            self.playing_mix = None;
        }
        if let Some(printing) = &self.printing
            && let Ok(result) = printing.result.try_recv()
        {
            self.printing = None;
            match result {
                Ok(entry) => {
                    self.remember_mixes(&format!("Print {}", entry.name));
                    self.status = format!("{} printed ({})", entry.name, machine_of(entry.machine).label());
                    self.mixes.insert(0, entry);
                    self.mark_dirty();
                }
                Err(e) => self.status = e,
            }
        }
    }

    /// Save a mix as a file in the export folder, in the Export window's format and depth.
    pub(crate) fn save_mix(&mut self, index: usize) {
        let (Some(mix), Some(dir)) = (self.mixes.get(index).cloned(), self.project_dir.clone()) else { return };
        let folder = self.export_folder(&dir);
        let result = (|| {
            let path = dir.join(&mix.file);
            let (clip, sides) = daw388_project::load_audio_sides(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let [mut left, mut right] = sides.unwrap_or_else(|| [clip.samples.clone(), clip.samples.clone()]);
            let rate = self.export.rate.unwrap_or(clip.sample_rate);
            if rate != clip.sample_rate {
                (left, right) = (resample_hq(&left, clip.sample_rate, rate), resample_hq(&right, clip.sample_rate, rate));
            }
            std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
            let base = self.export.base(&self.project_name());
            let channels = vec![left, right];
            let out = unused_path(&folder, &format!("{base}_{}", mix.name), self.export.format.extension());
            write_audio(&out, &channels, rate, self.export.format, self.export.depth).map_err(|e| format!("{}: {e}", out.display()))?;
            Ok::<_, String>((out, peak_of(&channels)))
        })();
        match result {
            Ok((out, peak)) => {
                let file = out.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let float = self.export.depth == daw388_project::export::BitDepth::Float32;
                let (note, over) = export_peak_note(peak, &file, float);
                self.last_export_peak = Some((note.clone(), over));
                self.last_export_dir = Some(folder);
                self.status = format!("{} saved as {file}. {note}", mix.name);
            }
            Err(e) => self.status = e,
        }
    }

    /// Print a mix's clean copy through `machine` into a new mix, in the background.
    pub(crate) fn print_mix(&mut self, index: usize, machine: Machine) {
        let (Some(mix), Some(dir)) = (self.mixes.get(index).cloned(), self.project_dir.clone()) else { return };
        let Some(clean) = mix.clean.as_ref().map(|c| dir.join(c)) else {
            self.status = format!("{} has no clean copy to print", mix.name);
            return;
        };
        if self.printing.is_some() {
            return;
        }
        let name = format!("{} {}", mix.name, machine_of(entry_of(machine)).label());
        let progress = Arc::new(AtomicU32::new(0));
        let (tx, rx) = std::sync::mpsc::channel();
        let (shared, ctx) = (Arc::clone(&progress), self.ctx.clone());
        self.printing = Some(Printing { progress, result: rx });
        self.status = format!("Printing {} through {}...", mix.name, machine.label());
        self.platform.spawn(Box::new(move || {
            let _ = tx.send(print(&clean, &dir, &name, machine, &shared));
            ctx.request_repaint();
        }));
    }
}

/// What a row of the list asks for, applied after the list is drawn.
enum RowAction {
    Play(usize),
    Stop,
    Save(usize),
    Print(usize, Machine),
    Remove(usize),
    StartRename(usize),
    Rename(usize, String),
}

/// One small reel of the deck: flange, three arms and the hub.
fn small_reel(painter: &egui::Painter, centre: egui::Pos2, radius: f32, angle: f32) {
    use egui::Stroke;
    painter.circle_filled(centre, radius, palette::REEL_FLANGE.gamma_multiply(0.35));
    painter.circle_stroke(centre, radius, Stroke::new(1.0, palette::REEL_FLANGE.gamma_multiply(0.7)));
    painter.circle_filled(centre, radius * 0.7, palette::TAPE_PACK);
    for spoke in 0..3 {
        let a = angle + spoke as f32 * std::f32::consts::TAU / 3.0;
        let tip = centre + egui::vec2(a.sin(), -a.cos()) * radius;
        painter.line_segment([centre, tip], Stroke::new(2.0, palette::REEL_FLANGE.gamma_multiply(0.8)));
    }
    painter.circle_filled(centre, radius * 0.25, palette::REEL);
}

/// A small flat text key for the rows of the list.
fn row_key(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let text = egui::RichText::new(label).size(9.5).color(palette::TEXT);
    ui.add(egui::Button::new(text).small().fill(palette::PANEL_DARK).stroke(egui::Stroke::new(1.0, palette::EDGE)))
}

impl DawApp {
    /// The deck under the transport, on a plate filling `field`.
    pub(crate) fn deck_panel(&mut self, ui: &mut egui::Ui, field: egui::Rect) {
        crate::skin::keys::blank_plate(ui.painter(), field);
        let inner = field.shrink2(egui::vec2(18.0, 16.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)));
        self.deck_contents(&mut child);
    }

    /// View, Mixdown Deck...: the same deck in a window (the compact console has no room for it).
    pub(crate) fn deck_window(&mut self, ctx: &egui::Context) {
        if !self.show_deck {
            return;
        }
        let mut open = true;
        egui::Window::new("Mixdown Deck")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(262.0)
            .frame(egui::Frame::window(&ctx.global_style()).fill(palette::PANEL))
            .show(ctx, |ui| {
                ui.set_width(262.0);
                ui.set_min_height(300.0);
                self.deck_contents(ui);
            });
        self.show_deck = open;
    }

    /// MIXDOWN with its reels, the machine, MIX REC and CLEAN, and the list of mixes.
    fn deck_contents(&mut self, ui: &mut egui::Ui) {
        use crate::skin::keys::{self, KeyCap, KeyColour, Symbol};
        use egui::{Align2, FontId, RichText, Sense, vec2};
        let width = ui.available_width();
        let taking = self.mix_rec && self.engine.as_ref().is_some_and(|e| e.motion() == daw388_engine::Motion::Playing);
        let moving = taking || self.playing_mix.is_some();
        if moving {
            let dt = ui.input(|i| i.stable_dt).min(0.1);
            self.deck_angle = (self.deck_angle - 3.0 * dt) % std::f32::consts::TAU;
            ui.ctx().request_repaint();
        }
        let (head, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
        let painter = ui.painter();
        painter.text(head.left_center(), Align2::LEFT_CENTER, "MIXDOWN", FontId::proportional(10.0), palette::TEXT_DIM);
        for x in [head.right() - 44.0, head.right() - 13.0] {
            small_reel(painter, egui::pos2(x, head.center().y), 13.0, self.deck_angle);
        }
        ui.add_space(6.0);

        let mut machine = machine_of(self.deck.machine);
        let (mut rec, mut clean) = (false, false);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.vertical(|ui| {
                ui.label(RichText::new("MACHINE").size(9.0).color(palette::TEXT_DIM));
                ui.scope(|ui| {
                    keys::led_style(ui);
                    ui.add_enabled_ui(!taking, |ui| {
                        egui::ComboBox::from_id_salt("deck-machine")
                            .selected_text(machine.label())
                            .wrap_mode(egui::TextWrapMode::Extend)
                            .width(104.0)
                            .show_ui(ui, |ui| {
                                for option in Machine::ALL {
                                    ui.selectable_value(&mut machine, option, option.label()).on_hover_text(machine_tip(option));
                                }
                            })
                            .response
                            .on_hover_text(machine_tip(machine));
                    });
                });
            });
            rec = KeyCap::new("MIX REC", keys::SMALL)
                .colour(KeyColour::Red)
                .lamp(taking || self.mix_rec, if taking { palette::LED } else { palette::LED.gamma_multiply(0.5) })
                .show(ui)
                .on_hover_text("Record the STEREO MASTER onto the mixdown deck while the tape plays; you hear the deck meanwhile")
                .clicked();
            ui.add_enabled_ui(!taking, |ui| {
                clean = KeyCap::new("CLEAN", keys::SMALL)
                    .lamp(self.deck.keep_clean, palette::LED_GREEN)
                    .show(ui)
                    .on_hover_text(
                        "Keep a clean copy next to each mix (twice the space), so it can be printed through another machine later",
                    )
                    .clicked();
            });
        });
        if machine != machine_of(self.deck.machine) {
            self.set_deck_machine(machine);
        }
        if rec {
            self.set_mix_rec(!self.mix_rec);
        }
        if clean {
            self.set_keep_clean(!self.deck.keep_clean);
        }
        ui.add_space(8.0);
        let line = ui.cursor().top();
        ui.painter().line_segment(
            [egui::pos2(ui.max_rect().left(), line), egui::pos2(ui.max_rect().left() + width, line)],
            egui::Stroke::new(1.0, palette::EDGE),
        );
        ui.add_space(6.0);

        if let Some(printing) = &self.printing {
            let done = printing.progress.load(Ordering::Relaxed) as f32 / 1000.0;
            ui.add(egui::ProgressBar::new(done).desired_width(width).text("Printing..."));
            ui.ctx().request_repaint();
        }
        if self.mixes.is_empty() {
            ui.label(RichText::new("No mixes yet: press MIX REC, then PLAY").size(10.5).color(palette::TEXT_DIM));
            return;
        }
        let mut actions = Vec::new();
        let mixes = self.mixes.clone();
        let renaming = self.mix_rename.clone();
        // Within the plate the list scrolls once it is longer; the window keeps it this high.
        let height = ui.available_height().clamp(80.0, 360.0);
        egui::ScrollArea::vertical().id_salt("deck-mixes").max_height(height).auto_shrink([false, true]).show(ui, |ui| {
            for (index, mix) in mixes.iter().enumerate() {
                let playing = self.playing_mix.as_deref() == Some(mix.file.as_str());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let symbol = if playing { Symbol::Stop } else { Symbol::Play };
                    let tip = if playing { "Stop this mix" } else { "Play this mix (the tape stops)" };
                    if ui
                        .add_enabled_ui(!self.mix_rec, |ui| KeyCap::new("", keys::NARROW).symbol(symbol).bare().show(ui))
                        .inner
                        .on_hover_text(tip)
                        .clicked()
                    {
                        actions.push(if playing { RowAction::Stop } else { RowAction::Play(index) });
                    }
                    ui.vertical(|ui| {
                        match &renaming {
                            Some((at, text)) if *at == index => {
                                let mut text = text.clone();
                                let edit = ui.add(egui::TextEdit::singleline(&mut text).desired_width(120.0));
                                if edit.lost_focus() {
                                    actions.push(RowAction::Rename(index, text));
                                } else {
                                    edit.request_focus();
                                    self.mix_rename = Some((index, text));
                                }
                            }
                            _ => {
                                let name = ui
                                    .add(egui::Label::new(RichText::new(&mix.name).size(11.0).color(palette::TEXT)).sense(Sense::click()));
                                if name.on_hover_text("Double-click to rename").double_clicked() {
                                    actions.push(RowAction::StartRename(index));
                                }
                            }
                        }
                        let detail = format!("{} · {}", minutes(mix.seconds), machine_of(mix.machine).label());
                        ui.label(RichText::new(detail).size(9.0).color(palette::TEXT_DIM));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if row_key(ui, "×").on_hover_text("Take this mix off the list (Cmd+Z brings it back)").clicked() {
                            actions.push(RowAction::Remove(index));
                        }
                        ui.add_enabled_ui(mix.clean.is_some() && self.printing.is_none(), |ui| {
                            let tip = if mix.clean.is_some() {
                                "Print the clean copy through another machine: a new mix"
                            } else {
                                "Only a mix with a clean copy (CLEAN) can be printed again"
                            };
                            ui.menu_button(RichText::new("PRINT").size(9.5).color(palette::TEXT), |ui| {
                                for option in Machine::ALL.into_iter().filter(|m| *m != machine_of(mix.machine)) {
                                    if ui.button(option.label()).on_hover_text(machine_tip(option)).clicked() {
                                        actions.push(RowAction::Print(index, option));
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text(tip);
                        });
                        if row_key(ui, "SAVE").on_hover_text("Save as a file in the export folder, in the Export window's format").clicked()
                        {
                            actions.push(RowAction::Save(index));
                        }
                    });
                });
                ui.add_space(4.0);
            }
        });
        for action in actions {
            match action {
                RowAction::Play(index) => self.play_mix(index),
                RowAction::Stop => self.stop_mix(),
                RowAction::Save(index) => self.save_mix(index),
                RowAction::Print(index, machine) => self.print_mix(index, machine),
                RowAction::Remove(index) => self.remove_mix(index),
                RowAction::StartRename(index) => self.mix_rename = self.mixes.get(index).map(|m| (index, m.name.clone())),
                RowAction::Rename(index, name) => {
                    self.mix_rename = None;
                    self.rename_mix(index, &name);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, file: &str) -> MixEntry {
        MixEntry { name: name.into(), file: file.into(), clean: None, machine: MachineEntry::Direct, seconds: 1.0 }
    }

    #[test]
    fn mixes_are_numbered_on_from_the_highest() {
        assert_eq!(next_mix_name(&[]), "Mix 1");
        let mixes = [entry("Mix 2", "mixes/Mix 2.wav"), entry("Final", "mixes/Mix 5.wav"), entry("Mix 3", "mixes/Mix 3.wav")];
        assert_eq!(next_mix_name(&mixes), "Mix 6", "a renamed mix keeps its number through its file");
    }

    #[test]
    fn minutes_and_seconds() {
        assert_eq!(minutes(222.4), "3:42");
        assert_eq!(minutes(5.0), "0:05");
    }

    #[test]
    fn printing_makes_a_new_mix_with_its_clean_copy() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let clean = dir.path().join("clean.wav");
        let tone: Vec<f32> = (0..48_000).map(|i| 0.3 * (i as f32 * 0.05).sin()).collect();
        write_audio(
            &clean,
            &[tone.clone(), tone.clone()],
            48_000,
            daw388_project::export::ExportFormat::Wav,
            daw388_project::export::BitDepth::Float32,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let progress = AtomicU32::new(0);
        let entry = print(&clean, dir.path(), "Mix 1 CASSETTE", Machine::Cassette, &progress).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(entry.machine, MachineEntry::Cassette);
        assert_eq!(entry.file, "mixes/Mix 1 CASSETTE.wav");
        assert_eq!(entry.clean.as_deref(), Some("mixes/Mix 1 CASSETTE clean.wav"));
        assert!((entry.seconds - 1.0).abs() < 1e-9);
        assert_eq!(progress.load(Ordering::Relaxed), 1000);
        let [l, _] = load_sides(&dir.path().join(&entry.file), 48_000).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(l.len(), 48_000);
        assert!(l.iter().zip(&tone).any(|(a, b)| (a - b).abs() > 1e-3), "the cassette changes it");
        let [c, _] = load_sides(&dir.path().join(entry.clean.as_deref().unwrap_or_default()), 48_000).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(c, tone, "the clean copy stays clean");
    }
}
