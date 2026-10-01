//! ID-based structural representation of static Ink content.
//!
//! The JSON readers construct the arena directly; its static nodes hold no `Rc`.

#[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
mod story_read_serde;

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
    story_error::StoryError,
    value_type::ValueType,
};

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

/// Read-only node access used by the interpreter. Views borrow operands from
/// their storage and do not allocate as instructions are fetched.
pub(crate) trait StaticStoryView {
    fn node_count(&self) -> usize;
    fn node_view(&self, id: NodeId) -> Option<NodeView<'_>>;
    fn child_count(&self, id: NodeId) -> Option<usize>;
    fn child_at(&self, id: NodeId, index: usize) -> Option<NodeId>;
    fn named_child_count(&self, id: NodeId) -> Option<usize>;
    fn named_child_at(&self, id: NodeId, index: usize) -> Option<NamedChildView<'_>>;
    fn list_item(&self, name: &str) -> Option<(InkListItem, i32)>;
    fn list_definition_count(&self) -> usize;
    fn list_definition_name(&self, index: usize) -> Option<&str>;
    fn list_definition_item_count(&self, index: usize) -> Option<usize>;
    fn list_definition_item_at(&self, index: usize, item: usize) -> Option<(&str, i32)>;

    fn root(&self) -> NodeId {
        NodeId(0)
    }

    fn container_id(&self, id: NodeId) -> Option<ContainerId> {
        matches!(self.node_view(id)?.kind, NodeKindView::Container { .. })
            .then_some(ContainerId(id))
    }

    fn named_child(&self, id: NodeId, name: &str) -> Option<NodeId> {
        self.find_named_child(id, name)
    }

    fn resolve_path(&self, origin: NodeId, path: &Path) -> Option<NodeId> {
        let (mut current, start) = if path.is_relative() {
            match self.node_view(origin)?.kind {
                NodeKindView::Container { .. } => (origin, 0),
                _ => (self.node_view(origin)?.parent?, 1),
            }
        } else {
            (self.root(), 0)
        };
        for component in path.components().iter().skip(start) {
            if component.is_parent() {
                current = self.node_view(current)?.parent?;
            } else if let Some(index) = component.index {
                current = self.child_at(current, index)?;
            } else {
                current = self.named_child(current, component.name.as_deref()?)?;
            }
        }
        Some(current)
    }

    fn pointer_at_path(&self, path: &Path) -> Option<ContentPointer> {
        if path.is_empty() {
            return Some(ContentPointer::NULL);
        }
        let last = path.get_last_component()?;
        if let Some(index) = last.index {
            let components: Vec<_> = (0..path.len() - 1)
                .map(|position| path.get_component(position).cloned())
                .collect::<Option<_>>()?;
            let parent_path = Path::new(&components, path.is_relative());
            let parent = self.resolve_path(self.root(), &parent_path)?;
            Some(ContentPointer {
                container: Some(self.container_id(parent)?),
                index: i32::try_from(index).ok()?,
            })
        } else {
            let target = self.resolve_path(self.root(), path)?;
            Some(ContentPointer::at_container(self.container_id(target)?))
        }
    }

    fn pointer_for(&self, id: NodeId) -> Option<ContentPointer> {
        if let Some(container) = self.container_id(id) {
            return Some(ContentPointer::at_container(container));
        }
        let node = self.node_view(id)?;
        Some(ContentPointer {
            container: Some(self.container_id(node.parent?)?),
            index: i32::try_from(node.child_index?).ok()?,
        })
    }

    fn path_for(&self, id: NodeId) -> Option<Path> {
        let mut components = Vec::new();
        let mut current = id;
        while let Some(parent) = self.node_view(current)?.parent {
            let node = self.node_view(current)?;
            if let NodeKindView::Container {
                name: Some(name), ..
            } = node.kind
                && !name.is_empty()
            {
                components.push(Component::new(name));
            } else {
                components.push(Component::new_i(node.child_index? as usize));
            }
            current = parent;
        }
        components.reverse();
        Some(Path::new(&components, false))
    }

