//! The console's channel strips and master section, laid out after the 388.

use super::*;
use crate::skin::fonts::bold;
use egui::{Align2, FontId, Pos2, Rect, UiBuilder, Vec2, pos2};

/// Heights of the bands of a strip, in points, label included.
pub(crate) const BAND_TRACK: f32 = 58.0;
pub(crate) const BAND_INPUT: f32 = 78.0;
pub(crate) const BAND_EQ: f32 = 132.0;
pub(crate) const BAND_AUX: f32 = 84.0;
pub(crate) const BAND_ASSIGN: f32 = 104.0;
pub(crate) const BAND_PAN: f32 = 58.0;
pub(crate) const BAND_KEYS: f32 = 38.0;
/// The gap between the last band and the faders.
pub(crate) const FADER_GAP: f32 = 6.0;
/// Bands of the EFFECT RTN column.
pub(crate) const BAND_RTN: f32 = 82.0;
pub(crate) const BAND_PLATE: f32 = 66.0;
pub(crate) const BAND_ECHO: f32 = 96.0;
pub(crate) const BAND_MONITOR: f32 = 302.0;
/// Bands of the MASTER column.
pub(crate) const BAND_MASTER: f32 = 66.0;
pub(crate) const BAND_SWITCHES: f32 = 110.0;
pub(crate) const BAND_LISTEN: f32 = 100.0;
/// The height of a key with its label above it.
const KEY_SMALL_H: f32 = 33.0;
const KEY_NARROW_H: f32 = 27.0;

/// Where the faders start below the top of the strips: the bands above them, ASSIGN only in the 388 routing.
pub(crate) fn fader_top(classic: bool) -> f32 {
    let assign = if classic { BAND_ASSIGN } else { 0.0 };
    BAND_TRACK + BAND_INPUT + BAND_EQ + BAND_AUX + assign + BAND_PAN + BAND_KEYS
}

/// The bands of the EFFECT RTN column, which must fit above the faders.
pub(crate) fn return_bands(classic: bool) -> f32 {
    BAND_RTN + BAND_PLATE + BAND_ECHO + if classic { BAND_MONITOR } else { 0.0 }
}

/// Lay out `add` in a box of `size` whose top centre is at (`x`, `top`): the grid of the console.
pub(crate) fn place<R>(ui: &mut Ui, x: f32, top: f32, size: Vec2, add: impl FnOnce(&mut Ui) -> R) -> R {
    let rect = Rect::from_min_size(pos2(x - size.x / 2.0, top), size);
    ui.scope_builder(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Center)), add).inner
}

/// A small label at `pos`, painted so it moves nothing; returns where it went.
pub(crate) fn tag(ui: &Ui, pos: Pos2, align: Align2, text: &str) -> Rect {
    ui.painter().text(pos, align, text, FontId::proportional(8.0), palette::TEXT_DIM)
}

/// One band of the console: its label small at the top left, the content centred in exactly
/// `height` points (clipped if it would run over), and a thin line under it, as on the 388.
pub(crate) fn section<R>(ui: &mut Ui, label: &str, height: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let top = if label.is_empty() { 4.0 } else { 14.0 };
    let inner = Rect::from_min_max(rect.min + vec2(0.0, top), rect.max - vec2(0.0, 3.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Center)));
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    // The columns lay out their bands without gaps; inside a band the widgets get some air.
    child.spacing_mut().item_spacing.y = 4.0;
    if !label.is_empty() {
        let font = egui::FontId::proportional(9.0);
        ui.painter().text(rect.left_top() + vec2(3.0, 2.0), egui::Align2::LEFT_TOP, label, font, palette::TEXT_DIM);
    }
    let result = add(&mut child);
    let line = palette::TEXT_DIM.gamma_multiply(0.35);
    ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, line));
    result
}

