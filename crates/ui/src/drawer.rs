//! The SPOREN view with the tracks, one of the two views next to the mixer; the leather armrest
//! under either view switches between them. Drawing uses the painter; the mouse and keys become
//! the edits in `edit.rs`.

use super::*;
use crate::sources::SourceState;
use crate::timeline::{self, Edge, Hit, Timeline};
use egui::{Align2, CursorIcon, FontId, Pos2, Rect, pos2};

/// Height of the armrest (the switch between the views).
pub(crate) const ARMREST: f32 = 18.0;
const RULER: f32 = 24.0;
const HEAD: f32 = 190.0;
/// A lane needs this height for its volume and pan under the name.
const CONTROLS_MIN: f32 = 44.0;
/// The lowest the eight lanes can go in the SPOREN view; the zoom keeps at least this.
pub(crate) const LANES_MIN: f32 = 8.0 * 52.0 + RULER + 32.0;

/// What the mouse is doing in the drawer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Drag {
    /// Dragging out a stretch over all tracks from `anchor` seconds.
    Range { anchor: f64 },
    /// Moving the selection; `grab` is where it was taken (seconds and track).
    Move { grab: f64, track: usize, time_delta: f64, track_delta: isize },
    /// Pulling an edge of one region to `at`.
    Trim { track: usize, index: usize, edge: Edge, at: f64 },
}

/// Where the parts of the drawer are on screen.
#[derive(Debug, Clone, Copy)]
struct Areas {
    /// Track numbers and names, left.
    head: Rect,
    /// The time ruler, top.
    ruler: Rect,
    /// The eight lanes.
    lanes: Rect,
    lane_h: f32,
}

impl Areas {
    /// The rectangle of one lane (`head` false) or of its name (`head` true).
    fn lane(&self, track: usize, head: bool) -> Rect {
        let top = self.lanes.top() + track as f32 * self.lane_h;
        let (left, right) = if head { (self.head.left(), self.head.right()) } else { (self.lanes.left(), self.lanes.right()) };
        Rect::from_min_max(pos2(left, top), pos2(right, top + self.lane_h))
    }

    /// The track at a height.
    fn track_at(&self, y: f32) -> Option<usize> {
        timeline::lane_at(self.lanes.top(), self.lane_h, TRACKS, y)
    }
}

/// A small triangle pointing up or down, for the armrest.
fn arrow(painter: &egui::Painter, centre: Pos2, up: bool, colour: Color32) {
    let (tip, base) = if up { (-3.5, 2.5) } else { (3.5, -2.5) };
    let points = vec![pos2(centre.x - 4.0, centre.y + base), pos2(centre.x + 4.0, centre.y + base), pos2(centre.x, centre.y + tip)];
    painter.add(egui::Shape::convex_polygon(points, colour, Stroke::NONE));
}

impl DawApp {
    /// The SPOREN view: the edit keys and the lanes.
    pub(crate) fn tracks_view(&mut self, ui: &mut Ui) {
        let recording = self.recording_now();
        ui.add_enabled_ui(!recording, |ui| self.toolbar(ui));
        self.lanes(ui, recording);
    }

    /// Switch between the MIXER and the SPOREN view.
    pub(crate) fn toggle_view(&mut self) {
        self.editor.settings.open = !self.editor.settings.open;
        self.mark_dirty();
    }

