//! The project's sources: every audio file the regions play, decoded once and shared.

use daw388_project::MonoClip;
use daw388_project::regions::SourceId;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceState {
    Loading,
    Ready,
    /// The file could not be read; regions that use it are silent.
    Missing,
}

pub(crate) struct Source {
    pub path: PathBuf,
    pub state: SourceState,
    pub clip: Option<Arc<MonoClip>>,
    /// The samples at the engine's rate, and that rate.
    pub at_rate: Option<(u32, Arc<Vec<f32>>)>,
    /// Identifies the latest decode or resample, so a late result is ignored.
    pub generation: u64,
}

#[derive(Default)]
pub(crate) struct Sources {
    map: HashMap<SourceId, Source>,
    next: SourceId,
}

impl Sources {
    /// The source with this path, if there is one.
    pub fn find(&self, path: &Path) -> Option<SourceId> {
        self.map.iter().find(|(_, s)| s.path == path).map(|(id, _)| *id)
    }

    /// A new source; ids are never reused, not even after [`Self::clear`].
    pub fn add(&mut self, path: PathBuf, state: SourceState) -> SourceId {
        let id = self.next;
        self.next += 1;
        self.map.insert(id, Source { path, state, clip: None, at_rate: None, generation: 0 });
        id
    }

    pub fn get(&self, id: SourceId) -> Option<&Source> {
        self.map.get(&id)
    }

    pub fn get_mut(&mut self, id: SourceId) -> Option<&mut Source> {
        self.map.get_mut(&id)
    }

    /// Every source, for a device change.
    pub fn ids(&self) -> Vec<SourceId> {
        self.map.keys().copied().collect()
    }

    /// Where a source's file is.
    pub fn path(&self, id: SourceId) -> Option<&Path> {
        self.map.get(&id).map(|s| s.path.as_path())
    }

    /// Forget a source nothing uses any more.
    pub fn remove(&mut self, id: SourceId) {
        self.map.remove(&id);
    }

    /// After saving: every path that moved into the project gets its new place.
    pub fn rename(&mut self, moved: &HashMap<PathBuf, PathBuf>) {
        for source in self.map.values_mut() {
            if let Some(to) = moved.get(&source.path) {
                source.path.clone_from(to);
            }
        }
    }

    /// Forget every source (a new or other project).
    pub fn clear(&mut self) {
        self.map.clear();
    }
}
