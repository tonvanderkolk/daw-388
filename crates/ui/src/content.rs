//! What is on the tracks: sources loading in the background, tracks rendered for the engine,
//! imports, "8 audiobestanden laden", Wis, recorded takes, and undo and redo.

use super::*;
use crate::history::TrackContent;
use crate::recording::{StoredTake, store_take};
use crate::sources::SourceState;
use daw388_project::regions::{Unresolved, place};
use daw388_project::render::{RateSources, render_track};

/// A file's name without its extension: the name of a region or a track.
fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

impl DawApp {
    pub(crate) fn next_generation(&mut self) -> u64 {
        let generation = self.next_generation;
        self.next_generation += 1;
        generation
    }

    /// What a track holds now.
    pub(crate) fn content(&self, track: usize) -> TrackContent {
        let t = &self.tracks[track];
        TrackContent { regions: t.regions.clone(), unresolved: t.unresolved.clone(), name: t.name.clone() }
    }

    /// Put `content` on a track (no undo step) and render it.
    fn set_content(&mut self, track: usize, content: TrackContent) {
        if content != self.content(track) {
            // Indices change: the selection and a drag no longer mean anything.
            self.editor.selection.clear();
            self.editor.drag = None;
        }
        let t = &mut self.tracks[track];
        t.regions = content.regions;
        t.unresolved = content.unresolved;
        t.name = content.name;
        self.request_render(track);
    }

    /// An undoable change to what is on `tracks`: `edit` changes them, the step keeps them as
    /// they were.
    pub(crate) fn change(&mut self, name: &str, tracks: &[usize], edit: impl FnOnce(&mut Self)) {
        self.change_in(name, None, tracks, edit);
    }

    /// Like [`Self::change`], joining the last step when it has the same `group`.
    fn change_in(&mut self, name: &str, group: Option<u64>, tracks: &[usize], edit: impl FnOnce(&mut Self)) {
        let before = tracks.iter().map(|&t| (t, self.content(t))).collect();
        edit(self);
        self.editor.selection.clear();
        self.editor.drag = None;
        match group {
            Some(group) => self.history.push_grouped(name, group, before),
            None => self.history.push(name, before),
        }
        for &track in tracks {
            self.request_render(track);
        }
        self.mark_dirty();
    }

