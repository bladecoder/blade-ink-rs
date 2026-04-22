use std::{
    cell::Cell,
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

use bladeink::{Container, Path, RTObject, path_of};

use super::DebugMetadata;

static NEXT_OBJECT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Story,
    ContentList,
    Text,
    Expression,
    Number,
    StringExpression,
    VariableReference,
    VariableAssignment,
    List,
    ListDefinition,
    ListElementDefinition,
    DivertTarget,
    FunctionCall,
    Return,
    IncludedFile,
    TunnelOnwards,
    Conditional,
    ConditionalBranch,
    Sequence,
    Tag,
    AuthorWarning,
    ConstDeclaration,
    ExternalDeclaration,
    FlowBase,
    Knot,
    Stitch,
    Choice,
    Gather,
    Weave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedObjectRef {
    id: usize,
    kind: ObjectKind,
}

impl ParsedObjectRef {
    pub fn new(id: usize, kind: ObjectKind) -> Self {
        Self { id, kind }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn kind(&self) -> ObjectKind {
        self.kind
    }
}

#[derive(Default)]
pub(crate) struct ParsedRuntimeCache {
    runtime_object: RefCell<Option<Rc<dyn RTObject>>>,
    runtime_path_target: RefCell<Option<Rc<dyn RTObject>>>,
    container_for_counting: RefCell<Option<Rc<Container>>>,
    visits_should_be_counted: Cell<bool>,
    turn_index_should_be_counted: Cell<bool>,
}

impl std::fmt::Debug for ParsedRuntimeCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedRuntimeCache")
            .field("has_runtime_object", &self.runtime_object.borrow().is_some())
            .field("has_runtime_path_target", &self.runtime_path_target.borrow().is_some())
            .field(
                "has_container_for_counting",
                &self.container_for_counting.borrow().is_some(),
            )
            .field("visits_should_be_counted", &self.visits_should_be_counted.get())
            .field("turn_index_should_be_counted", &self.turn_index_should_be_counted.get())
            .finish()
    }
}

impl ParsedRuntimeCache {
    pub(crate) fn set_runtime_object(&self, runtime_object: Rc<dyn RTObject>) {
        self.runtime_object.replace(Some(runtime_object));
    }

    pub(crate) fn runtime_object(&self) -> Option<Rc<dyn RTObject>> {
        self.runtime_object.borrow().clone()
    }

    pub(crate) fn has_runtime_object(&self) -> bool {
        self.runtime_object.borrow().is_some()
    }

    pub(crate) fn set_runtime_path_target(&self, runtime_path_target: Rc<dyn RTObject>) {
        self.runtime_path_target.replace(Some(runtime_path_target));
    }

    pub(crate) fn runtime_path_target(&self) -> Option<Rc<dyn RTObject>> {
        self.runtime_path_target.borrow().clone()
    }

    pub(crate) fn runtime_path(&self) -> Option<Path> {
        self.runtime_path_target()
            .or_else(|| self.runtime_object())
            .map(|target| path_of(target.as_ref()))
    }

    pub(crate) fn set_container_for_counting(&self, container: Rc<Container>) {
        self.container_for_counting.replace(Some(container));
    }

    pub(crate) fn container_for_counting(&self) -> Option<Rc<Container>> {
        if let Some(container) = self.container_for_counting.borrow().clone() {
            return Some(container);
        }

        self.runtime_object()
            .and_then(|object| object.into_any().downcast::<Container>().ok())
    }

    pub(crate) fn clear(&self) {
        self.runtime_object.replace(None);
        self.runtime_path_target.replace(None);
        self.container_for_counting.replace(None);
    }

    pub(crate) fn mark_visits_should_be_counted(&self) {
        self.visits_should_be_counted.set(true);
    }

    pub(crate) fn mark_turn_index_should_be_counted(&self) {
        self.turn_index_should_be_counted.set(true);
    }

    pub(crate) fn count_flags(&self, default_visits: bool, counting_at_start_only: bool) -> i32 {
        let visits = default_visits || self.visits_should_be_counted.get();
        let turns = self.turn_index_should_be_counted.get();
        (visits as i32) + ((turns as i32) * 2) + ((counting_at_start_only as i32) * 4)
    }
}

pub struct ParsedObject {
    id: usize,
    kind: ObjectKind,
    parent: Option<ParsedObjectRef>,
    content: Vec<ParsedObjectRef>,
    debug_metadata: Option<DebugMetadata>,
    error_messages: Vec<String>,
    warning_messages: Vec<String>,
    runtime_cache: Rc<ParsedRuntimeCache>,
}

