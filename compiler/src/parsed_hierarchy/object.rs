use std::sync::atomic::{AtomicUsize, Ordering};

use super::DebugMetadata;

static NEXT_OBJECT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Story,
    ContentList,
    Text,
    FlowBase,
    Knot,
    Stitch,
}

#[derive(Debug, Clone)]
pub struct ParsedObject {
    id: usize,
    kind: ObjectKind,
    parent_id: Option<usize>,
    debug_metadata: Option<DebugMetadata>,
}

impl ParsedObject {
    pub fn new(kind: ObjectKind) -> Self {
        Self {
            id: NEXT_OBJECT_ID.fetch_add(1, Ordering::Relaxed),
            kind,
            parent_id: None,
            debug_metadata: None,
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn kind(&self) -> ObjectKind {
        self.kind
    }

    pub fn parent_id(&self) -> Option<usize> {
        self.parent_id
    }

    pub fn set_parent_id(&mut self, parent_id: usize) {
        self.parent_id = Some(parent_id);
    }

    pub fn debug_metadata(&self) -> Option<&DebugMetadata> {
        self.debug_metadata.as_ref()
    }

    pub fn set_debug_metadata(&mut self, debug_metadata: DebugMetadata) {
        self.debug_metadata = Some(debug_metadata);
    }

    pub fn has_own_debug_metadata(&self) -> bool {
        self.debug_metadata.is_some()
    }
}
