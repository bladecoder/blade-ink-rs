//! Storage-backed access to story nodes and lists.

use super::*;

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