    /// The leather armrest: click it to switch to the other view.
    pub(crate) fn armrest(&mut self, ui: &mut Ui) {
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ARMREST), Sense::click());
        let tracks = self.editor.settings.open;
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, palette::LEATHER);
        painter.line_segment([rect.left_top(), rect.right_top()], Stroke::new(2.0, palette::LEATHER_EDGE));
        let label = if tracks { "MIXER" } else { "SPOREN" };
        painter.text(rect.center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(10.0), palette::TEXT_DIM);
        arrow(painter, rect.center() - vec2(4.0, 0.0), !tracks, palette::TEXT_DIM);
        let tip = if tracks { "Klik of Tab: naar de mixer" } else { "Klik of Tab: naar de sporen" };
        if response.on_hover_text(tip).clicked() {
            self.toggle_view();
        }
    }

    /// SPOREN with the edit keys, RASTER, zoom and the undo count.
    fn toolbar(&mut self, ui: &mut Ui) {
        Frame::new().fill(palette::PANEL).inner_margin(Margin::symmetric(10, 6)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("SPOREN").strong().size(12.0).color(palette::CREAM));
                ui.add_space(8.0);
                if KeyCap::new("SPLITS", keys::SMALL)
                    .show(ui)
                    .on_hover_text("S: de selectie op de afspeelpositie, zonder selectie alle sporen")
                    .clicked()
                {
                    self.split();
                }
                if KeyCap::new("KNIP", keys::SMALL)
                    .show(ui)
                    .on_hover_text("Cmd+X: een tijdvak is een splice (de rest schuift aan)")
                    .clicked()
                {
                    self.cut();
                    ui.ctx().copy_text("DAW 388".into());
                }
                if KeyCap::new("KOPIEER", keys::SMALL).show(ui).on_hover_text("Cmd+C").clicked() {
                    self.copy();
                    ui.ctx().copy_text("DAW 388".into());
                }
                if KeyCap::new("PLAK", keys::SMALL)
                    .show(ui)
                    .on_hover_text("Cmd+V: op de afspeelpositie; een tijdvak wordt ingevoegd")
                    .clicked()
                {
                    self.paste();
                }
                if KeyCap::new("WIS", keys::SMALL)
                    .show(ui)
                    .on_hover_text("Delete: de selectie of het tijdvak, zonder aanschuiven")
                    .clicked()
                {
                    self.delete();
                }
                ui.add_space(8.0);
                let snap = self.editor.settings.snap;
                if KeyCap::new("RASTER", keys::SMALL)
                    .lamp(snap, palette::LED_GREEN)
                    .show(ui)
                    .on_hover_text("Vastklikken op de tellen van de metronoom; Alt: tijdelijk vrij")
                    .clicked()
                {
                    self.editor.settings.snap = !snap;
                    self.mark_dirty();
                }
                ui.add_space(8.0);
                let current =
                    self.editor.settings.zoom.unwrap_or(if self.editor.shown_zoom > 0.0 { self.editor.shown_zoom } else { 100.0 });
                if KeyCap::new("−", keys::SMALL).show(ui).on_hover_text("Uitzoomen (of Cmd en scrollen)").clicked() {
                    self.editor.settings.zoom = Some((current * 0.5).clamp(timeline::ZOOM_MIN, timeline::ZOOM_MAX));
                    self.mark_dirty();
                }
                if KeyCap::new("PAST", keys::SMALL)
                    .lamp(self.editor.settings.zoom.is_none(), palette::LED_GREEN)
                    .show(ui)
                    .on_hover_text("Het hele nummer in beeld")
                    .clicked()
                {
                    self.editor.settings.zoom = None;
                    self.editor.scroll = 0.0;
                    self.mark_dirty();
                }
                if KeyCap::new("+", keys::SMALL).show(ui).on_hover_text("Inzoomen (of Cmd en scrollen)").clicked() {
                    self.editor.settings.zoom = Some((current * 2.0).clamp(timeline::ZOOM_MIN, timeline::ZOOM_MAX));
                    self.mark_dirty();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let m = &self.metronome;
                    let text = if ui.input(|i| !i.raw.hovered_files.is_empty()) {
                        format!("Loslaten: vanaf spoor {} op {}", self.editor.target + 1, format_counter(self.playhead()))
                    } else {
                        format!("Undo {}/{} · {:.0} BPM {}/{}", self.history.len(), history::STEPS, m.bpm, m.beats, m.unit)
                    };
                    ui.label(RichText::new(text).size(11.0).color(palette::TEXT_DIM));
                });
            });
        });
    }

    /// The ruler and the lanes: drawing, the mouse and dropped files.
    fn lanes(&mut self, ui: &mut Ui, recording: bool) {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let head = Rect::from_min_max(rect.min, pos2(rect.left() + HEAD, rect.bottom()));
        let ruler = Rect::from_min_max(pos2(head.right(), rect.top()), pos2(rect.right(), rect.top() + RULER));
        let lanes = Rect::from_min_max(pos2(head.right(), ruler.bottom()), rect.max);
        let areas = Areas { head, ruler, lanes, lane_h: (lanes.height() / TRACKS as f32).max(12.0) };
        let song_end = self.tracks.iter().map(|t| track_end(&t.regions)).fold(0.0, f64::max);
        let mut zoom = self.editor.settings.zoom.unwrap_or_else(|| Timeline::fit(song_end + 2.0, lanes.width()));

        if response.hovered() {
            let (zoom_delta, scroll, pointer) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.pointer.hover_pos()));
            let line = Timeline { left: lanes.left(), start: self.editor.scroll, zoom };
            if zoom_delta != 1.0 {
                let new = (zoom * f64::from(zoom_delta)).clamp(timeline::ZOOM_MIN, timeline::ZOOM_MAX);
                self.editor.scroll = line.zoomed_start(new, pointer.map_or(lanes.center().x, |p| p.x));
                zoom = new;
                self.editor.settings.zoom = Some(new);
                self.mark_dirty();
            } else if scroll != egui::Vec2::ZERO {
                self.editor.scroll = (self.editor.scroll - f64::from(scroll.x + scroll.y) / zoom).max(0.0);
            }
        }
        // While playing, the drawer pages along with the tape.
        let playhead = self.playhead();
        let visible = f64::from(lanes.width()) / zoom;
        if self.engine.as_ref().is_some_and(EngineHandle::is_playing)
            && (playhead < self.editor.scroll || playhead > self.editor.scroll + visible)
        {
            self.editor.scroll = (playhead - 0.1 * visible).max(0.0);
        }
        self.editor.shown_zoom = zoom;
        let line = Timeline { left: lanes.left(), start: self.editor.scroll, zoom };

        if !recording {
            self.lanes_input(ui, &response, &line, &areas);
        }
        self.paint(ui, rect, &areas, &line);
        self.track_controls(ui, &areas);
        if !recording {
            self.rename_field(ui, &areas);
            let dropped: Vec<PathBuf> = ui.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
            if !dropped.is_empty() {
                let at = self.playhead();
                self.drop_files(self.editor.target, at, dropped);
            }
        }
    }

    /// Volume and pan per track under its name, as in the mixer (same values).
    fn track_controls(&mut self, ui: &mut Ui, areas: &Areas) {
        if areas.lane_h < CONTROLS_MIN {
            return;
        }
        for index in 0..TRACKS {
            let head = areas.lane(index, true);
            let y = head.center().y + areas.lane_h * 0.22;
            let fader = Rect::from_center_size(pos2(head.left() + 82.0, y), vec2(120.0, 14.0));
            let pan = Rect::from_center_size(pos2(head.right() - 22.0, y), vec2(22.0, 22.0));
            let track = &mut self.tracks[index];
            let volume = ui.put(fader, |ui: &mut Ui| mini_fader(ui, &mut track.fader_db));
            let volume = volume.on_hover_text(format!("Volume {} (dubbelklik: 0 dB)", format_db(track.fader_db)));
            let turned = ui.put(pan, |ui: &mut Ui| knob(ui, &mut track.pan, -1.0..=1.0, 0.0, palette::CREAM, 20.0, true));
            let turned = turned.on_hover_text(format!("PAN {:+.2} (dubbelklik: midden)", track.pan));
            if volume.changed() || turned.changed() || turned.double_clicked() {
                self.send_channel(index);
                self.mark_dirty();
            }
        }
    }

    /// The track and what lies under `pos` in the lanes.
    fn lane_hit(&self, line: &Timeline, areas: &Areas, pos: Pos2) -> Option<(usize, Hit)> {
        if !areas.lanes.contains(pos) {
            return None;
        }
        let track = areas.track_at(pos.y)?;
        Some((track, timeline::hit(line, &self.tracks[track].regions, pos.x)))
    }

    /// What a moved or pulled edge can snap to: other regions' edges, the zero and CUE points,
    /// the playhead and, with RASTER, the beats up to `to` seconds.
    fn marks(&self, line: &Timeline, to: f64, except: &[(usize, usize)]) -> Vec<f64> {
        let mut marks = vec![self.zero_seconds, self.playhead()];
        marks.extend(self.cue_seconds);
        for (t, track) in self.tracks.iter().enumerate() {
            for (i, region) in track.regions.iter().enumerate() {
                if !except.contains(&(t, i)) {
                    marks.push(region.start);
                    marks.push(region.end());
                }
            }
        }
        if self.editor.settings.snap {
            let m = &self.metronome;
            let (beat, _) = timeline::beat_and_bar(m.bpm, m.beats, m.unit);
            marks.extend(timeline::beats(line.start, to, beat, m.beats).into_iter().map(|(t, _)| t));
        }
        marks
    }

    /// The mouse in the ruler and the lanes.
    fn lanes_input(&mut self, ui: &Ui, response: &egui::Response, line: &Timeline, areas: &Areas) {
        let (alt, shift) = ui.input(|i| (i.modifiers.alt, i.modifiers.shift));
        let reach = if alt { 0.0 } else { f64::from(timeline::SNAP_PIXELS) / line.zoom };
        let to = line.seconds(areas.lanes.right());

        if let Some(pos) = response.hover_pos() {
            match (self.editor.drag, self.lane_hit(line, areas, pos)) {
                (Some(Drag::Move { .. }), _) => ui.ctx().set_cursor_icon(CursorIcon::Grabbing),
                (Some(Drag::Trim { .. }), _) | (None, Some((_, Hit::Region { edge: Some(_), .. }))) => {
                    ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
                }
                (None, Some((_, Hit::Region { edge: None, .. }))) => ui.ctx().set_cursor_icon(CursorIcon::Grab),
                _ => {}
            }
        }

        // A click: where the button came up (press_origin is gone by then).
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            if areas.ruler.contains(pos) {
                let t = timeline::snap(line.seconds(pos.x).max(0.0), &self.marks(line, to, &[]), reach);
                if let Some(engine) = &mut self.engine {
                    engine.locate_seconds(t);
                }
            } else if let Some((track, hit)) = self.lane_hit(line, areas, pos) {
                self.editor.target = track;
                match hit {
                    Hit::Region { index, .. } => self.select_region(track, index, shift),
                    Hit::Empty => self.clear_selection(),
                }
            } else if areas.head.contains(pos)
                && let Some(track) = areas.track_at(pos.y)
            {
                self.editor.target = track;
            }
        }
        if response.double_clicked()
            && let Some(pos) = response.interact_pointer_pos()
            && areas.head.contains(pos)
            && let Some(track) = areas.track_at(pos.y)
        {
            self.editor.renaming = Some((track, self.tracks[track].name.clone(), true));
        }

        // A drag starts where the button went down: drag_started comes only after some movement.
        if response.drag_started()
            && let Some(pos) = ui.input(|i| i.pointer.press_origin())
        {
            if areas.ruler.contains(pos) {
                let anchor = timeline::snap(line.seconds(pos.x).max(0.0), &self.marks(line, to, &[]), reach);
                self.editor.drag = Some(Drag::Range { anchor });
            } else if let Some((track, Hit::Region { index, edge })) = self.lane_hit(line, areas, pos) {
                self.editor.target = track;
                let region = &self.tracks[track].regions[index];
                self.editor.drag = Some(match edge {
                    Some(Edge::Start) => Drag::Trim { track, index, edge: Edge::Start, at: region.start },
                    Some(Edge::End) => Drag::Trim { track, index, edge: Edge::End, at: region.end() },
                    None => {
                        if !self.editor.selection.contains(&(track, index)) {
                            self.select_region(track, index, shift);
                        }
                        Drag::Move { grab: line.seconds(pos.x), track, time_delta: 0.0, track_delta: 0 }
                    }
                });
            }
        }

        if response.dragged()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let t = line.seconds(pos.x);
            match self.editor.drag {
                Some(Drag::Range { anchor }) => {
                    let end = timeline::snap(t.max(0.0), &self.marks(line, to, &[]), reach);
                    self.select_range(anchor, end);
                }
                Some(Drag::Move { grab, track, .. }) => {
                    let y = pos.y.clamp(areas.lanes.top(), areas.lanes.bottom() - 1.0);
                    let lane = areas.track_at(y).unwrap_or(track);
                    let selection = self.editor.selection.clone();
                    let edges: Vec<f64> = selection
                        .iter()
                        .filter_map(|(t, i)| self.tracks.get(*t)?.regions.get(*i))
                        .flat_map(|r| [r.start, r.end()])
                        .collect();
                    let time_delta = timeline::snap_shift(&edges, t - grab, &self.marks(line, to, &selection), reach);
                    self.editor.drag = Some(Drag::Move { grab, track, time_delta, track_delta: lane as isize - track as isize });
                }
                Some(Drag::Trim { track, index, edge, .. }) => {
                    let at = timeline::snap(t, &self.marks(line, to, &[(track, index)]), reach);
                    self.editor.drag = Some(Drag::Trim { track, index, edge, at });
                }
                None => {}
            }
        }

        if response.drag_stopped() {
            match self.editor.drag.take() {
                Some(Drag::Move { time_delta, track_delta, .. }) if time_delta != 0.0 || track_delta != 0 => {
                    self.move_selection(track_delta, time_delta);
                }
                Some(Drag::Trim { track, index, edge, at }) => self.trim(track, index, edge, at),
                _ => {}
            }
        }
    }

    /// Everything the drawer shows.
    fn paint(&self, ui: &Ui, rect: Rect, areas: &Areas, line: &Timeline) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, palette::LANE);
        self.paint_ruler(&painter.with_clip_rect(areas.ruler), areas, line);

        let hovering = ui.input(|i| !i.raw.hovered_files.is_empty());
        for track in 0..TRACKS {
            let lane = areas.lane(track, false);
            painter.rect_filled(lane, 0.0, if track % 2 == 0 { palette::LANE } else { palette::LANE_ALT });
            painter.line_segment([lane.left_bottom(), lane.right_bottom()], Stroke::new(1.0, palette::PANEL_DARK));
            let name_rect = areas.lane(track, true);
            painter.rect_filled(name_rect, 0.0, palette::PANEL);
            // With room for volume and pan, number and name move up.
            let up = if areas.lane_h >= CONTROLS_MIN { -areas.lane_h * 0.18 } else { 0.0 };
            painter.text(
                name_rect.left_center() + vec2(8.0, up),
                Align2::LEFT_CENTER,
                format!("{}", track + 1),
                FontId::proportional(14.0),
                palette::CREAM,
            );
            let name = &self.tracks[track].name;
            painter.with_clip_rect(name_rect.shrink(2.0)).text(
                name_rect.left_center() + vec2(28.0, up),
                Align2::LEFT_CENTER,
                if name.is_empty() { "leeg" } else { name },
                FontId::proportional(12.0),
                palette::TEXT,
            );
            if let Some(unresolved) = &self.tracks[track].unresolved {
                let loading = self.source_is(unresolved.source, SourceState::Loading);
                let text = if loading { "laden...".to_string() } else { format!("ontbreekt: {}", unresolved.name) };
                painter.text(lane.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, text, FontId::proportional(11.0), palette::ORANGE);
            }
            let lane_painter = painter.with_clip_rect(lane);
            for (index, region) in self.tracks[track].regions.iter().enumerate() {
                let picked = self.editor.selection.contains(&(track, index));
                let (shown, alpha) = match self.editor.drag {
                    Some(Drag::Move { .. }) if picked => (region.clone(), 0x60),
                    Some(Drag::Trim { track: t, index: i, edge, at }) if (t, i) == (track, index) => (trimmed(region, edge, at), 0xff),
                    _ => (region.clone(), 0xff),
                };
                self.paint_region(&lane_painter, line, lane, &shown, picked, alpha);
            }
            if hovering && track == self.editor.target {
                painter.rect_stroke(lane, 0.0, Stroke::new(2.0, palette::CREAM), egui::StrokeKind::Inside);
            }
        }
        // The moved selection where it would land.
        if let Some(Drag::Move { time_delta, track_delta, .. }) = self.editor.drag {
            for &(t, i) in &self.editor.selection {
                let Some(region) = self.tracks.get(t).and_then(|track| track.regions.get(i)) else { continue };
                let target = (t as isize + track_delta).clamp(0, TRACKS as isize - 1) as usize;
                let lane = areas.lane(target, false);
                self.paint_region(&painter.with_clip_rect(lane), line, lane, &region.shifted(time_delta), true, 0xc0);
            }
        }
        let lanes_and_ruler = Rect::from_min_max(areas.ruler.min, areas.lanes.max);
        if let Some((from, to)) = self.editor.range {
            let shade = Rect::from_min_max(pos2(line.x(from), lanes_and_ruler.top()), pos2(line.x(to), lanes_and_ruler.bottom()));
            painter.with_clip_rect(lanes_and_ruler).rect_filled(shade, 0.0, palette::RANGE);
        }
        let x = line.x(self.playhead());
        if lanes_and_ruler.x_range().contains(x) {
            painter.line_segment([pos2(x, lanes_and_ruler.top()), pos2(x, lanes_and_ruler.bottom())], Stroke::new(2.0, palette::LED));
        }
    }

    /// Bars and beats with RASTER, else minutes and seconds; the zero and CUE points.
    fn paint_ruler(&self, painter: &egui::Painter, areas: &Areas, line: &Timeline) {
        let ruler = areas.ruler;
        painter.rect_filled(ruler, 0.0, palette::PANEL_DARK);
        let font = FontId::monospace(9.0);
        let (from, to) = (line.start, line.seconds(ruler.right()));
        let tick = |x: f32, length: f32| {
            painter.line_segment([pos2(x, ruler.bottom() - length), pos2(x, ruler.bottom())], Stroke::new(1.0, palette::TEXT_DIM));
        };
        if self.editor.settings.snap {
            let m = &self.metronome;
            let (beat, bar) = timeline::beat_and_bar(m.bpm, m.beats, m.unit);
            let every = [1u32, 2, 4, 8, 16, 32, 64].into_iter().find(|n| f64::from(*n) * bar * line.zoom >= 28.0).unwrap_or(128);
            for (t, bar_number) in timeline::beats(from, to, beat, m.beats) {
                let x = line.x(t);
                match bar_number {
                    Some(n) if (n - 1) % every == 0 => {
                        tick(x, 10.0);
                        painter.text(pos2(x + 3.0, ruler.top() + 2.0), Align2::LEFT_TOP, n.to_string(), font.clone(), palette::TEXT_DIM);
                    }
                    Some(_) => tick(x, 6.0),
                    None if beat * line.zoom >= 6.0 => tick(x, 3.0),
                    None => {}
                }
            }
        } else {
            let step = timeline::seconds_step(line.zoom, 60.0);
            let mut t = (from / step).floor() * step;
            while t <= to {
                let x = line.x(t);
                tick(x, 8.0);
                painter.text(pos2(x + 3.0, ruler.top() + 2.0), Align2::LEFT_TOP, format_counter(t), font.clone(), palette::TEXT_DIM);
                t += step;
            }
        }
        let marker = |t: f64, label: &str, colour: Color32| {
            let at = pos2(line.x(t), ruler.bottom());
            let galley = painter.layout_no_wrap(label.to_string(), FontId::proportional(9.0), palette::REGION_INK);
            let rect = Rect::from_min_size(at - vec2(0.0, galley.size().y + 2.0), galley.size() + vec2(6.0, 2.0));
            painter.rect_filled(rect, 1.0, colour);
            painter.galley(rect.min + vec2(3.0, 1.0), galley, palette::REGION_INK);
        };
        marker(self.zero_seconds, "0", palette::CREAM);
        if let Some(cue) = self.cue_seconds {
            marker(cue, "CUE", palette::ORANGE);
        }
    }

    /// One region: its body, its waveform and its name.
    fn paint_region(&self, painter: &egui::Painter, line: &Timeline, lane: Rect, region: &Region, picked: bool, alpha: u8) {
        let (x0, x1) = (line.x(region.start).max(lane.left() - 4.0), line.x(region.end()).min(lane.right() + 4.0));
        if x1 <= x0 {
            return;
        }
        let body = Rect::from_min_max(pos2(x0, lane.top() + 2.0), pos2(x1, lane.bottom() - 2.0));
        let source = self.sources.get(region.source);
        let missing = source.is_some_and(|s| s.state == SourceState::Missing);
        let fill = if missing {
            palette::RED
        } else if picked {
            palette::REGION_PICKED
        } else {
            palette::REGION
        };
        let fade = f32::from(alpha) / 255.0;
        painter.rect_filled(body, 2.0, fill.gamma_multiply(fade));
        if let Some(peaks) = source.and_then(|s| s.peaks.as_ref()) {
            let (mid, half) = (body.center().y, body.height() / 2.0 - 2.0);
            let ink = Color32::from_rgba_unmultiplied(0x1b, 0x17, 0x16, 0x99).gamma_multiply(fade);
            let clip = painter.clip_rect();
            let mut x = body.left().max(clip.left()).floor();
            while x < body.right().min(clip.right()) {
                let source_at = |x: f32| line.seconds(x) - region.start + region.offset;
                let (lo, hi) = peaks.range(source_at(x), source_at(x + 1.0));
                painter.line_segment([pos2(x + 0.5, mid - hi * half), pos2(x + 0.5, mid - lo * half)], Stroke::new(1.0, ink));
                x += 1.0;
            }
        }
        if body.width() > 30.0 {
            let name = if missing { "ontbreekt" } else { region.name.as_str() };
            painter.with_clip_rect(body.intersect(painter.clip_rect())).text(
                body.left_top() + vec2(4.0, 1.0),
                Align2::LEFT_TOP,
                name,
                FontId::proportional(9.0),
                palette::REGION_INK,
            );
        }
        if picked {
            painter.rect_stroke(body, 2.0, Stroke::new(2.0, palette::LED), egui::StrokeKind::Inside);
        }
    }

    /// The field for a track's new name, while one is being edited.
    fn rename_field(&mut self, ui: &mut Ui, areas: &Areas) {
        let Some((track, mut text, focus)) = self.editor.renaming.take() else { return };
        let rect = areas.lane(track, true).shrink2(vec2(24.0, (areas.lane_h - 20.0).max(0.0) / 2.0));
        let field = ui.put(rect, egui::TextEdit::singleline(&mut text).font(FontId::proportional(12.0)));
        if focus {
            field.request_focus();
        }
        // An egui text field also loses its focus on Escape: look at that first.
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            return;
        }
        if field.lost_focus() {
            self.rename(track, text);
        } else {
            self.editor.renaming = Some((track, text, false));
        }
    }

    /// S, Delete, Escape and cut, copy and paste while the drawer is open.
    pub(crate) fn handle_drawer_keys(&mut self, ui: &Ui) {
        if !self.editor.settings.open || ui.memory(|m| m.focused().is_some()) {
            return;
        }
        // egui-winit turns Cmd+X, Cmd+C and Cmd+V into these events, not into keys.
        let (split, delete, escape, cut, copy, paste) = ui.input(|i| {
            let plain = i.modifiers.is_none();
            let has = |f: fn(&egui::Event) -> bool| i.events.iter().any(f);
            (
                plain && i.key_pressed(Key::S),
                plain && (i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)),
                i.key_pressed(Key::Escape),
                has(|e| matches!(e, egui::Event::Cut)),
                has(|e| matches!(e, egui::Event::Copy)),
                has(|e| matches!(e, egui::Event::Paste(_) | egui::Event::PasteImage(_))),
            )
        });
        if split {
            self.split();
        }
        if delete {
            self.delete();
        }
        if escape {
            self.clear_selection();
        }
        if cut {
            self.cut();
        }
        if copy {
            self.copy();
        }
        if cut || copy {
            // Cmd+V only arrives as a paste when the system clipboard holds text.
            ui.ctx().copy_text("DAW 388".into());
        }
        if paste {
            self.paste();
        }
    }
}

