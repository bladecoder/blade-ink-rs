//! ID-based structural representation of static Ink content.
//!
//! The JSON readers construct the arena directly; its static nodes hold no `Rc`.

#[allow(unused_imports)]
use crate::prelude::*;

use crate::compat::fmt::Write as _;
use crate::{
    compat::{io::Read, rc::Rc},
    ink_list::InkList,
    ink_list_item::InkListItem,
    native_function_call::Op,
    path::{Component, Path},
    push_pop::PushPopType,
    story::error::StoryError,
    value_type::ValueType,
};

mod arena;
mod storage;
mod view;
pub(crate) use view::StaticStoryView;

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
pub(crate) struct ContainerId(pub(crate) NodeId);

impl ContainerId {
    pub(crate) fn node(self) -> NodeId {
        self.0
    }
}

/// Interpreter pointer without an owning `Rc<Container>`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContentPointer {
    pub(crate) container: Option<ContainerId>,
    pub(crate) index: i32,
}

impl ContentPointer {
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

    pub(crate) fn path(self, data: &impl StaticStoryView) -> Option<Path> {
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
    #[cfg(any(feature = "stream-json-parser", feature = "serde-json-parser"))]
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
#[cfg_attr(
    not(any(feature = "stream-json-parser", feature = "serde-json-parser")),
    allow(dead_code)
)]
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
    #[cfg_attr(
        not(any(feature = "stream-json-parser", feature = "serde-json-parser")),
        allow(dead_code)
    )]
    pub(crate) path: Option<StaticPath>,
    pub(crate) target: Option<ContainerId>,
    pub(crate) flags: i32,
}

#[derive(Debug)]
pub(crate) struct VariableReferenceRecord {
    pub(crate) name: String,
    #[cfg_attr(
        not(any(feature = "stream-json-parser", feature = "serde-json-parser")),
        allow(dead_code)
    )]
    pub(crate) count_path: Option<StaticPath>,
    pub(crate) count_target: Option<ContainerId>,
}

#[derive(Debug)]
#[cfg_attr(
    not(any(feature = "stream-json-parser", feature = "serde-json-parser")),
    allow(dead_code)
)]
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
    #[cfg(feature = "binary-image")]
    Image(&'a str),
}

impl PathView<'_> {
    pub(crate) fn to_runtime(self) -> Path {
        match self {
            Self::Arena(path) => path.runtime_path(),
            #[cfg(feature = "binary-image")]
            Self::Image(path) => Path::new_with_components_string(Some(path)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ListView<'a> {
    Arena(&'a StaticList),
    #[cfg(feature = "binary-image")]
    Image {
        payload: &'a [u8],
        strings: &'a [u8],
    },
}

impl ListView<'_> {
    fn to_runtime(self) -> InkList {
        match self {
            Self::Arena(list) => list.to_runtime(),
            #[cfg(feature = "binary-image")]
            Self::Image { payload, strings } => {
                fn word(bytes: &[u8], at: usize) -> usize {
                    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
                }
                fn string(bytes: &[u8], offset: usize, len: usize) -> &str {
                    core::str::from_utf8(&bytes[offset..offset + len]).unwrap()
                }
                let item_count = word(payload, 0);
                let origin_count = word(payload, 4);
                let mut list = InkList::new();
                for index in 0..item_count {
                    let at = 8 + index * 20;
                    let origin = if word(payload, at) == u32::MAX as usize {
                        None
                    } else {
                        Some(string(strings, word(payload, at), word(payload, at + 4)).to_owned())
                    };
                    let name = string(strings, word(payload, at + 8), word(payload, at + 12));
                    let value = word(payload, at + 16) as u32 as i32;
                    list.items
                        .insert(InkListItem::new(origin, name.to_owned()), value);
                }
                let mut origins = Vec::with_capacity(origin_count);
                for index in 0..origin_count {
                    let at = 8 + item_count * 20 + index * 8;
                    origins
                        .push(string(strings, word(payload, at), word(payload, at + 4)).to_owned());
                }
                list.set_initial_origin_names(origins);
                list
            }
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
        name: Option<&'a str>,
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
pub(crate) struct StoryData {
    pub(crate) nodes: Vec<NodeRecord>,
    pub(crate) children: Vec<NodeId>,
    pub(crate) named: Vec<NamedChild>,
    pub(crate) list_definitions: Vec<StaticListDefinition>,
}

/// Storage selected when a story is opened. The image arm retains only a
/// static byte slice and its validated section bounds.
pub(crate) enum StoryContent {
    Arena(StoryData),
    #[cfg(feature = "binary-image")]
    Image(crate::image::ImageView),
}

#[derive(Clone, Copy)]
pub(crate) enum LoadPhase {
    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    JsonDecoded,
    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    ArenaBuilt,
    #[cfg(feature = "stream-json-parser")]
    StreamParsedAndBuilt,
    #[cfg(any(feature = "stream-json-parser", feature = "serde-json-parser"))]
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
pub struct LoadProfile {
    pub json_decode: Option<core::time::Duration>,
    pub arena_build: Option<core::time::Duration>,
    pub stream_parse_and_build: Option<core::time::Duration>,
    pub targets: core::time::Duration,
    pub runtime: core::time::Duration,
}

#[cfg(feature = "load-profile")]
pub(crate) struct TimedLoadObserver {
    last: std::time::Instant,
    pub(crate) profile: LoadProfile,
}

#[cfg(feature = "load-profile")]
impl TimedLoadObserver {
    pub(crate) fn new() -> Self {
        Self {
            last: std::time::Instant::now(),
            profile: LoadProfile::default(),
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
            #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
            LoadPhase::JsonDecoded => self.profile.json_decode = Some(elapsed),
            #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
            LoadPhase::ArenaBuilt => self.profile.arena_build = Some(elapsed),
            #[cfg(feature = "stream-json-parser")]
            LoadPhase::StreamParsedAndBuilt => self.profile.stream_parse_and_build = Some(elapsed),
            #[cfg(any(feature = "stream-json-parser", feature = "serde-json-parser"))]
            LoadPhase::TargetsResolved => self.profile.targets = elapsed,
            LoadPhase::RuntimeInitialized => self.profile.runtime = elapsed,
        }
    }
}

#[cfg(all(
    test,
    any(feature = "stream-json-parser", feature = "serde-json-parser")
))]
mod tests {
    use super::*;

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

    #[test]
    fn both_json_backends_can_produce_an_arena() {
        let json = r#"{"inkVersion":21,"root":["^Hello","done",null],"listDefs":{}}"#;
        let (_, data) = StoryData::from_json_reader(json.as_bytes()).unwrap();
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
        let (_, data) = StoryData::from_json_reader(json.as_bytes()).unwrap();
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
    fn the_intercept_can_be_flattened_without_retaining_its_tree() {
        let json = include_bytes!("../../conformance-tests/inkfiles/TheIntercept.ink.json");
        let (_, data) = StoryData::from_json_reader(json.as_slice()).unwrap();
        assert!(data.nodes.len() > 100);
        assert!(data.children(data.root()).is_some());
    }
}