impl std::fmt::Debug for ParsedObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedObject")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("parent", &self.parent)
            .field("content", &self.content)
            .field("debug_metadata", &self.debug_metadata)
            .field("error_messages", &self.error_messages)
            .field("warning_messages", &self.warning_messages)
            .field("runtime_cache", &self.runtime_cache)
            .finish()
    }
}

impl Clone for ParsedObject {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            kind: self.kind,
            parent: self.parent,
            content: self.content.clone(),
            debug_metadata: self.debug_metadata.clone(),
            error_messages: self.error_messages.clone(),
            warning_messages: self.warning_messages.clone(),
            runtime_cache: self.runtime_cache.clone(),
        }
    }
}

impl ParsedObject {
    pub fn new(kind: ObjectKind) -> Self {
        Self {
            id: NEXT_OBJECT_ID.fetch_add(1, Ordering::Relaxed),
            kind,
            parent: None,
            content: Vec::new(),
            debug_metadata: None,
            error_messages: Vec::new(),
            warning_messages: Vec::new(),
            runtime_cache: Rc::new(ParsedRuntimeCache::default()),
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn kind(&self) -> ObjectKind {
        self.kind
    }

    pub fn reference(&self) -> ParsedObjectRef {
        ParsedObjectRef::new(self.id, self.kind)
    }

    pub fn parent_id(&self) -> Option<usize> {
        self.parent.map(|parent| parent.id())
    }

    pub fn parent_ref(&self) -> Option<ParsedObjectRef> {
        self.parent
    }

    pub fn set_parent_id(&mut self, parent_id: usize) {
        self.parent = Some(ParsedObjectRef::new(parent_id, ObjectKind::FlowBase));
    }

    pub fn set_parent(&mut self, parent: &ParsedObject) {
        self.parent = Some(parent.reference());
    }

    pub fn set_parent_ref(&mut self, parent: ParsedObjectRef) {
        self.parent = Some(parent);
    }

    pub fn content(&self) -> &[ParsedObjectRef] {
        &self.content
    }

    pub fn add_content_ref(&mut self, child: ParsedObjectRef) {
        self.content.push(child);
    }

    pub fn find_child_by_kind(&self, kind: ObjectKind) -> Option<ParsedObjectRef> {
        self.content
            .iter()
            .copied()
            .find(|child| child.kind() == kind)
    }

    pub fn find_all_children_by_kind(&self, kind: ObjectKind) -> Vec<ParsedObjectRef> {
        self.content
            .iter()
            .copied()
            .filter(|child| child.kind() == kind)
            .collect()
    }

    pub fn find(&self, index: &ParsedObjectIndex, kind: ObjectKind) -> Option<ParsedObjectRef> {
        index.find_descendant(self.reference(), kind)
    }

    pub fn find_all(&self, index: &ParsedObjectIndex, kind: ObjectKind) -> Vec<ParsedObjectRef> {
        index.find_all_descendants(self.reference(), kind)
    }

    pub fn ancestry(
        &self,
        mut resolve: impl FnMut(usize) -> Option<ParsedObjectRef>,
    ) -> Vec<ParsedObjectRef> {
        let mut result = Vec::new();
        let mut next = self.parent;

        while let Some(parent) = next {
            result.push(parent);
            next = resolve(parent.id()).and_then(|resolved| {
                if resolved.id() == parent.id() {
                    None
                } else {
                    Some(resolved)
                }
            });
        }

        result.reverse();
        result
    }

    pub fn closest_flow_base(
        &self,
        mut resolve: impl FnMut(usize) -> Option<ParsedObjectRef>,
    ) -> Option<ParsedObjectRef> {
        let mut next = self.parent;

        while let Some(parent) = next {
            if matches!(
                parent.kind(),
                ObjectKind::Story | ObjectKind::FlowBase | ObjectKind::Knot | ObjectKind::Stitch
            ) {
                return Some(parent);
            }

            next = resolve(parent.id()).and_then(|resolved| {
                if resolved.id() == parent.id() {
                    None
                } else {
                    Some(resolved)
                }
            });
        }

        None
    }

    pub fn story_ref(
        &self,
        mut resolve: impl FnMut(usize) -> Option<ParsedObjectRef>,
    ) -> Option<ParsedObjectRef> {
        if self.kind == ObjectKind::Story {
            return Some(self.reference());
        }

        let mut next = self.parent;
        while let Some(parent) = next {
            if parent.kind() == ObjectKind::Story {
                return Some(parent);
            }

            next = resolve(parent.id()).and_then(|resolved| {
                if resolved.id() == parent.id() {
                    None
                } else {
                    Some(resolved)
                }
            });
        }

        None
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

    pub fn set_runtime_object(&self, runtime_object: Rc<dyn RTObject>) {
        if let Some(debug_metadata) = self.debug_metadata() {
            runtime_object
                .get_object()
                .set_debug_metadata(debug_metadata.into());
        }
        self.runtime_cache.set_runtime_object(runtime_object);
    }

    pub fn runtime_object(&self) -> Option<Rc<dyn RTObject>> {
        self.runtime_cache.runtime_object()
    }

    pub fn has_runtime_object(&self) -> bool {
        self.runtime_cache.has_runtime_object()
    }

    pub fn set_runtime_path_target(&self, runtime_path_target: Rc<dyn RTObject>) {
        self.runtime_cache.set_runtime_path_target(runtime_path_target);
    }

    pub fn runtime_path(&self) -> Option<Path> {
        self.runtime_cache.runtime_path()
    }

    pub fn runtime_path_target(&self) -> Option<Rc<dyn RTObject>> {
        self.runtime_cache.runtime_path_target()
    }

    pub fn set_container_for_counting(&self, container: Rc<Container>) {
        self.runtime_cache.set_container_for_counting(container);
    }

    pub fn container_for_counting(&self) -> Option<Rc<Container>> {
        self.runtime_cache.container_for_counting()
    }

    pub fn clear_runtime_object_cache(&self) {
        self.runtime_cache.clear();
    }

    pub fn mark_visits_should_be_counted(&self) {
        self.runtime_cache.mark_visits_should_be_counted();
    }

    pub fn mark_turn_index_should_be_counted(&self) {
        self.runtime_cache.mark_turn_index_should_be_counted();
    }

    pub fn count_flags(&self, default_visits: bool, counting_at_start_only: bool) -> i32 {
        self.runtime_cache.count_flags(default_visits, counting_at_start_only)
    }

    pub(crate) fn runtime_cache_handle(&self) -> Rc<ParsedRuntimeCache> {
        self.runtime_cache.clone()
    }

    pub fn error(&mut self, message: impl Into<String>) {
        let message = message.into();
        if !self.error_messages.contains(&message) {
            self.error_messages.push(message);
        }
    }

    pub fn warning(&mut self, message: impl Into<String>) {
        let message = message.into();
        if !self.warning_messages.contains(&message) {
            self.warning_messages.push(message);
        }
    }

    pub fn errors(&self) -> &[String] {
        &self.error_messages
    }

    pub fn warnings(&self) -> &[String] {
        &self.warning_messages
    }

    pub fn had_error(&self) -> bool {
        !self.error_messages.is_empty()
    }

    pub fn had_warning(&self) -> bool {
        !self.warning_messages.is_empty()
    }
}

#[derive(Debug, Default, Clone)]
pub struct ParsedObjectIndex {
    objects: BTreeMap<usize, IndexedObject>,
}

#[derive(Debug, Clone)]
struct IndexedObject {
    reference: ParsedObjectRef,
    parent: Option<ParsedObjectRef>,
    content: Vec<ParsedObjectRef>,
}

impl ParsedObjectIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, object: &ParsedObject) {
        self.objects.insert(
            object.id(),
            IndexedObject {
                reference: object.reference(),
                parent: object.parent_ref(),
                content: object.content().to_vec(),
            },
        );
    }

    pub fn register_ref(
        &mut self,
        reference: ParsedObjectRef,
        parent: Option<ParsedObjectRef>,
        content: Vec<ParsedObjectRef>,
    ) {
        self.objects.insert(
            reference.id(),
            IndexedObject {
                reference,
                parent,
                content,
            },
        );
    }

    pub fn resolve(&self, id: usize) -> Option<ParsedObjectRef> {
        self.objects.get(&id).map(|object| object.reference)
    }

    pub fn parent_of(&self, id: usize) -> Option<ParsedObjectRef> {
        self.objects.get(&id).and_then(|object| object.parent)
    }

    pub fn content_of(&self, id: usize) -> &[ParsedObjectRef] {
        self.objects
            .get(&id)
            .map(|object| object.content.as_slice())
            .unwrap_or(&[])
    }

    pub fn ancestry_of(&self, object: ParsedObjectRef) -> Vec<ParsedObjectRef> {
        let mut result = Vec::new();
        let mut next = self.parent_of(object.id());

        while let Some(parent) = next {
            result.push(parent);
            next = self.parent_of(parent.id());
        }

        result.reverse();
        result
    }

    pub fn story_for(&self, object: ParsedObjectRef) -> Option<ParsedObjectRef> {
        if object.kind() == ObjectKind::Story {
            return Some(object);
        }

        self.ancestry_of(object)
            .into_iter()
            .find(|ancestor| ancestor.kind() == ObjectKind::Story)
    }

    pub fn closest_flow_base_for(&self, object: ParsedObjectRef) -> Option<ParsedObjectRef> {
        self.ancestry_of(object).into_iter().rev().find(|ancestor| {
            matches!(
                ancestor.kind(),
                ObjectKind::Story | ObjectKind::FlowBase | ObjectKind::Knot | ObjectKind::Stitch
            )
        })
    }

    pub fn find_descendant(
        &self,
        root: ParsedObjectRef,
        kind: ObjectKind,
    ) -> Option<ParsedObjectRef> {
        let mut queue = VecDeque::from(self.content_of(root.id()).to_vec());

        while let Some(candidate) = queue.pop_front() {
            if candidate.kind() == kind {
                return Some(candidate);
            }
            queue.extend(self.content_of(candidate.id()).iter().copied());
        }

        None
    }

    pub fn find_all_descendants(
        &self,
        root: ParsedObjectRef,
        kind: ObjectKind,
    ) -> Vec<ParsedObjectRef> {
        let mut found = Vec::new();
        let mut queue = VecDeque::from(self.content_of(root.id()).to_vec());

        while let Some(candidate) = queue.pop_front() {
            if candidate.kind() == kind {
                found.push(candidate);
            }
            queue.extend(self.content_of(candidate.id()).iter().copied());
        }

        found
    }
}

#[cfg(test)]
mod tests {
    use super::{ObjectKind, ParsedObject, ParsedObjectIndex};