/// How a region looks while one edge is pulled to `at` (the source's limits apply on release).
fn trimmed(region: &Region, edge: Edge, at: f64) -> Region {
    match edge {
        Edge::Start => {
            let start = at.max(region.start - region.offset).max(0.0).min(region.end() - daw388_project::regions::MIN_LENGTH);
            Region { start, offset: region.offset + (start - region.start), length: region.end() - start, ..region.clone() }
        }
        Edge::End => Region { length: (at - region.start).max(daw388_project::regions::MIN_LENGTH), ..region.clone() },
    }
}

/// A small horizontal fader on the mixer's scale: drag sideways (Shift for fine), double-click for 0 dB.
fn mini_fader(ui: &mut Ui, db: &mut f32) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(120.0, 14.0), Sense::click_and_drag());
    if response.dragged() {
        let fine = if ui.input(|i| i.modifiers.shift) { 0.2 } else { 1.0 };
        let position = (fader_position(*db) + response.drag_delta().x / rect.width() * fine).clamp(0.0, 1.0);
        *db = fader_db(position);
        response.mark_changed();
    }
    if response.double_clicked() {
        *db = 0.0;
        response.mark_changed();
    }
    let painter = ui.painter();
    let groove = Rect::from_center_size(rect.center(), vec2(rect.width() - 10.0, 3.0));
    painter.rect_filled(groove, 1.0, Color32::BLACK);
    let unity = groove.left() + fader_position(0.0) * groove.width();
    painter.line_segment([pos2(unity, rect.top() + 1.0), pos2(unity, rect.bottom() - 1.0)], Stroke::new(1.0, palette::TEXT_DIM));
    let x = groove.left() + fader_position(*db) * groove.width();
    let cap = Rect::from_center_size(pos2(x, rect.center().y), vec2(10.0, rect.height()));
    painter.rect_filled(cap, 2.0, palette::CREAM);
    painter.line_segment([pos2(x, cap.top() + 2.0), pos2(x, cap.bottom() - 2.0)], Stroke::new(1.0, palette::EDGE));
    response
}