impl DawApp {
    pub(crate) fn channel_strip(&mut self, ui: &mut Ui, index: usize) {
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
        // Asked before `track` borrows the tracks.
        let (loading, missing) = (self.track_loading(index), self.track_missing(index));
        // Renaming under the fader shares the state with SPOREN: one name at a time.
        let mut renaming = self.editor.renaming.take();
        let mut renamed: Option<String> = None;
        let track = &mut self.tracks[index];
        let has_content = !track.regions.is_empty() || track.unresolved.is_some();
        column(ui, 112.0, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            // Every control of a strip stands on a fixed spot: the knobs on the strip's centre
            // line, labels beside them without moving anything, as on the 388.
            // What the 388 does not have: loading, wiping and the interface input.
            section(ui, "", BAND_TRACK, |ui| {
                let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
                let load_key = place(ui, cx - 16.0, top, vec2(30.0, KEY_NARROW_H), |ui| KeyCap::new("LOAD", keys::NARROW).show(ui));
                if load_key.on_hover_text("Put a WAV, AIFF, FLAC or MP3 file on this track, from the play position").clicked() {
                    load = true;
                }
                let wipe = place(ui, cx + 16.0, top, vec2(30.0, KEY_NARROW_H), |ui| {
                    ui.add_enabled_ui(has_content, |ui| KeyCap::new("CLEAR", keys::NARROW).show(ui)).inner
                });
                if wipe.on_hover_text("Remove everything from this track (Cmd+Z brings it back)").clicked() {
                    clear = true;
                }
                if track.channel.source != ChannelSource::Rmx || track.channel.auto {
                    let text = |c: Option<u16>| c.map_or("no input".to_string(), |c| format!("in {c}"));
                    let mut input = track.input;
                    place(ui, cx, top + 31.0, vec2(84.0, 20.0), |ui| {
                        egui::ComboBox::from_id_salt(("strip-input", index)).selected_text(text(input)).width(76.0).show_ui(ui, |ui| {
                            ui.selectable_value(&mut input, None, text(None));
                            for c in 1..=input_channels {
                                ui.selectable_value(&mut input, Some(c), text(Some(c)));
                            }
                        });
                    });
                    if input != track.input {
                        track.input = input;
                        changed = true;
                    }
                }
            });
            // INPUT: MIC, LINE or RMX (the tape), the OL lamp and TRIM, as at the top of a 388
            // channel. The simple mode plays the tape by itself, so it has no RMX.
            section(ui, "INPUT", BAND_INPUT, |ui| {
                let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
                let sources: &[ChannelSource] = if simple {
                    &[ChannelSource::Mic, ChannelSource::Line]
                } else {
                    &[ChannelSource::Mic, ChannelSource::Line, ChannelSource::Rmx]
                };
                let labels: &[&str] = if simple { &["MIC", "LINE"] } else { &["MIC", "LINE", "RMX"] };
                let width = if simple { 52.0 } else { 78.0 };
                let mut chosen = sources.iter().position(|s| *s == track.channel.source).unwrap_or(1);
                let switch = place(ui, cx, top, vec2(width, 23.0), |ui| keys::slide_switch(ui, labels, &mut chosen, width)).on_hover_text(
                    "INPUT: MIC through the preamp (saturates softly above about -6 dBFS), LINE clean, RMX the playback of this track",
                );
                if switch.changed() {
                    track.channel.source = sources[chosen];
                    let range = trim_range(track.channel.source);
                    track.channel.trim_db = track.channel.trim_db.clamp(*range.start(), *range.end());
                    changed = true;
                }
                let y = top + 44.0;
                let lit = now < track.ol_until;
                ui.painter().circle_filled(pos2(cx - 38.0, y), 3.0, if lit { palette::LED } else { palette::PANEL_DARK });
                tag(ui, pos2(cx - 34.0, y), Align2::LEFT_CENTER, "OL");
                tag(ui, pos2(cx + 20.0, y), Align2::LEFT_CENTER, "TRIM");
                let range = trim_range(track.channel.source);
                let trim = place(ui, cx, y - 15.0, vec2(30.0, 30.0), |ui| {
                    knob(ui, &mut track.channel.trim_db, range, 0.0, palette::RED, 30.0, true)
                })
                .on_hover_text(if track.channel.auto {
                    format!("TRIM {:+.1} dB: for the input only (double-click: 0 dB)", track.channel.trim_db)
                } else {
                    format!("TRIM {:+.1} dB (dubbelklik: 0 dB)", track.channel.trim_db)
                });
                changed |= trim.changed() || trim.double_clicked();
            });
            // EQ: HIGH, MID and LOW top to bottom on the centre line, as on the 388.
            section(ui, "EQ", BAND_EQ, |ui| {
                let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
                for (row, band) in [2, 1, 0].into_iter().enumerate() {
                    let y = top + 19.0 + 38.0 * row as f32;
                    let entry = match band {
                        0 => &mut track.channel.eq.low,
                        1 => &mut track.channel.eq.mid,
                        _ => &mut track.channel.eq.high,
                    };
                    tag(ui, pos2(cx - 22.0, y), Align2::RIGHT_CENTER, EQ_NAMES[band]);
                    let response =
                        place(ui, cx, y - 17.0, vec2(34.0, 34.0), |ui| eq_knob(ui, entry, &EQ_RANGES[band], EQ_DEFAULTS[band].freq, 34.0));
                    let tip = format!(
                        "{} {} {:+.1} dB. Centre: gain, ring: frequency. Double-click resets.",
                        EQ_NAMES[band],
                        format_freq(entry.freq_hz),
                        entry.gain_db
                    );
                    changed |= response.on_hover_text(tip).changed();
                }
            });
            // The sends: AUX (PRE or POST) and EFF, in both modes.
            section(ui, "AUX", BAND_AUX, |ui| {
                let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
                let channel = &mut track.channel;
                let tip = "PRE: the AUX send does not depend on EQ, fader and MUTE (a headphone mix)";
                let pre = place(ui, cx - 36.0, top, vec2(34.0, KEY_NARROW_H), |ui| {
                    KeyCap::new("PRE", keys::NARROW).colour(KeyColour::Green).lamp(channel.aux_pre, palette::LED_GREEN).show(ui)
                });
                if pre.on_hover_text(tip).clicked() {
                    channel.aux_pre = !channel.aux_pre;
                    changed = true;
                }
                let aux = place(ui, cx, top, vec2(30.0, 30.0), |ui| {
                    knob(ui, &mut channel.aux_db, MIN_DB..=MAX_DB, MIN_DB, palette::GREEN, 30.0, true)
                })
                .on_hover_text(format!(
                    "AUX {}: PRE before EQ and fader, POST after the fader (double-click: off)",
                    format_db(channel.aux_db)
                ));
                changed |= aux.changed() || aux.double_clicked();
                tag(ui, pos2(cx - 22.0, top + 51.0), Align2::RIGHT_CENTER, "EFF");
                let eff = place(ui, cx, top + 36.0, vec2(30.0, 30.0), |ui| {
                    knob(ui, &mut channel.eff_db, MIN_DB..=MAX_DB, MIN_DB, palette::BLUE, 30.0, true)
                })
                .on_hover_text(format!("EFF {}: after the fader, to the echo (double-click: off)", format_db(channel.eff_db)));
                changed |= eff.changed() || eff.double_clicked();
            });
            // ASSIGN to the PGM buses and to L-R: the keys in one column, their numbers in front.
            if !simple {
                section(ui, "ASSIGN", BAND_ASSIGN, |ui| {
                    let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
                    let key_x = cx - 8.0;
                    for row in 0..5 {
                        let y = top + 8.0 + 17.0 * row as f32;
                        let (name, on, colour) = if row < 4 {
                            (format!("{}-{}", 2 * row + 1, 2 * row + 2), track.channel.assign[row], KeyColour::Beige)
                        } else {
                            ("L-R".to_string(), track.channel.lr, KeyColour::Red)
                        };
                        let label = tag(ui, pos2(key_x - 15.0, y), Align2::RIGHT_CENTER, &name);
                        keys::paint_lamp(ui.painter(), pos2(label.left() - 6.0, y), on, palette::LED_GREEN);
                        let key =
                            place(ui, key_x, y - 7.0, keys::NARROW, |ui| KeyCap::new("", keys::NARROW).colour(colour).bare().show(ui));
                        let tip = if row < 4 {
                            format!("To PGM bus {} (PAN left) and {} (PAN right)", 2 * row + 1, 2 * row + 2)
                        } else {
                            "L-R: to the stereo bus. With a channel on L-R you hear only the L-R channels, not the MONITOR".to_string()
                        };
                        if key.on_hover_text(tip).clicked() {
                            if row < 4 {
                                track.channel.assign[row] = !track.channel.assign[row];
                            } else {
                                track.channel.lr = !track.channel.lr;
                            }
                            changed = true;
                        }
                    }
                    let tip = format!(
                        "DIRECT: track {} records this channel (after TRIM and EQ, before the fader) instead of PGM bus {}",
                        index + 1,
                        index + 1
                    );
                    let direct = place(ui, cx + 32.0, top, vec2(46.0, KEY_NARROW_H), |ui| {
                        KeyCap::new("DIRECT", keys::NARROW).lamp(track.channel.direct, palette::LED_GREEN).show(ui)
                    });
                    if direct.on_hover_text(tip).clicked() {
                        track.channel.direct = !track.channel.direct;
                        changed = true;
                    }
                    let tip = "AUTO: the tape, and the input once this track is on REC (with INSERT from the punch-in point). The take gets no EQ";
                    let auto = place(ui, cx + 32.0, top + 36.0, vec2(46.0, KEY_NARROW_H), |ui| {
                        KeyCap::new("AUTO", keys::NARROW).lamp(track.channel.auto, palette::LED_GREEN).show(ui)
                    });
                    if auto.on_hover_text(tip).clicked() {
                        track.channel.auto = !track.channel.auto;
                        if track.channel.auto && track.channel.source == ChannelSource::Rmx {
                            track.channel.source = ChannelSource::Line;
                        }
                        changed = true;
                    }
                });
            }
            section(ui, "PAN", BAND_PAN, |ui| {
                let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
                let pan_tip = if simple {
                    format!("PAN {:+.2} (double-click: centre)", track.pan)
                } else {
                    format!("PAN {:+.2}: left to the odd bus, right to the even bus (double-click: centre)", track.pan)
                };
                let pan = place(ui, cx, top, vec2(30.0, 30.0), |ui| knob(ui, &mut track.pan, -1.0..=1.0, 0.0, palette::CREAM, 30.0, true))
                    .on_hover_text(pan_tip);
                changed |= pan.changed() || pan.double_clicked();
                let (left, right) = if simple { ("L", "R") } else { ("ODD", "EVEN") };
                tag(ui, pos2(cx - 18.0, top + 26.0), Align2::RIGHT_CENTER, left);
                tag(ui, pos2(cx + 18.0, top + 26.0), Align2::LEFT_CENTER, right);
            });
            section(ui, "", BAND_KEYS, |ui| {
                let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
                // REC FUNCTION: blinks when ready, lit while recording, as on the 388.
                let lit = recording || (track.armed && blink);
                let rec = place(ui, cx - 34.0, top, vec2(34.0, KEY_NARROW_H), |ui| {
                    KeyCap::new("REC", keys::NARROW).colour(KeyColour::Red).lamp(lit, palette::LED).show(ui)
                });
                if rec.on_hover_text("REC FUNCTION: track ready to record").clicked() {
                    arm = true;
                }
                let mute = place(ui, cx, top, vec2(34.0, KEY_NARROW_H), |ui| {
                    KeyCap::new("MUTE", keys::NARROW).lamp(track.mute, palette::LED_ORANGE).show(ui)
                });
                if mute.clicked() {
                    track.mute = !track.mute;
                    changed = true;
                }
                // SOLO is a DAW habit, not a 388 control: it only changes what you hear.
                let button = place(ui, cx + 34.0, top, vec2(34.0, KEY_NARROW_H), |ui| {
                    KeyCap::new("SOLO", keys::NARROW).lamp(track.solo, palette::LED_GREEN).show(ui)
                });
                if button.on_hover_text("SOLO: hear only the soloed tracks. Recordings and buses are not affected").clicked() {
                    track.solo = !track.solo;
                    solo = true;
                }
            });
            ui.add_space(FADER_GAP);
            let fader = fader(ui, &mut track.fader_db, palette::CREAM);
            if fader.changed() || fader.double_clicked() {
                changed = true;
            }
            ui.label(RichText::new(format_db(track.fader_db)).size(11.0).color(palette::TEXT_DIM));
            // The number and the name under the fader, as on the 388.
            ui.label(bold(format!("{}", index + 1)).size(20.0).color(palette::CREAM));
            let (name, colour) = match missing.first() {
                _ if loading => ("loading...".to_string(), palette::ORANGE),
                Some(path) => (format!("missing: {}", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()), palette::LED),
                None if has_content => (track.name.clone(), palette::TEXT_DIM),
                None => ("empty".to_string(), palette::TEXT_DIM),
            };
            match &mut renaming {
                Some((track_index, text, focus)) if *track_index == index => {
                    let field = ui.add(egui::TextEdit::singleline(text).desired_width(100.0).font(FontId::proportional(11.0)));
                    if *focus {
                        field.request_focus();
                        *focus = false;
                    }
                    // An egui text field also loses its focus on Escape: look at that first.
                    if ui.input(|i| i.key_pressed(Key::Escape)) {
                        renaming = None;
                    } else if field.lost_focus() {
                        renamed = renaming.take().map(|(_, text, _)| text);
                    }
                }
                _ => {
                    let label = ui.add(egui::Label::new(RichText::new(name).size(11.0).color(colour)).truncate().sense(Sense::click()));
                    let label = if missing.is_empty() {
                        label.on_hover_text("Double-click: rename")
                    } else {
                        let paths: Vec<String> = missing.iter().map(|p| p.display().to_string()).collect();
                        label.on_hover_text(format!("Not found: {}", paths.join(", ")))
                    };
                    if label.double_clicked() {
                        renaming = Some((index, track.name.clone(), true));
                    }
                }
            }
        });
        self.editor.renaming = renaming;
        if let Some(name) = renamed {
            self.rename(index, name);
        }
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
        if load {
            self.import(index);
        }
        if clear {
            self.clear_track(index);
        }
    }

    /// The left master column: EFFECT RTN, the plate and the echo, and in the 388 routing the
    /// MONITOR section with the PGM MASTERS under it, the faders on the line of the channel faders.
    pub(crate) fn return_monitor_column(&mut self, ui: &mut Ui) {
        let classic = self.mixer.mode == Routing::Classic;
        let mut mixer_changed = false;
        let mut monitors = Vec::new();
        let quarter = 60.0 / self.metronome.bpm.clamp(20.0, 400.0) as f32;
        column(ui, 196.0, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            section(ui, "EFFECT RTN", BAND_RTN, |ui| {
                let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
                tag(ui, pos2(cx - 21.0, top - 8.0), Align2::CENTER_CENTER, "GAIN");
                tag(ui, pos2(cx + 21.0, top - 8.0), Align2::CENTER_CENTER, "PAN");
                for (index, name) in ["plate", "echo"].into_iter().enumerate() {
                    let y = top + 16.0 + 34.0 * index as f32;
                    tag(ui, pos2(cx - 56.0, y), Align2::CENTER_CENTER, &format!("{}", index + 1));
                    let rtn = &mut self.mixer.effects.rtn[index];
                    let tip = format!("RTN {} ({name}) {} (double-click: 0 dB)", index + 1, format_db(rtn.gain_db));
                    let gain = place(ui, cx - 21.0, y - 15.0, vec2(30.0, 30.0), |ui| {
                        knob(ui, &mut rtn.gain_db, MIN_DB..=MAX_DB, 0.0, palette::BLUE, 30.0, true)
                    });
                    let gain = gain.on_hover_text(tip);
                    mixer_changed |= gain.changed() || gain.double_clicked();
                    let tip = format!("PAN RTN {} {:+.2} (double-click: centre)", index + 1, rtn.pan);
                    let pan = place(ui, cx + 21.0, y - 15.0, vec2(30.0, 30.0), |ui| {
                        knob(ui, &mut rtn.pan, -1.0..=1.0, 0.0, palette::CREAM, 30.0, true)
                    });
                    let pan = pan.on_hover_text(tip);
                    mixer_changed |= pan.changed() || pan.double_clicked();
                }
            });
            // Three knobs side by side on the same three columns for the plate and the echo.
            let turn = |ui: &mut Ui,
                        x: f32,
                        top: f32,
                        value: &mut f32,
                        range: std::ops::RangeInclusive<f32>,
                        default: f32,
                        colour,
                        enabled,
                        tip: String| {
                let response =
                    place(ui, x, top, vec2(30.0, 30.0), |ui| knob(ui, value, range, default, colour, 30.0, enabled)).on_hover_text(tip);
                response.changed() || response.double_clicked()
            };
            let effects = &mut self.mixer.effects;
            section(ui, "PLATE (AUX)", BAND_PLATE, |ui| {
                let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
                let plate = &mut effects.plate;
                let tip = format!("DECAY {:.1} s: how long the reverb lasts (double-click: 2 s)", plate.decay_s);
                mixer_changed |= turn(ui, cx - 48.0, top, &mut plate.decay_s, 0.5..=6.0, 2.0, palette::GREEN, true, tip);
                let tip = format!("PRE-DELAY {:.0} ms: wait before the reverb starts (double-click: 20 ms)", plate.predelay_ms);
                mixer_changed |= turn(ui, cx, top, &mut plate.predelay_ms, 0.0..=100.0, 20.0, palette::GREEN, true, tip);
                let tip = format!("TONE {:.0}%: from dark to bright (double-click: centre)", plate.tone * 100.0);
                mixer_changed |= turn(ui, cx + 48.0, top, &mut plate.tone, 0.0..=1.0, 0.5, palette::GREEN, true, tip);
                for (x, name) in [(cx - 48.0, "DECAY"), (cx, "PRE"), (cx + 48.0, "TONE")] {
                    tag(ui, pos2(x, top + 38.0), Align2::CENTER_CENTER, name);
                }
            });
            section(ui, "ECHO (EFF)", BAND_ECHO, |ui| {
                let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
                let echo = &mut effects.echo;
                let synced = echo.sync != EchoSyncEntry::Off;
                let sync_ms = 1000.0
                    * match echo.sync {
                        EchoSyncEntry::Off => echo.time_ms / 1000.0,
                        EchoSyncEntry::Quarter => quarter,
                        EchoSyncEntry::Eighth => quarter / 2.0,
                        EchoSyncEntry::DottedEighth => quarter * 0.75,
                    };
                let tip = if synced {
                    format!("TIME {:.0} ms, follows the tempo (choose FREE to set it yourself)", sync_ms.min(2000.0))
                } else {
                    format!("TIME {:.0} ms (double-click: 300 ms)", echo.time_ms)
                };
                mixer_changed |= turn(ui, cx - 48.0, top, &mut echo.time_ms, 40.0..=800.0, 300.0, palette::BLUE, !synced, tip);
                let tip = format!("FEEDBACK {:.0}%: how many repeats (double-click: 35%)", echo.feedback * 100.0);
                mixer_changed |= turn(ui, cx, top, &mut echo.feedback, 0.0..=0.9, 0.35, palette::BLUE, true, tip);
                let tip = format!("TONE {:.0}%: how dark the repeats get (double-click: centre)", echo.tone * 100.0);
                mixer_changed |= turn(ui, cx + 48.0, top, &mut echo.tone, 0.0..=1.0, 0.5, palette::BLUE, true, tip);
                for (x, name) in [(cx - 48.0, "TIME"), (cx, "FEEDB."), (cx + 48.0, "TONE")] {
                    tag(ui, pos2(x, top + 38.0), Align2::CENTER_CENTER, name);
                }
                for (column, (sync, label, tip)) in [
                    (EchoSyncEntry::Off, "FREE", "TIME in milliseconds"),
                    (EchoSyncEntry::Quarter, "1/4", "A quarter note of the metronome tempo"),
                    (EchoSyncEntry::Eighth, "1/8", "An eighth note of the metronome tempo"),
                    (EchoSyncEntry::DottedEighth, "1/8.", "A dotted eighth of the metronome tempo"),
                ]
                .into_iter()
                .enumerate()
                {
                    let on = echo.sync == sync;
                    let x = cx - 63.0 + 42.0 * column as f32;
                    let key = place(ui, x, top + 48.0, vec2(34.0, KEY_NARROW_H), |ui| {
                        KeyCap::new(label, keys::NARROW).lamp(on, palette::LED_GREEN).show(ui)
                    });
                    if key.on_hover_text(tip).clicked() && !on {
                        echo.sync = sync;
                        mixer_changed = true;
                    }
                }
            });
            if classic {
                section(ui, "MONITOR", BAND_MONITOR, |ui| {
                    let (rows, mon) = self.monitor_rows(ui);
                    monitors = rows;
                    mixer_changed |= mon;
                });
            }
            ui.add_space(fader_top(classic) - return_bands(classic) + FADER_GAP);
            if classic {
                // `horizontal_top`: both faders start on the fader line (`horizontal` would
                // centre them on each other).
                ui.horizontal_top(|ui| {
                    // ODD and EVEN side by side in the middle of the column, as on the 388.
                    ui.add_space((ui.available_width() - 2.0 * 74.0 - ui.spacing().item_spacing.x) / 2.0);
                    for (label, db) in [("ODD", &mut self.mixer.odd_db), ("EVEN", &mut self.mixer.even_db)] {
                        // A column as wide as the fader, so the labels under it do not push EVEN away.
                        column(ui, 74.0, |ui| {
                            let response = fader(ui, db, palette::ORANGE);
                            mixer_changed |= response.changed() || response.double_clicked();
                            ui.label(RichText::new(format_db(*db)).size(10.0).color(palette::TEXT_DIM));
                            ui.label(RichText::new(label).size(10.0).color(palette::TEXT_DIM));
                        });
                    }
                });
                ui.label(RichText::new("PGM MASTERS").size(9.0).color(palette::TEXT_DIM));
            }
        });
        for index in &monitors {
            self.send_monitor(*index);
        }
        if mixer_changed {
            self.send_mixer();
        }
        if !monitors.is_empty() || mixer_changed {
            self.mark_dirty();
        }
    }

    /// The MONITOR rows on fixed columns: per track GAIN, PAN, M and the TRK/PGM meter, then MON.
    /// Returns the tracks whose monitor changed and whether MON changed.
    fn monitor_rows(&mut self, ui: &mut Ui) -> (Vec<usize>, bool) {
        let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
        let mut changed = Vec::new();
        let mut mixer_changed = false;
        let (gain_x, pan_x, m_x, bar_x) = (cx - 44.0, cx - 8.0, cx + 30.0, cx + 70.0);
        for (x, name) in [(gain_x, "GAIN"), (pan_x, "PAN"), (m_x, "M"), (bar_x, "TRK")] {
            tag(ui, pos2(x, top - 8.0), Align2::CENTER_CENTER, name);
        }
        for (index, track) in self.tracks.iter_mut().enumerate() {
            let y = top + 13.0 + 30.0 * index as f32;
            tag(ui, pos2(cx - 80.0, y), Align2::CENTER_CENTER, &format!("{}", index + 1));
            let monitor = &mut track.monitor;
            let gain = place(ui, gain_x, y - 12.0, vec2(24.0, 24.0), |ui| {
                knob(ui, &mut monitor.gain_db, MIN_DB..=MONITOR_MAX_DB, 0.0, palette::CREAM, 24.0, true)
            })
            .on_hover_text(format!("MONITOR GAIN track {}: {} (double-click: 0 dB)", index + 1, format_db(monitor.gain_db)));
            let pan =
                place(ui, pan_x, y - 12.0, vec2(24.0, 24.0), |ui| knob(ui, &mut monitor.pan, -1.0..=1.0, 0.0, palette::CREAM, 24.0, true))
                    .on_hover_text(format!("MONITOR PAN {:+.2} (double-click: centre)", monitor.pan));
            keys::paint_lamp(ui.painter(), pos2(m_x - 18.0, y), monitor.mute, palette::LED_ORANGE);
            let mute = place(ui, m_x, y - 7.0, keys::NARROW, |ui| KeyCap::new("", keys::NARROW).bare().show(ui));
            if mute.clicked() {
                monitor.mute = !monitor.mute;
            }
            if gain.changed() || gain.double_clicked() || pan.changed() || pan.double_clicked() || mute.clicked() {
                changed.push(index);
            }
            let db = track.trk_meter_db;
            place(ui, bar_x, y - 3.0, vec2(36.0, 6.0), |ui| level_bar(ui, db));
        }
        let y = top + 250.0;
        tag(ui, pos2(cx - 80.0, y + 20.0), Align2::CENTER_CENTER, "MON");
        for (x, (mon, label, tip)) in [
            (cx - 24.0, (MonSwitch::Cue, "CUE", "Tape or input, following REC FUNCTION and INSERT (as on the 388)")),
            (cx + 24.0, (MonSwitch::Pgm, "PGM", "Always the PGM buses")),
        ] {
            let on = self.mixer.mon == mon;
            let key = place(ui, x, y, vec2(40.0, KEY_SMALL_H), |ui| KeyCap::new(label, keys::SMALL).lamp(on, palette::LED_GREEN).show(ui));
            if key.on_hover_text(tip).clicked() && !on {
                self.mixer.mon = mon;
                mixer_changed = true;
            }
        }
        (changed, mixer_changed)
    }

    /// The right master column: the AUX and EFF masters, the meter keys and MONO, what the main
    /// output plays with its level, and the STEREO MASTER.
    pub(crate) fn master_column(&mut self, ui: &mut Ui) {
        let classic = self.mixer.mode == Routing::Classic;
        let mut mixer_changed = false;
        column(ui, 124.0, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            // AUX MASTER and EFF MASTER, to their own interface outputs (Instellingen, Audio).
            section(ui, "MASTER", BAND_MASTER, |ui| {
                let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
                let aux = place(ui, cx - 24.0, top, vec2(30.0, 30.0), |ui| {
                    knob(ui, &mut self.mixer.aux_master_db, MIN_DB..=MAX_DB, 0.0, palette::GREEN, 30.0, true)
                })
                .on_hover_text(format!(
                    "AUX MASTER {} (double-click: 0 dB). Choose its output in Settings, Audio",
                    format_db(self.mixer.aux_master_db)
                ));
                mixer_changed |= aux.changed() || aux.double_clicked();
                let eff = place(ui, cx + 24.0, top, vec2(30.0, 30.0), |ui| {
                    knob(ui, &mut self.mixer.eff_master_db, MIN_DB..=MAX_DB, 0.0, palette::BLUE, 30.0, true)
                })
                .on_hover_text(format!(
                    "EFF MASTER {} (double-click: 0 dB). Choose its output in Settings, Audio",
                    format_db(self.mixer.eff_master_db)
                ));
                mixer_changed |= eff.changed() || eff.double_clicked();
                tag(ui, pos2(cx - 24.0, top + 38.0), Align2::CENTER_CENTER, "AUX");
                tag(ui, pos2(cx + 24.0, top + 38.0), Align2::CENTER_CENTER, "EFF");
            });
            section(ui, "", BAND_SWITCHES, |ui| {
                let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
                for (meter, label, tip, x) in [
                    (MeterSwitch::Lr, "L-R", "The stereo meters show left and right", cx - 24.0),
                    (MeterSwitch::AuxEff, "AUX-EFF", "The stereo meters show AUX and EFF after their master", cx + 24.0),
                ] {
                    let on = self.mixer.meter == meter;
                    let key = place(ui, x, top, vec2(48.0, KEY_SMALL_H), |ui| {
                        KeyCap::new(label, keys::SMALL).lamp(on, palette::LED_GREEN).show(ui)
                    });
                    if key.on_hover_text(tip).clicked() && !on {
                        self.mixer.meter = meter;
                        mixer_changed = true;
                    }
                }
                for (post, label, tip, x) in [
                    (false, "PRE", "TRK meters show the level after TRIM, before the fader (as on the 388)", cx - 24.0),
                    (true, "POST", "TRK meters show the level after fader and mute", cx + 24.0),
                ] {
                    let lit = self.meter_post == post;
                    let key = place(ui, x, top + 35.0, vec2(48.0, KEY_SMALL_H), |ui| {
                        KeyCap::new(label, keys::SMALL).lamp(lit, palette::LED_GREEN).show(ui)
                    });
                    if key.on_hover_text(tip).clicked() && !lit {
                        self.meter_post = post;
                        self.mark_dirty();
                    }
                }
                let mono = place(ui, cx, top + 70.0, vec2(48.0, KEY_SMALL_H), |ui| {
                    KeyCap::new("MONO", keys::SMALL).lamp(self.mixer.mono, palette::LED_GREEN).show(ui)
                });
                if mono.on_hover_text("Left and right together on both sides").clicked() {
                    self.mixer.mono = !self.mixer.mono;
                    mixer_changed = true;
                }
            });
            section(ui, "LISTEN", BAND_LISTEN, |ui| {
                let (cx, top) = (ui.max_rect().center().x, ui.max_rect().top());
                for (listen, label, tip, x) in [
                    (ListenSwitch::Stereo, "STEREO", "The main output gets the STEREO MASTER", cx - 40.0),
                    (ListenSwitch::Aux, "AUX", "The main output gets AUX, mono on both sides", cx),
                    (ListenSwitch::Eff, "EFF", "The main output gets EFF, mono on both sides", cx + 40.0),
                ] {
                    let on = self.mixer.listen == listen;
                    let key = place(ui, x, top, vec2(40.0, KEY_SMALL_H), |ui| {
                        KeyCap::new(label, keys::SMALL).lamp(on, palette::LED_GREEN).show(ui)
                    });
                    if key.on_hover_text(tip).clicked() && !on {
                        self.mixer.listen = listen;
                        mixer_changed = true;
                    }
                }
                tag(ui, pos2(cx, top + 78.0), Align2::CENTER_CENTER, "PHONES");
                let phones = place(ui, cx, top + 42.0, vec2(30.0, 30.0), |ui| {
                    knob(ui, &mut self.mixer.phones_db, MIN_DB..=MAX_DB, 0.0, palette::CREAM, 30.0, true)
                })
                .on_hover_text(format!(
                    "MONITOR/PHONES {}: level of the main output. The click has its own volume (double-click: 0 dB)",
                    format_db(self.mixer.phones_db)
                ));
                mixer_changed |= phones.changed() || phones.double_clicked();
            });
            ui.add_space(fader_top(classic) - BAND_MASTER - BAND_SWITCHES - BAND_LISTEN + FADER_GAP);
            let response = fader(ui, &mut self.master_db, palette::RED);
            if response.changed() || response.double_clicked() {
                let gain = db_to_gain(self.master_db);
                self.engine_send(Command::SetMaster(gain));
                self.mark_dirty();
            }
            ui.label(RichText::new(format_db(self.master_db)).size(11.0).color(palette::TEXT_DIM));
            ui.label(RichText::new("STEREO MASTER").size(9.0).color(palette::TEXT_DIM));
        });
        if mixer_changed {
            self.send_mixer();
            self.mark_dirty();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_faders_start_below_all_bands() {
        assert_eq!(fader_top(true), 552.0);
        assert_eq!(fader_top(false), 552.0 - BAND_ASSIGN, "no ASSIGN in the simple mode");
        assert!(return_bands(true) <= fader_top(true), "EFFECT RTN, PLATE, ECHO and MONITOR fit above the faders");
        assert!(return_bands(false) <= fader_top(false));
        assert!(BAND_MASTER + BAND_SWITCHES + BAND_LISTEN <= fader_top(false));
    }
}
