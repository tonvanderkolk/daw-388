//! Edits from the drawer: each one is a single undo step built from the pure functions in
//! `regions.rs`.

use super::*;
use crate::sources::SourceState;
use crate::timeline::Edge;
use daw388_project::regions::{self, Clipboard};

impl DawApp {
    /// Whether editing is possible now; says why not.
    fn may_edit(&mut self) -> bool {
        if self.recording_now() {
            self.status = "Bewerken kan niet tijdens een opname".into();
            return false;
        }
        true
    }

    /// Whether `tracks` can take new regions (a track waiting for a file cannot); says why not.
    fn tracks_free(&mut self, tracks: impl IntoIterator<Item = usize>) -> bool {
        tracks.into_iter().all(|t| !self.refuse_unresolved(t))
    }

    fn all_regions(&self) -> Vec<Vec<Region>> {
        self.tracks.iter().map(|t| t.regions.clone()).collect()
    }

    /// Put new region lists on the tracks as one step (only tracks that changed are in it).
    fn set_regions(&mut self, name: &str, new: Vec<Vec<Region>>) {
        let changed: Vec<usize> = (0..TRACKS).filter(|&t| self.tracks[t].regions != new[t]).collect();
        if changed.is_empty() {
            return;
        }
        self.change(name, &changed, |app| {
            for &t in &changed {
                app.tracks[t].regions.clone_from(&new[t]);
            }
        });
    }

    /// Select these regions again after an edit (they are found by their values).
    fn select(&mut self, placed: &[(usize, Region)]) {
        self.editor.selection =
            placed.iter().filter_map(|(t, r)| self.tracks.get(*t)?.regions.iter().position(|x| x == r).map(|i| (*t, i))).collect();
    }

    pub(crate) fn playhead(&self) -> f64 {
        self.engine.as_ref().map_or(0.0, EngineHandle::position_seconds).max(0.0)
    }

    /// The selection, highest index first, so removing them one by one keeps the rest valid.
    fn selection_by_track(&self) -> Vec<(usize, usize)> {
        let mut selection = self.editor.selection.clone();
        selection.sort_unstable_by(|a, b| b.cmp(a));
        selection.dedup();
        selection.retain(|(t, i)| self.tracks.get(*t).is_some_and(|track| *i < track.regions.len()));
        selection
    }

    /// The selected regions with their track.
    fn selected_regions(&self) -> Vec<(usize, Region)> {
        self.selection_by_track().into_iter().map(|(t, i)| (t, self.tracks[t].regions[i].clone())).collect()
    }

    /// The tracks without the selected regions.
    fn without_selection(&self) -> Vec<Vec<Region>> {
        let mut tracks = self.all_regions();
        for (t, i) in self.selection_by_track() {
            tracks[t] = regions::remove(&tracks[t], i);
        }
        tracks
    }

    /// SPLITS: the selected regions at the playhead, or without a selection every track.
    pub(crate) fn split(&mut self) {
        if !self.may_edit() {
            return;
        }
        let at = self.playhead();
        let mut tracks = self.all_regions();
        if self.editor.selection.is_empty() {
            tracks = tracks.iter().map(|t| regions::split(t, at)).collect();
        } else {
            for (t, i) in self.selection_by_track() {
                tracks[t] = regions::split_at(&tracks[t], i, at);
            }
        }
        self.set_regions("Splitsen", tracks);
    }

    /// KOPIEER: the stretch over all tracks, or the selected regions.
    pub(crate) fn copy(&mut self) {
        let tracks = self.all_regions();
        let clipboard = match self.editor.range {
            Some((from, to)) => regions::copy_range(&tracks, from, to),
            None if !self.editor.selection.is_empty() => regions::copy_regions(&tracks, &self.selection_by_track()),
            None => return,
        };
        self.editor.clipboard = Some(clipboard);
        self.status = "Gekopieerd".into();
    }

    /// KNIP: a stretch over all tracks is a splice (everything after it moves forward);
    /// selected regions leave a gap.
    pub(crate) fn cut(&mut self) {
        if !self.may_edit() {
            return;
        }
        if let Some((from, to)) = self.editor.range {
            if !self.tracks_free(0..TRACKS) {
                return;
            }
            let (tracks, clipboard) = regions::cut_range(&self.all_regions(), from, to);
            self.editor.clipboard = Some(clipboard);
            self.editor.range = None;
            self.set_regions("Splice", tracks);
        } else if !self.editor.selection.is_empty() {
            self.editor.clipboard = Some(regions::copy_regions(&self.all_regions(), &self.selection_by_track()));
            let tracks = self.without_selection();
            self.set_regions("Knippen", tracks);
        }
    }

    /// PLAK at the playhead: regions on their own track (replacing what they land on), a
    /// stretch inserted over all tracks.
    pub(crate) fn paste(&mut self) {
        if !self.may_edit() {
            return;
        }
        let at = self.playhead();
        match self.editor.clipboard.clone() {
            Some(Clipboard::Regions(items)) => {
                if !self.tracks_free(items.iter().map(|(t, _)| *t)) {
                    return;
                }
                let tracks = regions::paste_regions(&self.all_regions(), at, &Clipboard::Regions(items.clone()));
                self.set_regions("Plakken", tracks);
                let placed: Vec<(usize, Region)> = items.into_iter().map(|(t, r)| (t, r.shifted(at))).collect();
                self.select(&placed);
            }
            Some(range @ Clipboard::Range { .. }) => {
                if !self.tracks_free(0..TRACKS) {
                    return;
                }
                let tracks = regions::insert_range(&self.all_regions(), at, &range);
                self.set_regions("Invoegen", tracks);
            }
            None => self.status = "Er is niets om te plakken".into(),
        }
    }

