//! Flat, pointer-free structural representation of static Ink content.
//!
//! The JSON readers construct the arena directly. A legacy-tree converter is
//! retained for differential tests; no `Rc` is retained in `FlatStoryData`.

#[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
mod json_direct;

#[allow(unused_imports)]
use crate::prelude::*;

use crate::compat::fmt::Write as _;
use crate::{
    choice_point::ChoicePoint,
    compat::{collections::HashMap, io::Read, rc::Rc},
    container::Container,
    control_command::ControlCommand,
    divert::Divert,
    glue::Glue,
    ink_list::InkList,
    ink_list_item::InkListItem,
    list_definitions_origin::ListDefinitionsOrigin,
    native_function_call::{NativeFunctionCall, Op},
    object::RTObject,
    path::{Component, Path},
    push_pop::PushPopType,
    story_error::StoryError,
    tag::Tag,
    value::Value,
    value_type::ValueType,
    variable_assigment::VariableAssignment,
    variable_reference::VariableReference,
    void::Void,
};
use as_any::Downcast;

/// Stable index within one story. Zero is the root container.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct NodeId(pub(crate) u32);

impl NodeId {
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

/// A node that has container metadata and ordered children.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ContainerId(NodeId);

impl ContainerId {
    pub(crate) fn node(self) -> NodeId {
        self.0
    }
}

/// Location of the next instruction within a static container.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContentCursor {
    pub(crate) container: ContainerId,
    pub(crate) index: u32,
}

/// Interpreter pointer without an owning `Rc<Container>`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FlatPointer {
    pub(crate) container: Option<ContainerId>,
    pub(crate) index: i32,
}

impl FlatPointer {
    pub(crate) const NULL: Self = Self {
        container: None,
        index: -1,
    };

    pub(crate) const fn is_null(self) -> bool {
        self.container.is_none()
    }

    pub(crate) fn start_of(container: ContainerId) -> Self {
        Self {
            container: Some(container),
            index: 0,
        }
    }

    pub(crate) fn at_container(container: ContainerId) -> Self {
        Self {
            container: Some(container),
            index: -1,
        }
    }

    pub(crate) fn resolve(self, data: &impl StaticStoryView) -> Option<NodeId> {
        let container = self.container?;
        if self.index < 0 || data.child_count(container.node())? == 0 {
            return Some(container.node());
        }
        let index = usize::try_from(self.index).ok()?;
        data.child_at(container.node(), index)
    }

    pub(crate) fn path(self, data: &FlatStoryData) -> Option<Path> {
        let container = self.container?;
        let path = data.path_for(container.node())?;
        if self.index < 0 {
            return Some(path);
        }
        Some(path.path_by_appending_component(Component::new_i(usize::try_from(self.index).ok()?)))
    }

    /// Moves to the next indexed item, climbing out of finished containers.
    pub(crate) fn increment(self, data: &impl StaticStoryView) -> Option<Self> {
        let mut container = self.container?;
        let mut index = self.index.checked_add(1)?;
        loop {
            if usize::try_from(index).ok()? < data.child_count(container.node())? {
                return Some(Self {
                    container: Some(container),
                    index,
                });
            }
            let node = data.node_view(container.node())?;
            let parent = node.parent?;
            index = i32::try_from(node.child_index?).ok()?.checked_add(1)?;
            if !matches!(data.node_view(parent)?.kind, NodeKindView::Container { .. }) {
                return None;
            }
            container = ContainerId(parent);
        }
    }
}

/// A container's ordered children and named children occupy separate ranges.
#[derive(Debug)]
pub(crate) struct ContainerRecord {
    pub(crate) name: Option<String>,
    pub(crate) count_flags: i32,
    pub(crate) children: core::ops::Range<usize>,
    pub(crate) named: core::ops::Range<usize>,
}

/// Path components owned by the static arena, with no lazy cell.
#[derive(Debug)]
pub(crate) struct StaticPath {
    pub(crate) components: Vec<Component>,
    pub(crate) relative: bool,
}

impl StaticPath {
    pub(crate) fn from_text(text: &str) -> Self {
        if text.is_empty() {
            return Self {
                components: Vec::new(),
                relative: false,
            };
        }
        let (relative, text) = match text.strip_prefix('.') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let mut components = Vec::with_capacity(text.split('.').count());
        for part in text.split('.') {
            components.push(match part.parse::<usize>() {
                Ok(index) => Component::new_i(index),
                Err(_) => Component::new(part),
            });
        }
        Self {
            components,
            relative,
        }
    }

    pub(crate) fn from_runtime(path: &Path) -> Self {
        let components = (0..path.len())
            .map(|index| path.get_component(index).unwrap().clone())
            .collect();
        Self {
            components,
            relative: path.is_relative(),
        }
    }

    pub(crate) fn runtime_path(&self) -> Path {
        Path::new(&self.components, self.relative)
    }
}

#[derive(Debug)]
pub(crate) struct StaticList {
    pub(crate) items: Vec<(InkListItem, i32)>,
    pub(crate) origin_names: Vec<String>,
}

impl StaticList {
    fn from_runtime(list: &InkList) -> Self {
        let mut items: Vec<_> = list
            .items
            .iter()
            .map(|(item, value)| (item.clone(), *value))
            .collect();
        items.sort_unstable_by(|left, right| {
            left.0
                .get_origin_name()
                .cmp(&right.0.get_origin_name())
                .then_with(|| left.0.get_item_name().cmp(right.0.get_item_name()))
        });
        let mut origin_names = list.get_origin_names();
        origin_names.sort_unstable();
        origin_names.dedup();
        Self {
            items,
            origin_names,
        }
    }

    /// Copies a static list only when it enters mutable evaluation state.
    fn to_runtime(&self) -> InkList {
        let mut list = InkList::new();
        for (item, value) in &self.items {
            list.items.insert(item.clone(), *value);
        }
        list.set_initial_origin_names(self.origin_names.clone());
        list
    }
}

#[derive(Debug)]
pub(crate) enum StaticValue {
    Bool(bool),
    Int(i32),
    Float(f32),
    String(String),
    List(StaticList),
    DivertTarget {
        path: StaticPath,
        target: Option<NodeId>,
    },
    VariablePointer {
        name: String,
        context_index: i32,
    },
}

