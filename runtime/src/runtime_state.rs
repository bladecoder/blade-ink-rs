//! Mutable counters used while playing a story.

#[allow(unused_imports)]
use crate::prelude::*;

use crate::{
    compat::collections::HashMap,
    path::Path,
    story_content::{ContainerId, StaticStoryView},
    story_error::StoryError,
};

/// Visit and turn counters use compact IDs during execution. Save codecs
/// translate these keys to canonical Ink paths at their boundary.
#[derive(Clone)]
pub(crate) struct StoryCounters {
    visits: HashMap<ContainerId, i32>,
    turns: HashMap<ContainerId, i32>,
}

impl StoryCounters {
    pub(crate) fn new() -> Self {
        Self {
            visits: HashMap::new(),
            turns: HashMap::new(),
        }
    }

    pub(crate) fn record_visit(&mut self, container: ContainerId) {
        *self.visits.entry(container).or_insert(0) += 1;
    }

    pub(crate) fn visit_count(&self, container: ContainerId) -> i32 {
        self.visits.get(&container).copied().unwrap_or(0)
    }

    pub(crate) fn record_turn(&mut self, container: ContainerId, turn: i32) {
        self.turns.insert(container, turn);
    }

    pub(crate) fn turn_index(&self, container: ContainerId) -> Option<i32> {
        self.turns.get(&container).copied()
    }

    pub(crate) fn visit_paths_for_save(
        &self,
        data: &impl StaticStoryView,
    ) -> Result<HashMap<String, i32>, StoryError> {
        Self::encode_paths(data, &self.visits)
    }

    pub(crate) fn turn_paths_for_save(
        &self,
        data: &impl StaticStoryView,
    ) -> Result<HashMap<String, i32>, StoryError> {
        Self::encode_paths(data, &self.turns)
    }

    pub(crate) fn restore_visit_paths(
        &mut self,
        data: &impl StaticStoryView,
        paths: &HashMap<String, i32>,
    ) -> Result<(), StoryError> {
        self.visits = Self::decode_paths(data, paths)?;
        Ok(())
    }

    pub(crate) fn restore_turn_paths(
        &mut self,
        data: &impl StaticStoryView,
        paths: &HashMap<String, i32>,
    ) -> Result<(), StoryError> {
        self.turns = Self::decode_paths(data, paths)?;
        Ok(())
    }

    fn encode_paths(
        data: &impl StaticStoryView,
        counts: &HashMap<ContainerId, i32>,
    ) -> Result<HashMap<String, i32>, StoryError> {
        let mut paths = HashMap::with_capacity(counts.len());
        for (&container, &count) in counts {
            let path = data.canonical_path_text(container.node()).ok_or_else(|| {
                StoryError::InvalidStoryState("counter has an invalid container ID".to_owned())
            })?;
            paths.insert(path, count);
        }
        Ok(paths)
    }

    fn decode_paths(
        data: &impl StaticStoryView,
        paths: &HashMap<String, i32>,
    ) -> Result<HashMap<ContainerId, i32>, StoryError> {
        let mut counts = HashMap::with_capacity(paths.len());
        for (path, &count) in paths {
            let parsed = Path::new_with_components_string(Some(path));
            let container = data
                .resolve_path(data.root(), &parsed)
                .and_then(|id| data.container_id(id))
                .ok_or_else(|| {
                    StoryError::InvalidStoryState(format!(
                        "counter path '{}' is not a container in this story",
                        path
                    ))
                })?;
            counts.insert(container, count);
        }
        Ok(counts)
    }
}