    /// The sources a track plays (or waits for).
    fn track_sources(&self, track: usize) -> Vec<SourceId> {
        let t = &self.tracks[track];
        let mut ids: Vec<SourceId> = t.regions.iter().map(|r| r.source).chain(t.unresolved.as_ref().map(|u| u.source)).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    pub(crate) fn source_is(&self, id: SourceId, state: SourceState) -> bool {
        self.sources.get(id).is_some_and(|s| s.state == state)
    }

    /// A source of this track is still being decoded.
    pub(crate) fn track_loading(&self, track: usize) -> bool {
        self.track_sources(track).into_iter().any(|id| self.source_is(id, SourceState::Loading))
    }

    /// The files of this track that could not be read.
    pub(crate) fn track_missing(&self, track: usize) -> Vec<PathBuf> {
        self.track_sources(track)
            .into_iter()
            .filter_map(|id| self.sources.get(id))
            .filter(|s| s.state == SourceState::Missing)
            .map(|s| s.path.clone())
            .collect()
    }

    /// Whether any source is still being decoded.
    pub fn is_loading(&self) -> bool {
        self.sources.ids().into_iter().any(|id| self.source_is(id, SourceState::Loading))
    }

    /// The source for a file: the one already known (loaded again when it was missing, the file
    /// may be back), or a new one that starts loading.
    pub(crate) fn source_for(&mut self, path: &Path) -> SourceId {
        match self.sources.find(path) {
            Some(id) => {
                if self.source_is(id, SourceState::Missing) {
                    self.request_source(id);
                }
                id
            }
            None => {
                let id = self.sources.add(path.to_path_buf(), SourceState::Loading);
                self.request_source(id);
                id
            }
        }
    }

    /// Decode a source in the background, also at the engine's rate; [`Self::poll_loads`] picks
    /// it up.
    fn request_source(&mut self, id: SourceId) {
        let generation = self.next_generation();
        let (rate, tx, ctx) = (self.engine_rate(), self.loads_tx.clone(), self.ctx.clone());
        let Some(source) = self.sources.get_mut(id) else { return };
        source.generation = generation;
        source.state = SourceState::Loading;
        let path = source.path.clone();
        self.platform.spawn(Box::new(move || {
            let outcome = daw388_project::load_audio(&path)
                .map(|clip| {
                    let at_rate = Arc::new(clip.at_rate(rate));
                    let peaks = Some(Arc::new(Peaks::of(&clip.samples, clip.sample_rate)));
                    Loaded { clip: Arc::new(clip), at_rate, peaks }
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(LoadResult { source: id, generation, outcome, rate });
            ctx.request_repaint();
        }));
    }

    /// Resample a loaded source for the engine's rate in the background.
    fn request_source_rate(&mut self, id: SourceId) {
        let generation = self.next_generation();
        let (rate, tx, ctx) = (self.engine_rate(), self.loads_tx.clone(), self.ctx.clone());
        let Some(source) = self.sources.get_mut(id) else { return };
        let (SourceState::Ready, Some(clip)) = (source.state, source.clip.clone()) else { return };
        source.generation = generation;
        self.platform.spawn(Box::new(move || {
            let at_rate = Arc::new(clip.at_rate(rate));
            let _ = tx.send(LoadResult { source: id, generation, outcome: Ok(Loaded { clip, at_rate, peaks: None }), rate });
            ctx.request_repaint();
        }));
    }

    /// After a device change: every source at the new rate, then every track rendered again.
    pub(crate) fn resample_all(&mut self) {
        for id in self.sources.ids() {
            self.request_source_rate(id);
        }
        for track in 0..TRACKS {
            self.request_render(track);
        }
    }

    /// Take in finished decodes and resamples.
    pub fn poll_loads(&mut self) {
        while let Ok(result) = self.loads_rx.try_recv() {
            let id = result.source;
            if self.sources.get(id).is_none_or(|s| s.generation != result.generation) {
                continue;
            }
            match result.outcome {
                Ok(loaded) => self.source_loaded(id, loaded, result.rate),
                Err(e) => self.source_failed(id, &e),
            }
        }
    }

    fn source_loaded(&mut self, id: SourceId, loaded: Loaded, rate: u32) {
        let engine_rate = self.engine_rate();
        let duration = loaded.clip.duration_seconds();
        if let Some(source) = self.sources.get_mut(id) {
            source.state = SourceState::Ready;
            source.clip = Some(loaded.clip);
            source.at_rate = (rate == engine_rate).then_some((rate, loaded.at_rate));
            if loaded.peaks.is_some() {
                source.peaks = loaded.peaks;
            }
        }
        if rate != engine_rate {
            // The device changed while this was loading.
            self.request_source_rate(id);
        }
        // Now the length is known: references become regions, also in the undo steps.
        for track in 0..TRACKS {
            let mut content = self.content(track);
            if content.uses(id) {
                content.resolve(id, duration);
                self.set_content(track, content);
            }
        }
        self.history.resolve(id, duration);
        let (now, later): (Vec<PendingImport>, Vec<PendingImport>) =
            std::mem::take(&mut self.pending_imports).into_iter().partition(|i| i.source == id);
        self.pending_imports = later;
        for import in now {
            self.place_import(import);
        }
    }

    fn source_failed(&mut self, id: SourceId, error: &str) {
        let path = self.sources.path(id).map(Path::to_path_buf).unwrap_or_default();
        let waiting = self.pending_imports.len();
        self.pending_imports.retain(|i| i.source != id);
        if self.pending_imports.len() != waiting {
            self.status = format!("{}: {error}", path.display());
        }
        let used = (0..TRACKS).any(|t| self.content(t).uses(id)) || self.history.uses(id);
        if !used {
            self.sources.remove(id);
            return;
        }
        log::warn!("{}: {error}", path.display());
        if let Some(source) = self.sources.get_mut(id) {
            source.state = SourceState::Missing;
            source.clip = None;
            source.at_rate = None;
        }
        let missing: Vec<String> = (0..TRACKS).filter(|&t| !self.track_missing(t).is_empty()).map(|t| (t + 1).to_string()).collect();
        if !missing.is_empty() {
            self.status = format!("{}: audio ontbreekt op spoor {} ({error})", self.project_name(), missing.join(", "));
        }
        for track in 0..TRACKS {
            if self.content(track).uses(id) {
                self.request_render(track);
            }
        }
    }

    /// Render a track's regions for the engine in the background, once all its sources are there
    /// at the engine's rate (a source that arrives later renders the track again).
    pub(crate) fn request_render(&mut self, track: usize) {
        let generation = self.next_generation();
        self.tracks[track].render_generation = generation;
        let rate = self.engine_rate();
        if self.tracks[track].regions.is_empty() {
            self.tracks[track].rendered = None;
            self.engine_send(Command::SetTrack(track, None));
            return;
        }
        let mut sources = RateSources::new();
        for id in self.track_sources(track) {
            match self.sources.get(id) {
                Some(s) if s.state == SourceState::Loading => return,
                Some(s) if s.state == SourceState::Ready => match &s.at_rate {
                    Some((at, samples)) if *at == rate => {
                        sources.insert(id, Arc::clone(samples));
                    }
                    _ => return,
                },
                // Missing: silent.
                _ => {}
            }
        }
        let (regions, name) = (self.tracks[track].regions.clone(), self.tracks[track].name.clone());
        let (tx, ctx) = (self.renders_tx.clone(), self.ctx.clone());
        self.platform.spawn(Box::new(move || {
            let audio = Arc::new(TrackAudio { name, samples: render_track(&regions, &sources, rate) });
            let _ = tx.send(RenderDone { track, generation, rate, audio });
            ctx.request_repaint();
        }));
    }

    /// Hand rendered tracks to the engine.
    pub fn poll_renders(&mut self) {
        while let Ok(done) = self.renders_rx.try_recv() {
            let current = self.tracks.get(done.track).is_some_and(|t| t.render_generation == done.generation);
            if !current || done.rate != self.engine_rate() {
                continue;
            }
            self.tracks[done.track].rendered = Some((done.rate, done.audio.samples.len()));
            self.engine_send(Command::SetTrack(done.track, Some(done.audio)));
        }
    }

    /// How long a source is: the file's length, or for a missing file the furthest any region
    /// reaches into it (trimming cannot go beyond what is known).
    pub(crate) fn source_length(&self, id: SourceId) -> f64 {
        if let Some(clip) = self.sources.get(id).and_then(|s| s.clip.as_ref()) {
            return clip.duration_seconds();
        }
        self.tracks.iter().flat_map(|t| &t.regions).filter(|r| r.source == id).map(|r| r.offset + r.length).fold(0.0, f64::max)
    }

    /// A track waiting for a file whose length is unknown takes nothing new; says why.
    pub(crate) fn refuse_unresolved(&mut self, index: usize) -> bool {
        let Some(unresolved) = &self.tracks[index].unresolved else { return false };
        self.status = if self.source_is(unresolved.source, SourceState::Loading) {
            format!("Spoor {} wordt nog geladen", index + 1)
        } else {
            format!("Spoor {}: audio ontbreekt, wis het spoor om erop op te nemen", index + 1)
        };
        true
    }

    /// "Laad": a file onto the track at the playhead.
    pub(crate) fn import(&mut self, index: usize) {
        if self.refuse_unresolved(index) {
            return;
        }
        if let Some(path) = self.platform.pick_audio_files(false).into_iter().next() {
            self.import_path(index, &path);
        }
    }

    /// Put `path` on a track at the playhead, once it has loaded.
    pub(crate) fn import_path(&mut self, index: usize, path: &Path) {
        if self.refuse_unresolved(index) {
            return;
        }
        let start = self.engine.as_ref().map_or(0.0, EngineHandle::position_seconds).max(0.0);
        let source = self.source_for(path);
        let import = PendingImport { source, track: index, start, group: None };
        if self.source_is(source, SourceState::Ready) {
            self.place_import(import);
        } else {
            self.pending_imports.push(import);
        }
    }

    pub(crate) fn place_import(&mut self, import: PendingImport) {
        let track = import.track;
        if self.refuse_unresolved(track) {
            return;
        }
        let Some(clip) = self.sources.get(import.source).and_then(|s| s.clip.clone()) else { return };
        let region =
            Region { source: import.source, start: import.start, offset: 0.0, length: clip.duration_seconds(), name: clip.name.clone() };
        self.change_in("Importeren", import.group, &[track], |app| {
            let t = &mut app.tracks[track];
            t.regions = place(&t.regions, region);
            if t.name.is_empty() {
                t.name.clone_from(&clip.name);
            }
        });
        self.status = format!("Spoor {}: {} geplaatst op {}", track + 1, clip.name, format_counter(import.start));
        if let Some((group, note)) = &self.editor.drop_note
            && import.group == Some(*group)
        {
            self.status = format!("{}; {note}", self.status);
        }
    }

    /// A track whose file has already loaded does not wait: its reference becomes a region now.
    pub(crate) fn resolve_loaded(&mut self, track: usize) {
        let Some(unresolved) = &self.tracks[track].unresolved else { return };
        let Some(duration) = self.sources.get(unresolved.source).and_then(|s| s.clip.as_ref()).map(|c| c.duration_seconds()) else {
            return;
        };
        let id = unresolved.source;
        let mut content = self.content(track);
        content.resolve(id, duration);
        self.set_content(track, content);
    }

    /// Put files onto tracks 1 to 8 in name order (for example `01_Kick.wav`, `02_Snare.wav`),
    /// each from 0:00, replacing what the tracks held: one undo step.
    pub fn load_paths(&mut self, mut paths: Vec<PathBuf>) {
        paths.sort();
        let chosen: Vec<PathBuf> = paths.iter().take(TRACKS).cloned().collect();
        if chosen.is_empty() {
            return;
        }
        let tracks: Vec<usize> = (0..chosen.len()).collect();
        self.change("Laden", &tracks, |app| {
            for (index, path) in chosen.iter().enumerate() {
                let source = app.source_for(path);
                let t = &mut app.tracks[index];
                t.regions.clear();
                t.name = stem(path);
                t.unresolved = Some(Unresolved { source, name: t.name.clone() });
            }
        });
        for track in tracks {
            self.resolve_loaded(track);
        }
        if paths.len() > TRACKS {
            self.status = format!("{} bestanden gekozen, alleen de eerste {TRACKS} zijn geladen", paths.len());
        }
    }

    /// Wis: the track empty.
    pub(crate) fn clear_track(&mut self, index: usize) {
        self.change("Wis", &[index], |app| {
            let t = &mut app.tracks[index];
            t.regions.clear();
            t.unresolved = None;
            t.name.clear();
        });
        self.pending_imports.retain(|i| i.track != index);
    }

    pub(crate) fn recording_now(&self) -> bool {
        self.engine.as_ref().is_some_and(|e| (0..TRACKS).any(|t| e.is_recording(t)))
    }

    /// Cmd+Z: take back the last change to the tracks.
    pub(crate) fn undo(&mut self) {
        self.step(false);
    }

    /// Cmd+Shift+Z: do the last undone change again.
    pub(crate) fn redo(&mut self) {
        self.step(true);
    }

    fn step(&mut self, again: bool) {
        if self.recording_now() {
            self.status = "Niet tijdens een opname".into();
            return;
        }
        let contents: Vec<TrackContent> = (0..TRACKS).map(|t| self.content(t)).collect();
        let step = if again { self.history.redo(|t| contents[t].clone()) } else { self.history.undo(|t| contents[t].clone()) };
        let Some(step) = step else {
            self.status = if again { "Er is niets om opnieuw te doen" } else { "Er is niets om ongedaan te maken" }.into();
            return;
        };
        for (track, content) in step.tracks {
            self.set_content(track, content);
        }
        self.status = format!("{} {}", step.name, if again { "opnieuw gedaan" } else { "ongedaan gemaakt" });
        self.mark_dirty();
    }

    /// Save the next queued take of a track in the background (one at a time per track, so takes
    /// are placed in the order they were made).
    pub(crate) fn start_compose(&mut self, track: usize) {
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
        let name = &self.tracks[track].name;
        let dest = daw388_project::take::take_path(&audio_dir, track + 1, (!name.is_empty()).then_some(name.as_str()));
        let start = take.start / f64::from(take.rate.max(1));
        let (project_generation, engine_rate) = (self.project_generation, self.engine_rate());
        let (tx, ctx) = (self.composed_tx.clone(), self.ctx.clone());
        self.composing[track] = true;
        self.platform.spawn(Box::new(move || {
            let outcome = store_take(&take, &dest, engine_rate);
            let _ = tx.send(ComposeResult { track, project_generation, start, outcome });
            ctx.request_repaint();
        }));
    }

    /// Put saved takes on their tracks.
    pub fn poll_composed(&mut self) {
        while let Ok(result) = self.composed_rx.try_recv() {
            let track = result.track;
            self.composing[track] = false;
            match result.outcome {
                Ok(stored) if result.project_generation != self.project_generation => {
                    self.status =
                        format!("Opname op spoor {} hoort bij een ander project; hij staat in {}", track + 1, stored.path.display());
                }
                Ok(stored) if self.tracks[track].unresolved.is_some() => {
                    self.status = format!("Spoor {} wordt nog geladen; de opname staat in {}", track + 1, stored.path.display());
                }
                Ok(stored) => self.place_take(track, result.start, stored),
                Err(e) => self.status = format!("Opname op spoor {} niet bewaard: {e}", track + 1),
            }
            self.start_compose(track);
        }
    }

    /// A saved take becomes a source and a region at its place on the tape (no earlier than
    /// 0:00), replacing what it lands on.
    fn place_take(&mut self, track: usize, start: f64, stored: StoredTake) {
        let seconds = stored.clip.duration_seconds();
        let name = stored.clip.name.clone();
        let id = self.sources.add(stored.path, SourceState::Ready);
        if let Some(source) = self.sources.get_mut(id) {
            source.clip = Some(stored.clip);
            source.at_rate = Some((stored.engine_rate, stored.at_rate));
            source.peaks = Some(stored.peaks);
        }
        if stored.engine_rate != self.engine_rate() {
            self.request_source_rate(id);
        }
        let offset = (-start).max(0.0);
        let region = Region { source: id, start: start.max(0.0), offset, length: seconds - offset, name: name.clone() };
        self.change(&format!("Opname spoor {}", track + 1), &[track], |app| {
            let t = &mut app.tracks[track];
            t.regions = place(&t.regions, region);
            if t.name.is_empty() {
                t.name = name;
            }
        });
        self.status = format!("Spoor {}: opname vastgelegd ({seconds:.1} s)", track + 1);
    }
}