    fn canonical_path_text(&self, id: NodeId) -> Option<String> {
        let mut lineage = Vec::new();
        let mut current = id;
        while self.node_view(current)?.parent.is_some() {
            lineage.push(current);
            current = self.node_view(current)?.parent?;
        }
        let mut text = String::new();
        for id in lineage.into_iter().rev() {
            if !text.is_empty() {
                text.push('.');
            }
            let node = self.node_view(id)?;
            if let NodeKindView::Container {
                name: Some(name), ..
            } = node.kind
                && !name.is_empty()
            {
                text.push_str(name);
            } else {
                write!(&mut text, "{}", node.child_index?).ok()?;
            }
        }
        Some(text)
    }

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

    fn list_item(&self, name: &str) -> Option<(InkListItem, i32)> {
        self.as_ref().list_item(name)
    }

    fn list_definition_count(&self) -> usize {
        self.as_ref().list_definition_count()
    }
    fn list_definition_name(&self, index: usize) -> Option<&str> {
        self.as_ref().list_definition_name(index)
    }
    fn list_definition_item_count(&self, index: usize) -> Option<usize> {
        self.as_ref().list_definition_item_count(index)
    }
    fn list_definition_item_at(&self, index: usize, item: usize) -> Option<(&str, i32)> {
        self.as_ref().list_definition_item_at(index, item)
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

impl StoryContent {
    pub(crate) fn list_definition_index(&self, name: &str) -> Option<usize> {
        let mut low = 0;
        let mut high = self.list_definition_count();
        while low < high {
            let middle = low + (high - low) / 2;
            match self.list_definition_name(middle)?.cmp(name) {
                core::cmp::Ordering::Less => low = middle + 1,
                core::cmp::Ordering::Greater => high = middle,
                core::cmp::Ordering::Equal => return Some(middle),
            }
        }
        None
    }
}

impl StaticStoryView for StoryContent {
    fn node_count(&self) -> usize {
        match self {
            Self::Arena(data) => data.node_count(),
            #[cfg(feature = "binary-image")]
            Self::Image(data) => data.node_count(),
        }
    }

    fn node_view(&self, id: NodeId) -> Option<NodeView<'_>> {
        match self {
            Self::Arena(data) => data.node_view(id),
            #[cfg(feature = "binary-image")]
            Self::Image(data) => data.node_view(id),
        }
    }

    fn child_count(&self, id: NodeId) -> Option<usize> {
        match self {
            Self::Arena(data) => data.child_count(id),
            #[cfg(feature = "binary-image")]
            Self::Image(data) => data.child_count(id),
        }
    }

    fn child_at(&self, id: NodeId, index: usize) -> Option<NodeId> {
        match self {
            Self::Arena(data) => data.child_at(id, index),
            #[cfg(feature = "binary-image")]
            Self::Image(data) => data.child_at(id, index),
        }
    }

    fn named_child_count(&self, id: NodeId) -> Option<usize> {
        match self {
            Self::Arena(data) => data.named_child_count(id),
            #[cfg(feature = "binary-image")]
            Self::Image(data) => data.named_child_count(id),
        }
    }

    fn named_child_at(&self, id: NodeId, index: usize) -> Option<NamedChildView<'_>> {
        match self {
            Self::Arena(data) => data.named_child_at(id, index),
            #[cfg(feature = "binary-image")]
            Self::Image(data) => data.named_child_at(id, index),
        }
    }

    fn list_item(&self, name: &str) -> Option<(InkListItem, i32)> {
        match self {
            Self::Arena(data) => data.list_item(name),
            #[cfg(feature = "binary-image")]
            Self::Image(data) => data.list_item(name),
        }
    }

    fn list_definition_count(&self) -> usize {
        match self {
            Self::Arena(data) => data.list_definition_count(),
            #[cfg(feature = "binary-image")]
            Self::Image(data) => data.list_definition_count(),
        }
    }
    fn list_definition_name(&self, index: usize) -> Option<&str> {
        match self {
            Self::Arena(data) => data.list_definition_name(index),
            #[cfg(feature = "binary-image")]
            Self::Image(data) => data.list_definition_name(index),
        }
    }
    fn list_definition_item_count(&self, index: usize) -> Option<usize> {
        match self {
            Self::Arena(data) => data.list_definition_item_count(index),
            #[cfg(feature = "binary-image")]
            Self::Image(data) => data.list_definition_item_count(index),
        }
    }
    fn list_definition_item_at(&self, index: usize, item: usize) -> Option<(&str, i32)> {
        match self {
            Self::Arena(data) => data.list_definition_item_at(index, item),
            #[cfg(feature = "binary-image")]
            Self::Image(data) => data.list_definition_item_at(index, item),
        }
    }
}

