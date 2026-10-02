//! Story paths and current output.

use super::*;

impl Runtime {
    pub(crate) fn choose_path(&mut self, path: &Path) -> Result<(), StoryError> {
        let pointer = self.data.pointer_at_path(path).ok_or_else(|| {
            StoryError::BadArgument(format!("story path '{path}' does not resolve"))
        })?;
        let previous = self.callstack.borrow().current_pointer();
        self.record_entered_ancestors(previous, pointer);
        if let Some(container) = pointer.container.filter(|_| pointer.index < 0) {
            let flags = match self.data.node_view(container.node()).unwrap().kind {
                NodeKindView::Container { count_flags, .. } => count_flags,
                _ => 0,
            };
            if flags & 1 != 0 {
                self.counters.record_visit(container);
            }
            if flags & 2 != 0 {
                self.counters
                    .record_turn(container, self.current_turn_index);
            }
            self.previsited_container = Some(container);
        }
        self.callstack.borrow_mut().set_current_pointer(pointer);
        Ok(())
    }

    pub(super) fn record_entered_ancestors(
        &mut self,
        previous: ContentPointer,
        target: ContentPointer,
    ) {
        let Some(target_id) = target.resolve(&self.data) else {
            return;
        };
        let mut previous_ancestors = Vec::new();
        let mut old = previous.resolve(&self.data);
        while let Some(id) = old {
            previous_ancestors.push(id);
            old = self.data.node_view(id).and_then(|node| node.parent);
        }
        let mut child = target_id;
        let mut entered_at_start = true;
        while let Some(parent) = self.data.node_view(child).and_then(|node| node.parent) {
            let child_index = self.data.node_view(child).and_then(|node| node.child_index);
            entered_at_start &= child_index == Some(0);
            if previous_ancestors.contains(&parent) {
                break;
            }
            if let NodeKindView::Container { count_flags, .. } =
                self.data.node_view(parent).unwrap().kind
                && (count_flags & 4 == 0 || entered_at_start)
            {
                let container = self.data.container_id(parent).unwrap();
                if count_flags & 1 != 0 {
                    self.counters.record_visit(container);
                }
                if count_flags & 2 != 0 {
                    self.counters
                        .record_turn(container, self.current_turn_index);
                }
            }
            child = parent;
        }
    }

    pub(crate) fn current_path_string(&self) -> Option<String> {
        self.callstack
            .borrow()
            .current_pointer()
            .path(&self.data)
            .map(|path| path.to_string())
    }

    pub(crate) fn visit_count_at_path_string(&self, path: &str) -> Result<i32, StoryError> {
        let path = Path::new_with_components_string(Some(path));
        let container = self
            .data
            .resolve_path(self.data.root(), &path)
            .and_then(|id| self.data.container_id(id))
            .ok_or_else(|| {
                StoryError::BadArgument(format!("story path '{path}' does not resolve"))
            })?;
        Ok(self.counters.visit_count(container))
    }

    pub(crate) fn current_tags(&self) -> &[String] {
        &self.current_tags
    }

    pub(crate) fn public_choices(&self) -> Vec<Rc<Choice>> {
        if self.can_continue() {
            return Vec::new();
        }
        self.choices
            .iter()
            .filter(|choice| !choice.is_invisible_default)
            .enumerate()
            .map(|(index, choice)| {
                let target = self
                    .data
                    .canonical_path_text(choice.target.node())
                    .unwrap_or_default();
                let source = self
                    .data
                    .canonical_path_text(choice.source)
                    .unwrap_or_default();
                Rc::new(Choice::new_from_json(
                    &target,
                    source,
                    &choice.text,
                    index,
                    choice.thread.thread_index,
                    choice.tags.clone(),
                ))
            })
            .collect()
    }

    pub(crate) fn build_string_of_hierarchy(&self) -> String {
        fn append(
            data: &impl StaticStoryView,
            id: NodeId,
            current: Option<NodeId>,
            depth: usize,
            text: &mut String,
        ) {
            let Some(node) = data.node_view(id) else {
                return;
            };
            for _ in 0..depth {
                text.push_str("  ");
            }
            text.push_str(&format!("{:?}", node.kind));
            if Some(id) == current {
                text.push_str(" <---");
            }
            text.push('\n');
            if let Some(count) = data.child_count(id) {
                for index in 0..count {
                    let child = data.child_at(id, index).unwrap();
                    append(data, child, current, depth + 1, text);
                }
                if let Some(named_count) = data.named_child_count(id) {
                    for index in 0..named_count {
                        let entry = data.named_child_at(id, index).unwrap();
                        if data
                            .node_view(entry.node)
                            .is_some_and(|child| child.child_index.is_none())
                        {
                            append(data, entry.node, current, depth + 1, text);
                        }
                    }
                }
            }
        }
        let mut text = String::new();
        let current = self
            .callstack
            .borrow()
            .current_pointer()
            .resolve(&self.data);
        append(&*self.data, self.data.root(), current, 0, &mut text);
        text
    }

    pub(crate) fn current_text(&self) -> String {
        clean_output_whitespace(&self.output)
    }
}