    #[test]
    fn object_tracks_parent_and_content_refs() {
        let mut parent = ParsedObject::new(ObjectKind::Story);
        let mut child = ParsedObject::new(ObjectKind::Text);

        child.set_parent(&parent);
        parent.add_content_ref(child.reference());

        assert_eq!(Some(parent.id()), child.parent_id());
        assert_eq!(
            Some(child.reference()),
            parent.find_child_by_kind(ObjectKind::Text)
        );
        assert_eq!(1, parent.find_all_children_by_kind(ObjectKind::Text).len());
    }

    #[test]
    fn object_tracks_runtime_cache_state() {
        let object = ParsedObject::new(ObjectKind::ContentList);
        let runtime = bladeink::Container::new(None, 0, Vec::new(), std::collections::HashMap::new());
        object.set_runtime_object(runtime.clone());
        assert!(object.has_runtime_object());
        assert_eq!(Some("".to_owned()), object.runtime_path().map(|path| path.to_string()));
        object.set_container_for_counting(runtime.clone());
        assert!(object.container_for_counting().is_some());
        object.clear_runtime_object_cache();
        assert!(!object.has_runtime_object());
        assert!(object.runtime_path().is_none());
    }

    #[test]
    fn object_clone_shares_runtime_cache() {
        let object = ParsedObject::new(ObjectKind::ContentList);
        let cloned = object.clone();
        let runtime = bladeink::Container::new(None, 0, Vec::new(), std::collections::HashMap::new());

        cloned.set_runtime_object(runtime);

        assert!(object.has_runtime_object());
        object.clear_runtime_object_cache();
        assert!(!cloned.has_runtime_object());
    }