#[derive(Debug)]
pub(crate) struct DivertRecord {
    pub(crate) path: Option<StaticPath>,
    pub(crate) target: Option<NodeId>,
    pub(crate) variable_name: Option<String>,
    pub(crate) external_args: usize,
    pub(crate) conditional: bool,
    pub(crate) external: bool,
    pub(crate) pushes_to_stack: bool,
    pub(crate) stack_push_type: PushPopType,
}

#[derive(Debug)]
pub(crate) struct ChoiceRecord {
    pub(crate) path: Option<StaticPath>,
    pub(crate) target: Option<ContainerId>,
    pub(crate) flags: i32,
}

#[derive(Debug)]
pub(crate) struct VariableReferenceRecord {
    pub(crate) name: String,
    pub(crate) count_path: Option<StaticPath>,
    pub(crate) count_target: Option<ContainerId>,
}

#[derive(Debug)]
pub(crate) enum NodeKind {
    Container(ContainerRecord),
    ChoicePoint(ChoiceRecord),
    ControlCommand(crate::control_command::CommandType),
    Divert(DivertRecord),
    Glue,
    NativeFunction(Op),
    Tag(String),
    Value(StaticValue),
    VariableAssignment {
        name: String,
        global: bool,
        new_declaration: bool,
    },
    VariableReference(VariableReferenceRecord),
    Void,
}

#[derive(Debug)]
pub(crate) struct NodeRecord {
    pub(crate) parent: Option<NodeId>,
    /// Position in the parent's ordered children; absent for named-only nodes.
    pub(crate) child_index: Option<u32>,
    pub(crate) kind: NodeKind,
}

/// Borrowed static operands. An image backend can provide these without
/// materializing owned arena records during execution.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PathView<'a> {
    Arena(&'a StaticPath),
}

