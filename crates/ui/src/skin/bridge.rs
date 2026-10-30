//! The meter bridge with the VU meters and the name plate, and the tape deck.

use super::fonts::bold;
use super::{reels, vu};
use crate::{DawApp, MeterSwitch, TRACKS, palette};
use egui::{Frame, Margin, RichText, Ui};

/// The width of the name plate's box at the right end of the bridge.
const PLATE_WIDTH: f32 = 140.0;

impl DawApp {
    /// TRK 1-8, the two big meters and the name plate, along the top of the console. Returns
    /// the width the bridge needs for all of it, whatever the width of the console.
    pub(crate) fn meter_bridge(&mut self, ui: &mut Ui) -> f32 {
        let now = self.now;
        let classic = self.mixer.mode == crate::Routing::Classic;
        // In the MIXER the bridge spans the console it stands on (as wide as last frame's
        // console), with the name plate at its right end.
        let span = if self.editor.settings.open { 0.0 } else { (self.needed.x - 100.0 - 24.0).max(0.0) };
        let mut needs = 0.0;
        Frame::new().fill(palette::PANEL_DARK).inner_margin(Margin::symmetric(12, 10)).corner_radius(3).show(ui, |ui| {
            ui.set_min_width(span);
            let right = ui.max_rect().left() + span;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                for index in 0..TRACKS {
                    // Each TRK meter above its own strip, as on the machine.
                    let gap = self.strip_centres[index] - vu::SMALL.x / 2.0 - ui.cursor().min.x;
                    if gap > 0.0 {
                        ui.add_space(gap);
                    }
                    let track = &self.tracks[index];
                    ui.vertical(|ui| {
                        ui.set_width(vu::SMALL.x);
                        let tip = if classic {
                            "TRK/PGM: what this track's MONITOR hears, the bus to the track or the tape. PEAK lights near full scale"
                        } else {
                            "The channel, before or after the fader (TRACK METERS). PEAK lights near full scale"
                        };
                        vu::draw(ui, vu::SMALL, track.vu.position(), now < track.peak_until).on_hover_text(tip);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            ui.add_space(vu::SMALL.x / 2.0 - 18.0);
                            ui.label(RichText::new("TRK").size(10.0).color(palette::TEXT_DIM));
                            ui.label(bold(format!("{}", index + 1)).size(14.0).color(palette::CREAM));
                        });
                    });
                }
                ui.add_space(8.0);
                let aux_eff = self.mixer.meter == MeterSwitch::AuxEff;
                for (side, (stereo, send)) in [("STEREO L", "AUX"), ("STEREO R", "EFFECT")].into_iter().enumerate() {
                    ui.vertical(|ui| {
                        ui.set_width(vu::LARGE.x);
                        vu::draw(ui, vu::LARGE, self.big_vu[side].position(), now < self.big_ol_until[side])
                            .on_hover_text("The METER switch chooses STEREO or AUX and EFF");
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            let lit = |on: bool| if on { palette::CREAM } else { palette::TEXT_DIM.gamma_multiply(0.6) };
                            ui.add_space(14.0);
                            ui.label(RichText::new(stereo).size(10.0).color(lit(!aux_eff)));
                            ui.label(RichText::new("/").size(10.0).color(palette::TEXT_DIM));
                            ui.label(RichText::new(send).size(10.0).color(lit(aux_eff)));
                        });
                    });
                }
                ui.add_space(16.0);
                // The meters and the plate side by side, plus the frame's margins.
                needs = ui.cursor().min.x - ui.max_rect().left() + PLATE_WIDTH + 24.0;
                // The name plate in a box of fixed width against the right end of the bridge.
                let gap = right - PLATE_WIDTH - ui.cursor().min.x;
                if gap > 0.0 {
                    ui.add_space(gap);
                }
                self.name_plate(ui);
            });
        });
        needs
    }

    /// "DAW 388 / STUDIO 8": an own name plate, no maker's logo.
    fn name_plate(&self, ui: &mut Ui) {
        let layout = egui::Layout::top_down(egui::Align::Max);
        ui.allocate_ui_with_layout(egui::vec2(PLATE_WIDTH, 0.0), layout, |ui| {
            ui.set_width(PLATE_WIDTH);
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(8.0);
            ui.label(bold("DAW 388").italics().size(30.0).color(palette::RED));
            ui.label(bold("STUDIO 8").italics().size(15.0).color(palette::CREAM));
            ui.label(RichText::new("tape emulation · 8 tracks").size(11.0).color(palette::TEXT_DIM));
        });
    }

    /// The tape deck: the reels behind smoked glass.
    pub(crate) fn tape_deck(&self, ui: &mut Ui) {
        reels::draw(ui, &self.reels, &reels::tape_line(&self.tape));
    }
}
