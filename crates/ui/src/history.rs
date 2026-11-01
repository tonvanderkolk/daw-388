//! Undo and redo for what is on the tracks: recordings, imports, Wis and (from 0.18) edits.
//! A step keeps the tracks it touched as they were; sources are shared, so a step is cheap.

use daw388_project::regions::{Region, SourceId, Unresolved, fit_to_source};

/// How many steps can be undone.
pub(crate) const STEPS: usize = 50;

/// What a track holds.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TrackContent {
    pub regions: Vec<Region>,
    pub unresolved: Option<Unresolved>,
    pub name: String,
}

/// One undoable change: its name and the tracks it touched, as they were on the other side of it.
#[derive(Debug, Clone)]
pub(crate) struct Step {
    pub name: String,
    pub tracks: Vec<(usize, TrackContent)>,
    /// Steps of one group, such as files dropped together, are one step.
    group: Option<u64>,
}

#[derive(Debug, Default)]
pub(crate) struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
}

impl History {
    /// Remember a change; `tracks` holds the touched tracks as they were before it.
    pub fn push(&mut self, name: impl Into<String>, tracks: Vec<(usize, TrackContent)>) {
        self.undo.push(Step { name: name.into(), tracks, group: None });
        if self.undo.len() > STEPS {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Like [`Self::push`], but a change in the same `group` as the last step joins that step
    /// (files dropped together arrive one by one, and are still one step).
    pub fn push_grouped(&mut self, name: impl Into<String>, group: u64, tracks: Vec<(usize, TrackContent)>) {
        if let Some(last) = self.undo.last_mut().filter(|s| s.group == Some(group)) {
            for (track, content) in tracks {
                if !last.tracks.iter().any(|(t, _)| *t == track) {
                    last.tracks.push((track, content));
                }
            }
            self.redo.clear();
            return;
        }
        self.push(name, tracks);
        if let Some(last) = self.undo.last_mut() {
            last.group = Some(group);
        }
    }

    /// Take back the last change. `current` gives a track's content now, which redo will need.
    /// Returns the step's name and what to put on the tracks.
    pub fn undo(&mut self, current: impl Fn(usize) -> TrackContent) -> Option<Step> {
        let step = self.undo.pop()?;
        let tracks = step.tracks.iter().map(|(t, _)| (*t, current(*t))).collect();
        self.redo.push(Step { name: step.name.clone(), tracks, group: step.group });
        Some(step)
    }

    /// Do the last undone change again.
    pub fn redo(&mut self, current: impl Fn(usize) -> TrackContent) -> Option<Step> {
        let step = self.redo.pop()?;
        let tracks = step.tracks.iter().map(|(t, _)| (*t, current(*t))).collect();
        self.undo.push(Step { name: step.name.clone(), tracks, group: step.group });
        Some(step)
    }

    /// The name of the step Cmd+Z would take back.
    pub fn undo_name(&self) -> Option<&str> {
        self.undo.last().map(|s| s.name.as_str())
    }

    /// The name of the step Cmd+Shift+Z would do again.
    pub fn redo_name(&self) -> Option<&str> {
        self.redo.last().map(|s| s.name.as_str())
    }

    /// How many steps can be undone now.
    pub fn len(&self) -> usize {
        self.undo.len()
    }

    /// Forget every step (a new or other project).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Whether a stored step still refers to `source`, so it must not be forgotten.
    pub fn uses(&self, source: SourceId) -> bool {
        self.undo.iter().chain(&self.redo).flat_map(|s| &s.tracks).any(|(_, c)| c.uses(source))
    }

    /// A source has loaded and is `duration` seconds long: every unresolved reference to it,
    /// also in the stored steps, becomes a region (its length is a fact about the file).
    pub fn resolve(&mut self, source: SourceId, duration: f64) {
        for step in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            for (_, content) in &mut step.tracks {
                content.resolve(source, duration);
            }
        }
    }
}

impl TrackContent {
    /// Whether a region or the unresolved reference plays `source`.
    pub fn uses(&self, source: SourceId) -> bool {
        self.regions.iter().any(|r| r.source == source) || self.unresolved.as_ref().is_some_and(|u| u.source == source)
    }

