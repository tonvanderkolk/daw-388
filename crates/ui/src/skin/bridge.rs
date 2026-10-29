//! The meter bridge with the VU meters and the name plate, and the tape deck.

use super::fonts::bold;
use super::{reels, vu};
use crate::{DawApp, MeterSwitch, TRACKS, palette};
use egui::{Frame, Margin, RichText, Ui};

impl DawApp {
    /// TRK 1-8, the two big meters and the name plate, along the top of the console.
    pub(crate) fn meter_bridge(&mut self, ui: &mut Ui) {
        let now = self.now;
        Frame::new().fill(palette::PANEL_DARK).inner_margin(Margin::symmetric(12, 10)).corner_radius(3).show(ui, |ui| {
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
                        vu::draw(ui, vu::SMALL, track.vu.position(), now < track.ol_until).on_hover_text("OL: piek na de EQ boven -3 dBFS");
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
                            .on_hover_text("De METER-schakelaar kiest STEREO of AUX en EFF");
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
                self.name_plate(ui);
            });
        });
    }

    /// "DAW 388 / STUDIO 8": an own name plate, no maker's logo.
    fn name_plate(&self, ui: &mut Ui) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(8.0);
            ui.label(bold("DAW 388").italics().size(30.0).color(palette::RED));
            ui.label(bold("STUDIO 8").italics().size(15.0).color(palette::CREAM));
            ui.label(RichText::new("tape-emulatie · 8 sporen").size(11.0).color(palette::TEXT_DIM));
        });
    }

    /// The tape deck: the reels behind smoked glass.
    pub(crate) fn tape_deck(&self, ui: &mut Ui) {
        reels::draw(ui, &self.reels, &reels::tape_line(&self.tape));
    }
}
