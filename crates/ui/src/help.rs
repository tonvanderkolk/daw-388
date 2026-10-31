//! The Help menu: About DAW 388 and a short guide, both on paper and without the internet.

use super::*;
use skin::paper::{self, note, section_title};

/// Where Support DAW 388... goes: a page to buy the maker a coffee.
pub(crate) const SUPPORT_URL: &str = "https://buymeacoffee.com/tvdk71";

/// Which help sheet is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Help {
    About,
    Guide,
}

/// The guide: a title and its lines, short enough to read in one go.
const GUIDE: [(&str, &[&str]); 5] = [
    (
        "Getting started",
        &[
            "File > New starts an empty project with 8 tracks. Save it first: recordings go into the project.",
            "Tab switches between the MIXER and the TRACKS view.",
            "Settings (Cmd+,) chooses the audio output, the input and the buffer.",
            "Everything in detail: Help > User manual (PDF). Help > Check for Updates... looks for a new version.",
            "View > Compact console leaves out EQ, AUX and the effects, for a low screen. They keep working.",
        ],
    ),
    (
        "Recording",
        &[
            "Choose the input of a channel in its red menu and set the level with TRIM: OL should only flash.",
            "Press REC on the channel, then REC in the transport (or R): the tape starts and records.",
            "The TRK meter above the channel shows what reaches the track.",
        ],
    ),
    (
        "Simple and 388",
        &[
            "SIMPLE: one strip per track, as in most DAWs. Each channel records on its own track and plays on L-R.",
            "388: the buses, ASSIGN, DIRECT and the MONITOR section, as on the real machine. A channel on DIRECT records \
             on its own track; without DIRECT the track records its PGM bus (ASSIGN and PAN decide what goes there).",
            "You hear a channel through L-R. The MONITOR section reaches STEREO only when no channel is on L-R: a channel off L-R is then not heard. MUTE, fader and EQ of a channel work on L-R; the MONITOR rows have their own GAIN, PAN and M.",
        ],
    ),
    (
        "Tracks",
        &[
            "Drop audio files on a lane, or LOAD on a channel. A stereo file goes on two tracks, left and right.",
            "Drag a region to move it; Alt and drag copies it. Cmd while dragging ignores SNAP.",
            "S splits at the playhead, Delete removes, Cmd+X, Cmd+C and Cmd+V cut, copy and paste.",
            "Cmd+Left and Cmd+Right zoom out and in; Cmd and scroll zooms around the mouse.",
            "M, S and R beside a track's name mute it, solo it and make it ready to record.",
        ],
    ),
    (
        "Keys",
        &[
            "Space: play or stop · R: record · Home: return to zero · , and .: rewind and fast forward",
            "C: REPEAT · K: click · Cmd+Z and Cmd+Shift+Z: undo and redo · Ctrl+Cmd+F: full screen",
        ],
    ),
];

impl DawApp {
    pub(crate) fn help_menu(&mut self, ui: &mut Ui) {
        ui.menu_button("Help", |ui| {
            if ui.button("Quick guide").clicked() {
                self.help = Some(Help::Guide);
            }
            if ui.button("User manual (PDF)").clicked() {
                match self.platform.manual() {
                    Some(path) => self.platform.reveal(&path),
                    None => self.status = "The manual comes with the downloaded app: DAW 388 Manual.pdf".to_string(),
                }
            }
            ui.separator();
            self.update_menu_items(ui);
            ui.separator();
            if ui.button("Support DAW 388...").on_hover_text(SUPPORT_URL).clicked() {
                self.platform.open_url(SUPPORT_URL);
            }
            if ui.button("About DAW 388").clicked() {
                self.help = Some(Help::About);
            }
        });
    }

    pub(crate) fn help_window(&mut self, ctx: &egui::Context) {
        let Some(help) = self.help else { return };
        let mut open = true;
        let mut support = false;
        match help {
            Help::About => paper::paper_window(ctx, "about-window", "About DAW 388", "", &mut open, 360.0, |ui| {
                ui.label(RichText::new("DAW 388").size(22.0).strong());
                ui.label(format!("8-track tape emulation · version {}", env!("CARGO_PKG_VERSION")));
                ui.add_space(12.0);
                ui.label("Made by Ton van der Kolk.");
                ui.add_space(4.0);
                ui.label("Free and open source, under the MIT or Apache-2.0 licence.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Like DAW 388?");
                    support = ui.link("You can support it with a coffee.").on_hover_text(SUPPORT_URL).clicked();
                });
                ui.add_space(12.0);
                note(ui, "After the TASCAM 388 Studio 8. Not made by or connected with TASCAM.");
            }),
            Help::Guide => paper::paper_window(ctx, "guide-window", "Quick guide", "", &mut open, 520.0, |ui| {
                egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.7).show(ui, |ui| {
                    for (title, lines) in GUIDE {
                        section_title(ui, title);
                        for line in lines {
                            ui.label(*line);
                            ui.add_space(4.0);
                        }
                        ui.add_space(6.0);
                    }
                });
            }),
        }
        if support {
            self.platform.open_url(SUPPORT_URL);
        }
        if !open {
            self.help = None;
        }
    }
}