    /// Turn an unresolved reference to `source` into a region and fit the regions of `source`
    /// to its real length.
    pub fn resolve(&mut self, source: SourceId, duration: f64) {
        if let Some(unresolved) = self.unresolved.take_if(|u| u.source == source) {
            self.regions = vec![Region { source, start: 0.0, offset: 0.0, length: duration, name: unresolved.name }];
        }
        self.regions = fit_to_source(&self.regions, source, duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(source: SourceId, length: f64) -> TrackContent {
        TrackContent {
            regions: vec![Region { source, start: 0.0, offset: 0.0, length, name: "Kick".into() }],
            unresolved: None,
            name: "Kick".into(),
        }
    }

    #[test]
    fn at_most_steps() {
        let mut history = History::default();
        for n in 0..STEPS + 2 {
            history.push(format!("step {n}"), vec![(0, TrackContent::default())]);
        }
        assert_eq!(history.len(), STEPS);
        for _ in 0..STEPS {
            assert!(history.undo(|_| TrackContent::default()).is_some());
        }
        assert!(history.undo(|_| TrackContent::default()).is_none());
    }

    #[test]
    fn undo_and_redo_swap_the_content() {
        let (a, b) = (content(1, 2.0), TrackContent::default());
        let mut history = History::default();
        history.push("Clear", vec![(0, a.clone())]);
        assert_eq!(history.undo_name(), Some("Clear"));
        let step = history.undo(|_| b.clone()).unwrap_or_else(|| panic!("a step"));
        assert_eq!(step.tracks, vec![(0, a.clone())]);
        assert_eq!((history.undo_name(), history.redo_name()), (None, Some("Clear")));
        let step = history.redo(|_| a.clone()).unwrap_or_else(|| panic!("a step"));
        assert_eq!(step.tracks, vec![(0, b)]);
        assert_eq!(history.undo_name(), Some("Clear"));
    }

    #[test]
    fn a_new_step_drops_redo() {
        let mut history = History::default();
        history.push("Clear", vec![(0, TrackContent::default())]);
        let _ = history.undo(|_| TrackContent::default());
        history.push("Import", vec![(0, TrackContent::default())]);
        assert_eq!(history.redo_name(), None);
    }

    #[test]
    fn a_group_is_one_step() {
        let (a, b, c) = (content(1, 1.0), content(2, 1.0), content(3, 1.0));
        let mut history = History::default();
        history.push_grouped("Import", 7, vec![(0, a.clone())]);
        history.push_grouped("Import", 7, vec![(1, b.clone()), (0, c)]);
        assert_eq!(history.len(), 1);
        history.push_grouped("Import", 8, vec![(2, b.clone())]);
        assert_eq!(history.len(), 2);
        let _ = history.undo(|_| TrackContent::default());
        let step = history.undo(|_| TrackContent::default()).unwrap_or_else(|| panic!("a step"));
        assert_eq!(step.tracks, vec![(0, a), (1, b)]);
    }

    #[test]
    fn uses_sees_stored_steps() {
        let mut history = History::default();
        history.push("Clear", vec![(2, content(3, 1.0))]);
        assert!(history.uses(3) && !history.uses(4));
    }

    #[test]
    fn resolving_turns_a_reference_into_a_region_also_in_steps() {
        let mut history = History::default();
        let waiting = TrackContent { unresolved: Some(Unresolved { source: 4, name: "Kick".into() }), ..TrackContent::default() };
        history.push("Load", vec![(0, waiting)]);
        history.resolve(4, 12.5);
        let step = history.undo(|_| TrackContent::default()).unwrap_or_else(|| panic!("a step"));
        let resolved = &step.tracks[0].1;
        assert!(resolved.unresolved.is_none());
        assert_eq!(resolved.regions.len(), 1);
        assert_eq!(resolved.regions[0].length, 12.5);
    }
}
