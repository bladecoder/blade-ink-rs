//! Common read-only interface for arena and image storage.

use super::*;

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