    /// WIS: the stretch over all tracks without closing the gap, or the selected regions.
    pub(crate) fn delete(&mut self) {
        if !self.may_edit() {
            return;
        }
        if let Some((from, to)) = self.editor.range {
            if !self.tracks_free(0..TRACKS) {
                return;
            }
            let tracks = regions::clear_range(&self.all_regions(), from, to);
            self.editor.range = None;
            self.set_regions("Wissen", tracks);
        } else if !self.editor.selection.is_empty() {
            let tracks = self.without_selection();
            self.set_regions("Verwijderen", tracks);
        }
    }

    /// Move the selection by whole tracks and by time; it stays selected.
    pub(crate) fn move_selection(&mut self, track_delta: isize, time_delta: f64) {
        if !self.may_edit() {
            return;
        }
        let picked = self.selected_regions();
        if picked.is_empty() {
            return;
        }
        // The same limits as regions::move_regions, so the moved regions can be found again.
        let lowest = picked.iter().map(|(t, _)| *t as isize).min().unwrap_or(0);
        let highest = picked.iter().map(|(t, _)| *t as isize).max().unwrap_or(0);
        let track_delta = track_delta.clamp(-lowest, TRACKS as isize - 1 - highest);
        let earliest = picked.iter().map(|(_, r)| r.start).fold(f64::INFINITY, f64::min);
        let time_delta = time_delta.max(-earliest);
        let target = |t: usize| (t as isize + track_delta) as usize;
        if !self.tracks_free(picked.iter().map(|(t, _)| target(*t))) {
            return;
        }
        let tracks = regions::move_regions(&self.all_regions(), &self.selection_by_track(), track_delta, time_delta);
        self.set_regions("Verschuiven", tracks);
        let moved: Vec<(usize, Region)> = picked.into_iter().map(|(t, r)| (target(t), r.shifted(time_delta))).collect();
        self.select(&moved);
    }

    /// Pull one edge of a region to `at`, no further than its source; it stays selected.
    pub(crate) fn trim(&mut self, track: usize, index: usize, edge: Edge, at: f64) {
        if !self.may_edit() {
            return;
        }
        let Some(region) = self.tracks.get(track).and_then(|t| t.regions.get(index)).cloned() else { return };
        let old = &self.tracks[track].regions;
        let new = match edge {
            Edge::Start => regions::trim_start(old, index, at),
            Edge::End => regions::trim_end(old, index, at, self.source_length(region.source)),
        };
        // The trimmed region: same source, and the edge that did not move.
        let trimmed = new.iter().find(|r| {
            r.source == region.source
                && match edge {
                    Edge::Start => (r.end() - region.end()).abs() < 1e-9,
                    Edge::End => (r.start - region.start).abs() < 1e-9 && (r.offset - region.offset).abs() < 1e-9,
                }
        });
        let trimmed = trimmed.cloned();
        let mut tracks = self.all_regions();
        tracks[track] = new;
        self.set_regions("Inkorten", tracks);
        if let Some(trimmed) = trimmed {
            self.select(&[(track, trimmed)]);
        }
    }

    /// A new name for a track.
    pub(crate) fn rename(&mut self, track: usize, name: String) {
        let name = name.trim().to_string();
        if !self.may_edit() || self.tracks[track].name == name {
            return;
        }
        self.change("Naam", &[track], |app| app.tracks[track].name = name);
    }

    /// Audio files dropped on the drawer: one per track from `track`, at `at`, one undo step.
    pub(crate) fn drop_files(&mut self, track: usize, at: f64, paths: Vec<PathBuf>) {
        if !self.may_edit() {
            return;
        }
        let audio =
            |p: &PathBuf| p.extension().is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|x| e.to_string_lossy().eq_ignore_ascii_case(x)));
        let group = self.next_generation();
        let mut skipped = paths.len();
        let mut placements = Vec::new();
        for (k, path) in paths.iter().filter(|p| audio(p)).enumerate() {
            let target = track + k;
            if target >= TRACKS || self.refuse_unresolved(target) {
                continue;
            }
            placements.push((target, path.clone()));
            skipped -= 1;
        }
        let note = format!("{skipped} bestand(en) niet geplaatst");
        self.editor.drop_note = (skipped > 0).then_some((group, note.clone()));
        if placements.is_empty() {
            self.status = note;
            return;
        }
        for (target, path) in placements {
            let source = self.source_for(&path);
            let import = PendingImport { source, track: target, start: at.max(0.0), group: Some(group) };
            if self.source_is(source, SourceState::Ready) {
                self.place_import(import);
            } else {
                self.pending_imports.push(import);
            }
        }
    }

    /// Select one region; with `add`, add it to the selection or take it out.
    pub(crate) fn select_region(&mut self, track: usize, index: usize, add: bool) {
        self.editor.range = None;
        if !add {
            self.editor.selection = vec![(track, index)];
        } else if let Some(at) = self.editor.selection.iter().position(|s| *s == (track, index)) {
            self.editor.selection.remove(at);
        } else {
            self.editor.selection.push((track, index));
        }
    }

    /// Select a stretch over all tracks (none when it is too short).
    pub(crate) fn select_range(&mut self, from: f64, to: f64) {
        self.editor.selection.clear();
        let (from, to) = (from.min(to).max(0.0), from.max(to).max(0.0));
        self.editor.range = (to - from >= regions::MIN_LENGTH).then_some((from, to));
    }

    /// Nothing selected.
    pub(crate) fn clear_selection(&mut self) {
        self.editor.selection.clear();
        self.editor.range = None;
    }
}