impl PathView<'_> {
    pub(crate) fn to_runtime(self) -> Path {
        match self {
            Self::Arena(path) => path.runtime_path(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ListView<'a> {
    Arena(&'a StaticList),
}

impl ListView<'_> {
    fn to_runtime(self) -> InkList {
        match self {
            Self::Arena(list) => list.to_runtime(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ValueView<'a> {
    Bool(bool),
    Int(i32),
    Float(f32),
    String(&'a str),
    List(ListView<'a>),
    DivertTarget(PathView<'a>),
    VariablePointer { name: &'a str, context_index: i32 },
}

impl ValueView<'_> {
    pub(crate) fn to_runtime(self) -> ValueType {
        match self {
            Self::Bool(value) => ValueType::Bool(value),
            Self::Int(value) => ValueType::Int(value),
            Self::Float(value) => ValueType::Float(value),
            Self::String(value) => ValueType::new(value),
            Self::List(value) => ValueType::List(value.to_runtime()),
            Self::DivertTarget(path) => ValueType::DivertTarget(path.to_runtime()),
            Self::VariablePointer {
                name,
                context_index,
            } => ValueType::VariablePointer(crate::value_type::VariablePointerValue {
                variable_name: name.to_owned(),
                context_index,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum NodeKindView<'a> {
    Container {
        count_flags: i32,
    },
    ChoicePoint {
        flags: i32,
        target: Option<ContainerId>,
    },
    ControlCommand(crate::control_command::CommandType),
    Divert {
        path: Option<PathView<'a>>,
        target: Option<NodeId>,
        variable_name: Option<&'a str>,
        external_args: usize,
        conditional: bool,
        external: bool,
        pushes_to_stack: bool,
        stack_push_type: PushPopType,
    },
    Glue,
    NativeFunction(Op),
    Tag(&'a str),
    Value(ValueView<'a>),
    VariableAssignment {
        name: &'a str,
        global: bool,
        new_declaration: bool,
    },
    VariableReference {
        name: &'a str,
        count_target: Option<ContainerId>,
    },
    Void,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NodeView<'a> {
    pub(crate) parent: Option<NodeId>,
    pub(crate) child_index: Option<u32>,
    pub(crate) kind: NodeKindView<'a>,
}

/// Read-only node access used by the interpreter. Views borrow operands from
/// their storage and do not allocate as instructions are fetched.
pub(crate) trait StaticStoryView {
    fn node_count(&self) -> usize;
    fn node_view(&self, id: NodeId) -> Option<NodeView<'_>>;
    fn child_count(&self, id: NodeId) -> Option<usize>;
    fn child_at(&self, id: NodeId, index: usize) -> Option<NodeId>;
    fn named_child_count(&self, id: NodeId) -> Option<usize>;
    fn named_child_at(&self, id: NodeId, index: usize) -> Option<NamedChildView<'_>>;

    fn find_named_child(&self, id: NodeId, name: &str) -> Option<NodeId> {
        let mut low = 0;
        let mut high = self.named_child_count(id)?;
        while low < high {
            let middle = low + (high - low) / 2;
            let entry = self.named_child_at(id, middle)?;
            match entry.name.cmp(name) {
                core::cmp::Ordering::Less => low = middle + 1,
                core::cmp::Ordering::Greater => high = middle,
                core::cmp::Ordering::Equal => return Some(entry.node),
            }
        }
        None
    }
}

impl<T: StaticStoryView> StaticStoryView for Rc<T> {
    fn node_count(&self) -> usize {
        self.as_ref().node_count()
    }

    fn node_view(&self, id: NodeId) -> Option<NodeView<'_>> {
        self.as_ref().node_view(id)
    }

    fn child_count(&self, id: NodeId) -> Option<usize> {
        self.as_ref().child_count(id)
    }

    fn child_at(&self, id: NodeId, index: usize) -> Option<NodeId> {
        self.as_ref().child_at(id, index)
    }

    fn named_child_count(&self, id: NodeId) -> Option<usize> {
        self.as_ref().named_child_count(id)
    }

    fn named_child_at(&self, id: NodeId, index: usize) -> Option<NamedChildView<'_>> {
        self.as_ref().named_child_at(id, index)
    }
}

#[derive(Debug)]
pub(crate) struct NamedChild {
    pub(crate) name: String,
    pub(crate) node: NodeId,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NamedChildView<'a> {
    pub(crate) name: &'a str,
    pub(crate) node: NodeId,
}

#[derive(Debug)]
pub(crate) struct StaticListDefinition {
    pub(crate) name: String,
    pub(crate) items: Vec<(String, i32)>,
}

/// Structural arena. All fields own their data and contain no runtime pointers.
pub(crate) struct FlatStoryData {
    pub(crate) nodes: Vec<NodeRecord>,
    pub(crate) children: Vec<NodeId>,
    pub(crate) named: Vec<NamedChild>,
    pub(crate) list_definitions: Vec<StaticListDefinition>,
}

impl StaticStoryView for FlatStoryData {
    fn node_count(&self) -> usize {
        self.nodes.len()
    }

    fn node_view(&self, id: NodeId) -> Option<NodeView<'_>> {
        let node = self.nodes.get(id.index())?;
        let kind = match &node.kind {
            NodeKind::Container(record) => NodeKindView::Container {
                count_flags: record.count_flags,
            },
            NodeKind::ChoicePoint(record) => NodeKindView::ChoicePoint {
                flags: record.flags,
                target: record.target,
            },
            NodeKind::ControlCommand(command) => NodeKindView::ControlCommand(*command),
            NodeKind::Divert(record) => NodeKindView::Divert {
                path: record.path.as_ref().map(PathView::Arena),
                target: record.target,
                variable_name: record.variable_name.as_deref(),
                external_args: record.external_args,
                conditional: record.conditional,
                external: record.external,
                pushes_to_stack: record.pushes_to_stack,
                stack_push_type: record.stack_push_type,
            },
            NodeKind::Glue => NodeKindView::Glue,
            NodeKind::NativeFunction(op) => NodeKindView::NativeFunction(*op),
            NodeKind::Tag(text) => NodeKindView::Tag(text),
            NodeKind::Value(value) => NodeKindView::Value(match value {
                StaticValue::Bool(value) => ValueView::Bool(*value),
                StaticValue::Int(value) => ValueView::Int(*value),
                StaticValue::Float(value) => ValueView::Float(*value),
                StaticValue::String(value) => ValueView::String(value),
                StaticValue::List(value) => ValueView::List(ListView::Arena(value)),
                StaticValue::DivertTarget { path, .. } => {
                    ValueView::DivertTarget(PathView::Arena(path))
                }
                StaticValue::VariablePointer {
                    name,
                    context_index,
                } => ValueView::VariablePointer {
                    name,
                    context_index: *context_index,
                },
            }),
            NodeKind::VariableAssignment {
                name,
                global,
                new_declaration,
            } => NodeKindView::VariableAssignment {
                name,
                global: *global,
                new_declaration: *new_declaration,
            },
            NodeKind::VariableReference(record) => NodeKindView::VariableReference {
                name: &record.name,
                count_target: record.count_target,
            },
            NodeKind::Void => NodeKindView::Void,
        };
        Some(NodeView {
            parent: node.parent,
            child_index: node.child_index,
            kind,
        })
    }

    fn child_count(&self, id: NodeId) -> Option<usize> {
        Some(self.children(id)?.len())
    }

    fn child_at(&self, id: NodeId, index: usize) -> Option<NodeId> {
        self.children(id)?.get(index).copied()
    }

    fn named_child_count(&self, id: NodeId) -> Option<usize> {
        Some(self.named_children(id)?.len())
    }

    fn named_child_at(&self, id: NodeId, index: usize) -> Option<NamedChildView<'_>> {
        let entry = self.named_children(id)?.get(index)?;
        Some(NamedChildView {
            name: &entry.name,
            node: entry.node,
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) enum LoadPhase {
    JsonDecoded,
    ArenaBuilt,
    StreamParsedAndBuilt,
    TargetsResolved,
    RuntimeInitialized,
}

pub(crate) trait LoadObserver {
    fn record(&mut self, phase: LoadPhase);
}

pub(crate) struct NoopLoadObserver;

impl LoadObserver for NoopLoadObserver {
    fn record(&mut self, _phase: LoadPhase) {}
}

/// Timings for the flat JSON constructor. Streaming reads JSON while building
/// the arena, so those two costs are reported together.
#[cfg(feature = "load-profile")]
#[derive(Clone, Copy, Debug, Default)]
pub struct FlatLoadProfile {
    pub json_decode: Option<core::time::Duration>,
    pub arena_build: Option<core::time::Duration>,
    pub stream_parse_and_build: Option<core::time::Duration>,
    pub targets: core::time::Duration,
    pub runtime: core::time::Duration,
}

#[cfg(feature = "load-profile")]
pub(crate) struct TimedLoadObserver {
    last: std::time::Instant,
    pub(crate) profile: FlatLoadProfile,
}

#[cfg(feature = "load-profile")]
impl TimedLoadObserver {
    pub(crate) fn new() -> Self {
        Self {
            last: std::time::Instant::now(),
            profile: FlatLoadProfile::default(),
        }
    }
}

#[cfg(feature = "load-profile")]
impl LoadObserver for TimedLoadObserver {
    fn record(&mut self, phase: LoadPhase) {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last);
        self.last = now;
        match phase {
            LoadPhase::JsonDecoded => self.profile.json_decode = Some(elapsed),
            LoadPhase::ArenaBuilt => self.profile.arena_build = Some(elapsed),
            LoadPhase::StreamParsedAndBuilt => self.profile.stream_parse_and_build = Some(elapsed),
            LoadPhase::TargetsResolved => self.profile.targets = elapsed,
            LoadPhase::RuntimeInitialized => self.profile.runtime = elapsed,
        }
    }
}

/// Mutable data belongs to one run, never to a static node or image view.
pub(crate) struct FlatRuntimeCaches {
    path_limit: usize,
    clock: u64,
    paths: HashMap<NodeId, CachedPath>,
    uncached_path: String,
}

struct CachedPath {
    text: String,
    last_used: u64,
}

impl FlatRuntimeCaches {
    pub(crate) fn new(path_limit: usize) -> Self {
        Self {
            path_limit,
            clock: 0,
            paths: HashMap::new(),
            uncached_path: String::new(),
        }
    }

    /// Only visited nodes can occupy the cache. Eviction affects speed only.
    pub(crate) fn path<'a>(&'a mut self, data: &FlatStoryData, id: NodeId) -> Option<&'a str> {
        if self.clock == u64::MAX {
            self.paths.clear();
            self.clock = 0;
        }
        self.clock += 1;
        let hit = if let Some(cached) = self.paths.get_mut(&id) {
            cached.last_used = self.clock;
            true
        } else {
            false
        };
        if hit {
            return self.paths.get(&id).map(|cached| cached.text.as_str());
        }

        let text = data.canonical_path_text(id)?;
        if self.path_limit == 0 {
            self.uncached_path = text;
            return Some(&self.uncached_path);
        }
        if self.paths.len() == self.path_limit
            && let Some((&oldest, _)) = self.paths.iter().min_by_key(|(_, cached)| cached.last_used)
        {
            self.paths.remove(&oldest);
        }
        self.paths.insert(
            id,
            CachedPath {
                text,
                last_used: self.clock,
            },
        );
        self.paths.get(&id).map(|cached| cached.text.as_str())
    }
}

/// Visit and turn counters use compact IDs during execution. Save codecs
/// translate these keys to canonical Ink paths at their boundary.
#[derive(Clone)]
pub(crate) struct FlatCounters {
    visits: HashMap<ContainerId, i32>,
    turns: HashMap<ContainerId, i32>,
}

impl FlatCounters {
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
        data: &FlatStoryData,
    ) -> Result<HashMap<String, i32>, StoryError> {
        Self::encode_paths(data, &self.visits)
    }

    pub(crate) fn turn_paths_for_save(
        &self,
        data: &FlatStoryData,
    ) -> Result<HashMap<String, i32>, StoryError> {
        Self::encode_paths(data, &self.turns)
    }

    pub(crate) fn restore_visit_paths(
        &mut self,
        data: &FlatStoryData,
        paths: &HashMap<String, i32>,
    ) -> Result<(), StoryError> {
        self.visits = Self::decode_paths(data, paths)?;
        Ok(())
    }

    pub(crate) fn restore_turn_paths(
        &mut self,
        data: &FlatStoryData,
        paths: &HashMap<String, i32>,
    ) -> Result<(), StoryError> {
        self.turns = Self::decode_paths(data, paths)?;
        Ok(())
    }

    fn encode_paths(
        data: &FlatStoryData,
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
        data: &FlatStoryData,
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

impl FlatStoryData {
    /// Builds the arena directly from compiled Ink JSON using the selected
    /// JSON reader.
    pub(crate) fn from_json_reader(reader: impl Read) -> Result<(i32, Self), StoryError> {
        Self::from_json_reader_observed(reader, &mut NoopLoadObserver)
    }

    pub(crate) fn from_json_reader_observed(
        reader: impl Read,
        observer: &mut impl LoadObserver,
    ) -> Result<(i32, Self), StoryError> {
        #[cfg(feature = "stream-json-parser")]
        {
            crate::json::flat_json_stream::load_from_reader(reader, observer)
        }
        #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
        json_direct::load_from_reader(reader, observer)
    }

    /// Converts the current parser's tree as a temporary migration step.
    /// The returned arena contains no references to `root`.
    pub(crate) fn from_legacy_tree(
        root: &Rc<Container>,
        lists: &ListDefinitionsOrigin,
    ) -> Result<Self, StoryError> {
        let mut list_definitions: Vec<_> = lists
            .definitions()
            .map(|definition| {
                let mut items: Vec<_> = definition
                    .item_values()
                    .iter()
                    .map(|(name, value)| (name.clone(), *value))
                    .collect();
                items.sort_unstable_by(|left, right| left.0.cmp(&right.0));
                StaticListDefinition {
                    name: definition.get_name().to_owned(),
                    items,
                }
            })
            .collect();
        list_definitions.sort_unstable_by(|left, right| left.name.cmp(&right.name));
        let mut data = Self {
            nodes: Vec::new(),
            children: Vec::new(),
            named: Vec::new(),
            list_definitions,
        };
        let mut pending: Vec<Rc<dyn RTObject>> = Vec::new();
        let mut ids_by_pointer = HashMap::new();
        let root_object: Rc<dyn RTObject> = root.clone();
        let root_id = data.register(root_object, None, None, &mut pending, &mut ids_by_pointer)?;
        debug_assert_eq!(root_id, NodeId(0));

        let mut current = 0;
        while current < pending.len() {
            let node = pending[current].clone();
            if let Some(container) = node.as_ref().downcast_ref::<Container>() {
                let parent = NodeId(u32::try_from(current).map_err(|_| {
                    StoryError::BadJson("story contains too many nodes".to_owned())
                })?);

                let mut child_ids = Vec::with_capacity(container.content.len());
                for (index, child) in container.content.iter().enumerate() {
                    let child_index = u32::try_from(index).map_err(|_| {
                        StoryError::BadJson("container contains too many children".to_owned())
                    })?;
                    child_ids.push(data.register(
                        child.clone(),
                        Some(parent),
                        Some(child_index),
                        &mut pending,
                        &mut ids_by_pointer,
                    )?);
                }
                let child_start = data.children.len();
                data.children.extend(child_ids);
                let child_end = data.children.len();

                let mut named: Vec<_> = container.named_content.iter().collect();
                named.sort_unstable_by(|left, right| left.0.cmp(right.0));
                let mut named_ids = Vec::with_capacity(named.len());
                for (name, child) in named {
                    let child_object: Rc<dyn RTObject> = child.clone();
                    let child_id = data.register(
                        child_object,
                        Some(parent),
                        None,
                        &mut pending,
                        &mut ids_by_pointer,
                    )?;
                    named_ids.push(NamedChild {
                        name: name.clone(),
                        node: child_id,
                    });
                }
                let named_start = data.named.len();
                data.named.extend(named_ids);
                let named_end = data.named.len();

                let NodeKind::Container(record) = &mut data.nodes[current].kind else {
                    unreachable!("container classification changed")
                };
                record.children = child_start..child_end;
                record.named = named_start..named_end;
            }
            current += 1;
        }

        data.resolve_static_targets()?;
        Ok(data)
    }

    pub(crate) fn resolve_static_targets(&mut self) -> Result<(), StoryError> {
        for index in 0..self.nodes.len() {
            let origin =
                NodeId(u32::try_from(index).map_err(|_| {
                    StoryError::BadJson("story contains too many nodes".to_owned())
                })?);
            let path = match &self.nodes[index].kind {
                NodeKind::Divert(divert) if !divert.external && divert.variable_name.is_none() => {
                    divert.path.as_ref()
                }
                NodeKind::ChoicePoint(choice) => choice.path.as_ref(),
                NodeKind::VariableReference(reference) => reference.count_path.as_ref(),
                NodeKind::Value(StaticValue::DivertTarget { path, .. }) => Some(path),
                _ => None,
            };
            let Some(path) = path else { continue };
            let target = self.resolve_static_path(origin, path).ok_or_else(|| {
                StoryError::BadJson("static story target could not be resolved".to_owned())
            })?;
            let target_container = self.container_id(target);
            let target_is_container = target_container.is_some();
            if matches!(
                &self.nodes[index].kind,
                NodeKind::ChoicePoint(_) | NodeKind::VariableReference(_)
            ) && !target_is_container
            {
                return Err(StoryError::BadJson(
                    "choice or read-count target is not a container".to_owned(),
                ));
            }
            if let NodeKind::Divert(divert) = &self.nodes[index].kind
                && !divert
                    .path
                    .as_ref()
                    .and_then(|path| path.components.last())
                    .is_some_and(Component::is_index)
                && !target_is_container
            {
                return Err(StoryError::BadJson(
                    "named divert target is not a container".to_owned(),
                ));
            }
            match &mut self.nodes[index].kind {
                NodeKind::Divert(divert) => {
                    divert.target = Some(target);
                    divert.path = None;
                }
                NodeKind::ChoicePoint(choice) => {
                    choice.target = target_container;
                    choice.path = None;
                }
                NodeKind::VariableReference(reference) => {
                    reference.count_target = target_container;
                    reference.count_path = None;
                }
                NodeKind::Value(StaticValue::DivertTarget {
                    target: value_target,
                    ..
                }) => *value_target = Some(target),
                _ => unreachable!("target-bearing node changed kind"),
            }
        }
        Ok(())
    }

    fn register(
        &mut self,
        node: Rc<dyn RTObject>,
        parent: Option<NodeId>,
        child_index: Option<u32>,
        pending: &mut Vec<Rc<dyn RTObject>>,
        ids_by_pointer: &mut HashMap<usize, NodeId>,
    ) -> Result<NodeId, StoryError> {
        let pointer = Rc::as_ptr(&node) as *const () as usize;
        if let Some(&id) = ids_by_pointer.get(&pointer) {
            let existing = &self.nodes[id.index()];
            if existing.parent != parent
                || child_index.is_some_and(|index| existing.child_index != Some(index))
            {
                return Err(StoryError::BadJson(
                    "a story node appears at multiple content positions".to_owned(),
                ));
            }
            return Ok(id);
        }

        let id = NodeId(
            u32::try_from(self.nodes.len())
                .map_err(|_| StoryError::BadJson("story contains too many nodes".to_owned()))?,
        );
        let kind = classify(node.as_ref())?;
        self.nodes.push(NodeRecord {
            parent,
            child_index,
            kind,
        });
        ids_by_pointer.insert(pointer, id);
        pending.push(node);
        Ok(id)
    }

    pub(crate) fn root(&self) -> NodeId {
        NodeId(0)
    }

    /// Finds a list item in static definitions without materializing all
    /// possible list values during story construction.
    pub(crate) fn list_item(&self, name: &str) -> Option<(InkListItem, i32)> {
        if name.trim().is_empty() {
            return None;
        }
        let (qualified_origin, item_name) = match name.split_once('.') {
            Some((origin, item)) => (Some(origin), item),
            None => (None, name),
        };
        let mut found = None;
        for definition in &self.list_definitions {
            if qualified_origin.is_some_and(|origin| origin != definition.name) {
                continue;
            }
            if let Ok(index) = definition
                .items
                .binary_search_by(|(item, _)| item.as_str().cmp(item_name))
            {
                let value = definition.items[index].1;
                found = Some((
                    InkListItem::from_full_name(&format!("{}.{}", definition.name, item_name)),
                    value,
                ));
            }
        }
        found
    }

    pub(crate) fn node(&self, id: NodeId) -> Option<&NodeRecord> {
        self.nodes.get(id.index())
    }

    pub(crate) fn container_id(&self, id: NodeId) -> Option<ContainerId> {
        matches!(&self.node(id)?.kind, NodeKind::Container(_)).then_some(ContainerId(id))
    }

    pub(crate) fn start_of(&self, id: ContainerId) -> ContentCursor {
        ContentCursor {
            container: id,
            index: 0,
        }
    }

    pub(crate) fn at_cursor(&self, cursor: ContentCursor) -> Option<NodeId> {
        self.children(cursor.container.node())?
            .get(cursor.index as usize)
            .copied()
    }

    /// Advances past a node, climbing out of exhausted containers.
    pub(crate) fn next_after(&self, id: NodeId) -> Option<ContentCursor> {
        let mut current = id;
        loop {
            let record = self.node(current)?;
            let parent = record.parent?;
            if let Some(index) = record.child_index {
                let next_index = index.checked_add(1)?;
                let container = self.container_id(parent)?;
                let cursor = ContentCursor {
                    container,
                    index: next_index,
                };
                if self.at_cursor(cursor).is_some() {
                    return Some(cursor);
                }
            }
            current = parent;
        }
    }

    pub(crate) fn children(&self, id: NodeId) -> Option<&[NodeId]> {
        let NodeKind::Container(container) = &self.node(id)?.kind else {
            return None;
        };
        Some(&self.children[container.children.clone()])
    }

    pub(crate) fn named_children(&self, id: NodeId) -> Option<&[NamedChild]> {
        let NodeKind::Container(container) = &self.node(id)?.kind else {
            return None;
        };
        Some(&self.named[container.named.clone()])
    }

    pub(crate) fn named_child(&self, id: NodeId, name: &str) -> Option<NodeId> {
        let named = self.named_children(id)?;
        let index = named
            .binary_search_by(|entry| entry.name.as_str().cmp(name))
            .ok()?;
        Some(named[index].node)
    }

    /// Resolves Ink path components using only IDs and arena ranges.
    /// A relative path on a leaf starts at its parent, as in `Object::resolve_path`.
    pub(crate) fn resolve_path(&self, origin: NodeId, path: &Path) -> Option<NodeId> {
        self.resolve_components(origin, path.components(), path.is_relative())
    }

    fn resolve_static_path(&self, origin: NodeId, path: &StaticPath) -> Option<NodeId> {
        self.resolve_components(origin, &path.components, path.relative)
    }

    fn resolve_components(
        &self,
        origin: NodeId,
        components: &[Component],
        relative: bool,
    ) -> Option<NodeId> {
        let (mut current, start) = if relative {
            match &self.node(origin)?.kind {
                NodeKind::Container(_) => (origin, 0),
                _ => (self.node(origin)?.parent?, 1),
            }
        } else {
            (self.root(), 0)
        };

        for component in components.iter().skip(start) {
            if component.is_parent() {
                current = self.node(current)?.parent?;
            } else if let Some(child_index) = component.index {
                current = *self.children(current)?.get(child_index)?;
            } else {
                current = self.named_child(current, component.name.as_deref()?)?;
            }
        }
        Some(current)
    }

    /// Resolves a public Ink path to the pointer shape used by the call stack.
    pub(crate) fn pointer_at_path(&self, path: &Path) -> Option<FlatPointer> {
        if path.is_empty() {
            return Some(FlatPointer::NULL);
        }
        let last = path.get_last_component()?;
        if let Some(index) = last.index {
            let components: Vec<_> = (0..path.len() - 1)
                .map(|position| path.get_component(position).cloned())
                .collect::<Option<_>>()?;
            let parent_path = Path::new(&components, path.is_relative());
            let parent = self.resolve_path(self.root(), &parent_path)?;
            let container = self.container_id(parent)?;
            Some(FlatPointer {
                container: Some(container),
                index: i32::try_from(index).ok()?,
            })
        } else {
            let target = self.resolve_path(self.root(), path)?;
            Some(FlatPointer::at_container(self.container_id(target)?))
        }
    }

    /// Converts a linked static destination to the pointer form used by the
    /// interpreter, without formatting and resolving a path again.
    pub(crate) fn pointer_for(&self, id: NodeId) -> Option<FlatPointer> {
        if let Some(container) = self.container_id(id) {
            return Some(FlatPointer::at_container(container));
        }
        let record = self.node(id)?;
        Some(FlatPointer {
            container: Some(self.container_id(record.parent?)?),
            index: i32::try_from(record.child_index?).ok()?,
        })
    }

    /// Builds a canonical path only when an API or state codec requests it.
    pub(crate) fn path_for(&self, id: NodeId) -> Option<Path> {
        let mut components = Vec::new();
        let mut current = id;
        while let Some(parent) = self.node(current)?.parent {
            let record = self.node(current)?;
            if let NodeKind::Container(container) = &record.kind
                && let Some(name) = container.name.as_deref().filter(|name| !name.is_empty())
            {
                components.push(Component::new(name));
            } else {
                components.push(Component::new_i(record.child_index? as usize));
            }
            current = parent;
        }
        components.reverse();
        Some(Path::new(&components, false))
    }

    /// Formats directly from parent links without copying component names.
    pub(crate) fn canonical_path_text(&self, id: NodeId) -> Option<String> {
        let mut lineage = Vec::new();
        let mut current = id;
        while self.node(current)?.parent.is_some() {
            lineage.push(current);
            current = self.node(current)?.parent?;
        }
        let mut text = String::new();
        for id in lineage.into_iter().rev() {
            if !text.is_empty() {
                text.push('.');
            }
            let record = self.node(id)?;
            if let NodeKind::Container(container) = &record.kind
                && let Some(name) = container.name.as_deref().filter(|name| !name.is_empty())
            {
                text.push_str(name);
            } else {
                write!(&mut text, "{}", record.child_index?).ok()?;
            }
        }
        Some(text)
    }
}

fn classify(node: &dyn RTObject) -> Result<NodeKind, StoryError> {
    if let Some(container) = node.downcast_ref::<Container>() {
        return Ok(NodeKind::Container(ContainerRecord {
            name: container.name.clone(),
            count_flags: container.get_count_flags(),
            children: 0..0,
            named: 0..0,
        }));
    }
    let kind = if let Some(choice) = node.downcast_ref::<ChoicePoint>() {
        NodeKind::ChoicePoint(ChoiceRecord {
            path: Some(StaticPath::from_runtime(&choice.raw_path())),
            target: None,
            flags: choice.get_flags(),
        })
    } else if let Some(command) = node.downcast_ref::<ControlCommand>() {
        NodeKind::ControlCommand(command.command_type)
    } else if let Some(divert) = node.downcast_ref::<Divert>() {
        NodeKind::Divert(DivertRecord {
            path: divert
                .raw_target_path()
                .as_ref()
                .map(StaticPath::from_runtime),
            target: None,
            variable_name: divert.variable_divert_name.clone(),
            external_args: divert.external_args,
            conditional: divert.is_conditional,
            external: divert.is_external,
            pushes_to_stack: divert.pushes_to_stack,
            stack_push_type: divert.stack_push_type,
        })
    } else if node.is::<Glue>() {
        NodeKind::Glue
    } else if let Some(function) = node.downcast_ref::<NativeFunctionCall>() {
        NodeKind::NativeFunction(function.op)
    } else if let Some(tag) = node.downcast_ref::<Tag>() {
        NodeKind::Tag(tag.get_text().clone())
    } else if let Some(value) = node.downcast_ref::<Value>() {
        let value = match &value.value {
            ValueType::Bool(value) => StaticValue::Bool(*value),
            ValueType::Int(value) => StaticValue::Int(*value),
            ValueType::Float(value) => StaticValue::Float(*value),
            ValueType::String(value) => StaticValue::String(value.string.clone()),
            ValueType::List(value) => StaticValue::List(StaticList::from_runtime(value)),
            ValueType::DivertTarget(value) => StaticValue::DivertTarget {
                path: StaticPath::from_runtime(value),
                target: None,
            },
            ValueType::VariablePointer(value) => StaticValue::VariablePointer {
                name: value.variable_name.clone(),
                context_index: value.context_index,
            },
        };
        NodeKind::Value(value)
    } else if let Some(assignment) = node.downcast_ref::<VariableAssignment>() {
        NodeKind::VariableAssignment {
            name: assignment.variable_name.clone(),
            global: assignment.is_global,
            new_declaration: assignment.is_new_declaration,
        }
    } else if let Some(reference) = node.downcast_ref::<VariableReference>() {
        NodeKind::VariableReference(VariableReferenceRecord {
            name: reference.name.clone(),
            count_path: reference
                .path_for_count
                .as_ref()
                .map(StaticPath::from_runtime),
            count_target: None,
        })
    } else if node.is::<Void>() {
        NodeKind::Void
    } else {
        return Err(StoryError::BadJson(
            "unsupported static runtime object".to_owned(),
        ));
    };
    Ok(kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::list_definitions_origin::ListDefinitionsOrigin;

    #[test]
    fn static_text_paths_match_runtime_paths() {
        for text in [
            "",
            ".",
            "0",
            "knot.stitch.12",
            ".^.next.0",
            "..name",
            "éxito.0003",
            "-1",
        ] {
            let expected = Path::new_with_components_string(Some(text));
            let actual = StaticPath::from_text(text);
            assert_eq!(actual.relative, expected.is_relative(), "{text}");
            assert_eq!(actual.components, expected.components(), "{text}");
        }
    }

    fn empty_lists() -> ListDefinitionsOrigin {
        ListDefinitionsOrigin::new(&mut Vec::new())
    }

    #[test]
    fn flat_structure_keeps_order_and_named_only_containers() {
        let named_only = Container::new(Some("side".to_owned()), 0, Vec::new(), HashMap::new());
        let mut names = HashMap::new();
        names.insert("side".to_owned(), named_only);
        let root = Container::new(
            None,
            0,
            vec![Rc::new(Value::new(7)), Rc::new(Value::new(8))],
            names,
        );
        let data = FlatStoryData::from_legacy_tree(&root, &empty_lists()).unwrap();

        let children = data.children(data.root()).unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(data.canonical_path_text(children[0]).as_deref(), Some("0"));
        assert_eq!(data.canonical_path_text(children[1]).as_deref(), Some("1"));
        let side = data.named_child(data.root(), "side").unwrap();
        assert_eq!(data.canonical_path_text(side).as_deref(), Some("side"));
        assert_eq!(
            data.resolve_path(data.root(), &Path::new_with_components_string(Some("side"))),
            Some(side)
        );
        assert_eq!(
            FlatPointer::start_of(data.container_id(data.root()).unwrap()).resolve(&data),
            Some(children[0])
        );
    }

    #[test]
    fn flat_pointer_advances_without_container_objects() {
        let root = Container::new(
            None,
            0,
            vec![Rc::new(Value::new(1)), Rc::new(Value::new(2))],
            HashMap::new(),
        );
        let data = FlatStoryData::from_legacy_tree(&root, &empty_lists()).unwrap();
        let start = FlatPointer::start_of(data.container_id(data.root()).unwrap());
        assert_eq!(start.increment(&data).unwrap().index, 1);
        assert!(start.increment(&data).unwrap().increment(&data).is_none());
    }

    #[test]
    fn bounded_lazy_paths_only_keep_requested_nodes() {
        let root = Container::new(
            None,
            0,
            vec![Rc::new(Value::new(1)), Rc::new(Value::new(2))],
            HashMap::new(),
        );
        let data = FlatStoryData::from_legacy_tree(&root, &empty_lists()).unwrap();
        let children = data.children(data.root()).unwrap();
        let mut cache = FlatRuntimeCaches::new(1);
        assert!(cache.paths.is_empty());
        assert_eq!(cache.path(&data, children[0]), Some("0"));
        assert_eq!(cache.paths.len(), 1);
        assert_eq!(cache.path(&data, children[1]), Some("1"));
        assert_eq!(cache.paths.len(), 1);
        assert!(cache.paths.contains_key(&children[1]));
    }

    #[test]
    fn both_json_backends_can_produce_a_flat_arena() {
        let json = r#"{"inkVersion":21,"root":["^Hello","done",null],"listDefs":{}}"#;
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        assert_eq!(data.children(data.root()).unwrap().len(), 2);
        assert!(matches!(
            data.node(data.children(data.root()).unwrap()[0])
                .map(|node| &node.kind),
            Some(NodeKind::Value(StaticValue::String(text))) if text == "Hello"
        ));
    }

    #[test]
    fn direct_json_keeps_named_only_nodes_and_does_not_duplicate_inline_names() {
        let json = r##"{"inkVersion":21,"root":[["^inline",{"#n":"inline"}],{"side":["^side",null],"inline":["^shadow",null]}],"listDefs":{}}"##;
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let root = data.root();
        assert_eq!(data.nodes.len(), 5);
        let inline = data.children(root).unwrap()[0];
        assert_eq!(data.named_child(root, "inline"), Some(inline));
        assert_eq!(
            data.canonical_path_text(data.named_child(root, "side").unwrap())
                .as_deref(),
            Some("side")
        );
    }

    #[test]
    fn static_targets_and_saved_counter_paths_use_ids() {
        let target = Container::new(Some("target".to_owned()), 1, Vec::new(), HashMap::new());
        let divert = Rc::new(Divert::new(
            false,
            PushPopType::Function,
            false,
            0,
            false,
            None,
            Some("target"),
        ));
        let choice = Rc::new(ChoicePoint::new(0, "target"));
        let count = Rc::new(VariableReference::from_path_for_count("target"));
        let root = Container::new(None, 0, vec![divert, choice, count, target], HashMap::new());
        let data = FlatStoryData::from_legacy_tree(&root, &empty_lists()).unwrap();
        let children = data.children(data.root()).unwrap();
        let target_id = data.container_id(children[3]).unwrap();

        assert!(matches!(
            &data.node(children[0]).unwrap().kind,
            NodeKind::Divert(divert) if divert.target == Some(target_id.node()) && divert.path.is_none()
        ));
        assert!(matches!(
            &data.node(children[1]).unwrap().kind,
            NodeKind::ChoicePoint(choice) if choice.target == Some(target_id) && choice.path.is_none()
        ));
        assert!(matches!(
            &data.node(children[2]).unwrap().kind,
            NodeKind::VariableReference(reference)
                if reference.count_target == Some(target_id) && reference.count_path.is_none()
        ));

        let mut counters = FlatCounters::new();
        counters.record_visit(target_id);
        let saved = counters.visit_paths_for_save(&data).unwrap();
        assert_eq!(saved.get("target"), Some(&1));
        let mut restored = FlatCounters::new();
        restored.restore_visit_paths(&data, &saved).unwrap();
        assert_eq!(restored.visit_count(target_id), 1);
    }

    #[test]
    fn the_intercept_can_be_flattened_without_retaining_its_tree() {
        let json = include_bytes!("../../conformance-tests/inkfiles/TheIntercept.ink.json");
        let (_, data) = FlatStoryData::from_json_reader(json.as_slice()).unwrap();
        assert!(data.nodes.len() > 100);
        assert!(data.children(data.root()).is_some());
    }

    #[test]
    fn direct_arena_matches_legacy_parser_structure() {
        let json = include_bytes!("../../conformance-tests/inkfiles/TheIntercept.ink.json");
        let (_, direct) = FlatStoryData::from_json_reader(json.as_slice()).unwrap();
        #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
        let (_, root, lists) = crate::json::json_read::load_from_reader(json.as_slice()).unwrap();
        #[cfg(feature = "stream-json-parser")]
        let (_, root, lists) =
            crate::json::json_read_stream::load_from_reader(json.as_slice()).unwrap();
        let legacy = FlatStoryData::from_legacy_tree(&root, &lists).unwrap();
        assert_eq!(direct.nodes.len(), legacy.nodes.len());
        assert_eq!(direct.list_definitions.len(), legacy.list_definitions.len());
        for (index, record) in direct.nodes.iter().enumerate() {
            let id = NodeId(index as u32);
            let path = direct.canonical_path_text(id).unwrap();
            let parsed = Path::new_with_components_string(Some(&path));
            let equivalent = legacy.resolve_path(legacy.root(), &parsed).unwrap();
            let old = legacy.node(equivalent).unwrap();
            assert_eq!(
                core::mem::discriminant(&record.kind),
                core::mem::discriminant(&old.kind),
                "node kind at {path}"
            );
            if let (NodeKind::Container(new), NodeKind::Container(old)) = (&record.kind, &old.kind)
            {
                assert_eq!(new.name, old.name, "name at {path}");
                assert_eq!(new.count_flags, old.count_flags, "flags at {path}");
                assert_eq!(
                    direct.children(id).unwrap().len(),
                    legacy.children(equivalent).unwrap().len(),
                    "children at {path}"
                );
                let new_names: Vec<_> = direct
                    .named_children(id)
                    .unwrap()
                    .iter()
                    .map(|child| child.name.as_str())
                    .collect();
                let old_names: Vec<_> = legacy
                    .named_children(equivalent)
                    .unwrap()
                    .iter()
                    .map(|child| child.name.as_str())
                    .collect();
                assert_eq!(new_names, old_names, "named children at {path}");
            } else if let (NodeKind::ChoicePoint(new), NodeKind::ChoicePoint(old)) =
                (&record.kind, &old.kind)
            {
                assert_eq!(new.flags, old.flags, "choice flags at {path}");
                assert_eq!(
                    new.target
                        .and_then(|id| direct.canonical_path_text(id.node())),
                    old.target
                        .and_then(|id| legacy.canonical_path_text(id.node())),
                    "choice target at {path}"
                );
            } else if let (NodeKind::Divert(new), NodeKind::Divert(old)) = (&record.kind, &old.kind)
            {
                assert_eq!(new.variable_name, old.variable_name, "divert var at {path}");
                assert_eq!(
                    new.external_args, old.external_args,
                    "divert args at {path}"
                );
                assert_eq!(
                    new.conditional, old.conditional,
                    "divert condition at {path}"
                );
                assert_eq!(new.external, old.external, "divert external at {path}");
                assert_eq!(
                    new.pushes_to_stack, old.pushes_to_stack,
                    "divert push at {path}"
                );
                assert_eq!(
                    new.stack_push_type, old.stack_push_type,
                    "divert stack at {path}"
                );
                assert_eq!(
                    new.target.and_then(|id| direct.canonical_path_text(id)),
                    old.target.and_then(|id| legacy.canonical_path_text(id)),
                    "divert target at {path}"
                );
            } else if let (
                NodeKind::Value(StaticValue::DivertTarget { target: new, .. }),
                NodeKind::Value(StaticValue::DivertTarget { target: old, .. }),
            ) = (&record.kind, &old.kind)
            {
                assert_eq!(
                    new.and_then(|id| direct.canonical_path_text(id)),
                    old.and_then(|id| legacy.canonical_path_text(id)),
                    "value target at {path}"
                );
            } else if let (NodeKind::VariableReference(new), NodeKind::VariableReference(old)) =
                (&record.kind, &old.kind)
            {
                assert_eq!(new.name, old.name, "variable name at {path}");
                assert_eq!(
                    new.count_target
                        .and_then(|id| direct.canonical_path_text(id.node())),
                    old.count_target
                        .and_then(|id| legacy.canonical_path_text(id.node())),
                    "read count target at {path}"
                );
            } else {
                assert_eq!(
                    format!("{:?}", record.kind),
                    format!("{:?}", old.kind),
                    "operand at {path}"
                );
            }
        }
    }
}