impl StaticStoryView for StoryData {
    fn node_count(&self) -> usize {
        self.nodes.len()
    }

    fn node_view(&self, id: NodeId) -> Option<NodeView<'_>> {
        let node = self.nodes.get(id.index())?;
        let kind = match &node.kind {
            NodeKind::Container(record) => NodeKindView::Container {
                count_flags: record.count_flags,
                name: record.name.as_deref(),
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

    fn list_item(&self, name: &str) -> Option<(InkListItem, i32)> {
        StoryData::list_item(self, name)
    }

    fn list_definition_count(&self) -> usize {
        self.list_definitions.len()
    }
    fn list_definition_name(&self, index: usize) -> Option<&str> {
        Some(&self.list_definitions.get(index)?.name)
    }
    fn list_definition_item_count(&self, index: usize) -> Option<usize> {
        Some(self.list_definitions.get(index)?.items.len())
    }
    fn list_definition_item_at(&self, index: usize, item: usize) -> Option<(&str, i32)> {
        let (name, value) = self.list_definitions.get(index)?.items.get(item)?;
        Some((name, *value))
    }
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

impl StoryData {
    /// Builds the arena directly from compiled Ink JSON using the selected
    /// JSON reader.
    #[cfg(any(
        all(
            test,
            any(feature = "stream-json-parser", feature = "serde-json-parser")
        ),
        all(feature = "binary-image", feature = "std")
    ))]
    pub(crate) fn from_json_reader(reader: impl Read) -> Result<(i32, Self), StoryError> {
        Self::from_json_reader_observed(reader, &mut NoopLoadObserver)
    }

    pub(crate) fn from_json_reader_observed(
        reader: impl Read,
        observer: &mut impl LoadObserver,
    ) -> Result<(i32, Self), StoryError> {
        #[cfg(not(any(feature = "stream-json-parser", feature = "serde-json-parser")))]
        {
            let _ = (reader, observer);
            Err(StoryError::BadArgument(
                "JSON story parser is not enabled".to_owned(),
            ))
        }
        #[cfg(feature = "stream-json-parser")]
        {
            crate::json::story_read_stream::load_from_reader(reader, observer)
        }
        #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
        story_read_serde::load_from_reader(reader, observer)
    }

    #[cfg(any(feature = "stream-json-parser", feature = "serde-json-parser"))]
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

    #[cfg(any(feature = "stream-json-parser", feature = "serde-json-parser"))]
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

    #[cfg(any(feature = "stream-json-parser", feature = "serde-json-parser"))]
    pub(crate) fn container_id(&self, id: NodeId) -> Option<ContainerId> {
        matches!(&self.node(id)?.kind, NodeKind::Container(_)).then_some(ContainerId(id))
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

    #[cfg(any(feature = "stream-json-parser", feature = "serde-json-parser"))]
    pub(crate) fn named_child(&self, id: NodeId, name: &str) -> Option<NodeId> {
        let named = self.named_children(id)?;
        let index = named
            .binary_search_by(|entry| entry.name.as_str().cmp(name))
            .ok()?;
        Some(named[index].node)
    }

    #[cfg(any(feature = "stream-json-parser", feature = "serde-json-parser"))]
    fn resolve_static_path(&self, origin: NodeId, path: &StaticPath) -> Option<NodeId> {
        self.resolve_components(origin, &path.components, path.relative)
    }

    #[cfg(any(feature = "stream-json-parser", feature = "serde-json-parser"))]
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

    /// Formats directly from parent links without copying component names.
    #[cfg(any(
        all(
            test,
            any(feature = "stream-json-parser", feature = "serde-json-parser")
        ),
        all(feature = "binary-image", feature = "std")
    ))]
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