    #[test]
    fn object_deduplicates_errors_and_warnings() {
        let mut object = ParsedObject::new(ObjectKind::Text);
        object.error("bad");
        object.error("bad");
        object.warning("careful");
        object.warning("careful");

        assert_eq!(&["bad".to_owned()], object.errors());
        assert_eq!(&["careful".to_owned()], object.warnings());
        assert!(object.had_error());
        assert!(object.had_warning());
    }

    #[test]
    fn object_index_resolves_ancestry_story_flow_and_deep_search() {
        let mut story = ParsedObject::new(ObjectKind::Story);
        let mut knot = ParsedObject::new(ObjectKind::Knot);
        let mut content = ParsedObject::new(ObjectKind::ContentList);
        let mut text = ParsedObject::new(ObjectKind::Text);

        knot.set_parent(&story);
        content.set_parent(&knot);
        text.set_parent(&content);
        story.add_content_ref(knot.reference());
        knot.add_content_ref(content.reference());
        content.add_content_ref(text.reference());

        let mut index = ParsedObjectIndex::new();
        for object in [&story, &knot, &content, &text] {
            index.register(object);
        }

        assert_eq!(
            vec![story.reference(), knot.reference(), content.reference()],
            index.ancestry_of(text.reference())
        );
        assert_eq!(Some(story.reference()), index.story_for(text.reference()));
        assert_eq!(
            Some(knot.reference()),
            index.closest_flow_base_for(text.reference())
        );
        assert_eq!(Some(text.reference()), story.find(&index, ObjectKind::Text));
        assert_eq!(1, story.find_all(&index, ObjectKind::Text).len());
    }
}
