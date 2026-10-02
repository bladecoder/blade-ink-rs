//! Arena loading and static target resolution.

use super::*;

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
            crate::json::story::read_stream::load_from_reader(reader, observer)
        }
        #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
        crate::json::story::read_dom::load_from_reader(reader, observer)
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
