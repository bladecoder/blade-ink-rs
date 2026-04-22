use std::{collections::HashMap, rc::Rc};

use crate::{
    error::CompilerError,
    runtime_export::{
        ExportState, PendingContainer, PendingItem, Scope, collect_current_level_named_paths,
        collect_loose_choice_ends, command, divert_object, export_nodes_with_paths, has_terminal,
        has_weave_content, register_named_path,
    },
};

use bladeink::CommandType;

use super::{
    ChoiceNode, ChoiceNodeSpec, ContentList, GatherNode, GatherNodeSpec, ObjectKind, ParsedNode,
    ParsedNodeKind, ParsedObject, Story,
};

#[derive(Debug, Clone)]
pub struct Choice {
    object: ParsedObject,
    identifier: Option<String>,
    indentation_depth: usize,
    has_weave_style_inline_brackets: bool,
    once_only: bool,
    is_invisible_default: bool,
    start_content: Option<ContentList>,
    choice_only_content: Option<ContentList>,
    inner_content: ContentList,
}

impl Choice {
    pub fn new(
        indentation_depth: usize,
        once_only: bool,
        identifier: Option<String>,
        start_content: Option<ContentList>,
        choice_only_content: Option<ContentList>,
        inner_content: ContentList,
    ) -> Self {
        let mut choice = Self {
            object: ParsedObject::new(ObjectKind::Choice),
            identifier,
            indentation_depth,
            has_weave_style_inline_brackets: choice_only_content.is_some(),
            once_only,
            is_invisible_default: false,
            start_content,
            choice_only_content,
            inner_content,
        };
        choice.set_content_parents();
        choice
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        &mut self.object
    }

    pub fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
    }

    pub fn indentation_depth(&self) -> usize {
        self.indentation_depth
    }

    pub fn has_weave_style_inline_brackets(&self) -> bool {
        self.has_weave_style_inline_brackets
    }

    pub fn once_only(&self) -> bool {
        self.once_only
    }

    pub fn is_invisible_default(&self) -> bool {
        self.is_invisible_default
    }

    pub fn set_invisible_default(&mut self, value: bool) {
        self.is_invisible_default = value;
    }

    pub fn start_content(&self) -> Option<&ContentList> {
        self.start_content.as_ref()
    }

    pub fn choice_only_content(&self) -> Option<&ContentList> {
        self.choice_only_content.as_ref()
    }

    pub fn inner_content(&self) -> &ContentList {
        &self.inner_content
    }

    fn set_content_parents(&mut self) {
        if let Some(start) = self.start_content.as_mut() {
            start.object_mut().set_parent(&self.object);
            self.object.add_content_ref(start.object().reference());
        }
        if let Some(choice_only) = self.choice_only_content.as_mut() {
            choice_only.object_mut().set_parent(&self.object);
            self.object
                .add_content_ref(choice_only.object().reference());
        }
        self.inner_content.object_mut().set_parent(&self.object);
        self.object
            .add_content_ref(self.inner_content.object().reference());
    }
}

#[derive(Debug, Clone)]
pub struct Gather {
    object: ParsedObject,
    identifier: Option<String>,
    indentation_depth: usize,
    content: Option<ContentList>,
}

impl Gather {
    pub fn new(
        indentation_depth: usize,
        identifier: Option<String>,
        mut content: Option<ContentList>,
    ) -> Self {
        let mut object = ParsedObject::new(ObjectKind::Gather);
        if let Some(content) = content.as_mut() {
            content.object_mut().set_parent(&object);
            object.add_content_ref(content.object().reference());
        }
        Self {
            object,
            identifier,
            indentation_depth,
            content,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        &mut self.object
    }

    pub fn identifier(&self) -> Option<&str> {
        self.identifier.as_deref()
    }

    pub fn indentation_depth(&self) -> usize {
        self.indentation_depth
    }

    pub fn content(&self) -> Option<&ContentList> {
        self.content.as_ref()
    }
}

#[derive(Debug, Clone)]
pub enum WeaveElement {
    Content(ContentList),
    Choice(Choice),
    Gather(Gather),
    NestedWeave(Weave),
}

#[derive(Debug, Clone)]
pub struct Weave {
    object: ParsedObject,
    base_indentation_index: usize,
    elements: Vec<WeaveElement>,
}

#[derive(Debug, Clone)]
pub enum StructuredWeaveEntryKind {
    Content(Vec<ParsedNode>),
    Choice(ChoiceNodeSpec),
    Gather(GatherNodeSpec),
}

#[derive(Debug, Clone)]
pub struct StructuredWeaveEntry {
    kind: StructuredWeaveEntryKind,
    nested: Option<Box<StructuredWeave>>,
}

impl StructuredWeaveEntry {
    pub fn kind(&self) -> &StructuredWeaveEntryKind {
        &self.kind
    }

    pub fn nested(&self) -> Option<&StructuredWeave> {
        self.nested.as_deref()
    }
}

#[derive(Debug, Clone)]
pub struct StructuredWeave {
    base_depth: usize,
    entries: Vec<StructuredWeaveEntry>,
}

impl StructuredWeave {
    pub fn from_nodes(nodes: &[ParsedNode]) -> Option<Self> {
        let base_depth = nodes
            .iter()
            .filter_map(weave_depth)
            .min()
            .unwrap_or(1);

        let mut entries = Vec::new();
        let mut pending_content = Vec::new();
        let mut index = 0usize;

        while index < nodes.len() {
            let node = &nodes[index];

            if let Some(depth) = weave_depth(node)
                && depth > base_depth
            {
                let nested_start = index;
                index += 1;
                while index < nodes.len() {
                    if let Some(inner_depth) = weave_depth(&nodes[index])
                        && inner_depth <= base_depth
                    {
                        break;
                    }
                    index += 1;
                }

                let nested = Self::from_nodes(&nodes[nested_start..index])
                    .map(Box::new)
                    .expect("nested weave slice should structure");
                if entries.is_empty() {
                    entries.push(StructuredWeaveEntry {
                        kind: StructuredWeaveEntryKind::Content(Vec::new()),
                        nested: Some(nested),
                    });
                } else {
                    entries
                        .last_mut()
                        .expect("entry exists")
                        .nested = Some(nested);
                }
                continue;
            }

            match node.kind() {
                ParsedNodeKind::Choice => {
                    if !pending_content.is_empty() {
                        entries.push(StructuredWeaveEntry {
                            kind: StructuredWeaveEntryKind::Content(std::mem::take(&mut pending_content)),
                            nested: None,
                        });
                    }
                    entries.push(StructuredWeaveEntry {
                        kind: StructuredWeaveEntryKind::Choice(
                            ChoiceNodeSpec::from_node(node).expect("choice node spec"),
                        ),
                        nested: None,
                    });
                }
                ParsedNodeKind::GatherPoint | ParsedNodeKind::GatherLabel => {
                    if !pending_content.is_empty() {
                        entries.push(StructuredWeaveEntry {
                            kind: StructuredWeaveEntryKind::Content(std::mem::take(&mut pending_content)),
                            nested: None,
                        });
                    }
                    entries.push(StructuredWeaveEntry {
                        kind: StructuredWeaveEntryKind::Gather(
                            GatherNodeSpec::from_node(node).expect("gather node spec"),
                        ),
                        nested: None,
                    });
                }
                _ => pending_content.push(node.clone()),
            }

            index += 1;
        }

        if !pending_content.is_empty() {
            entries.push(StructuredWeaveEntry {
                kind: StructuredWeaveEntryKind::Content(pending_content),
                nested: None,
            });
        }

        Some(Self { base_depth, entries })
    }

    pub fn entries(&self) -> &[StructuredWeaveEntry] {
        &self.entries
    }

    pub fn base_depth(&self) -> usize {
        self.base_depth
    }

    pub(crate) fn build_runtime_pending(
        &self,
        state: &ExportState,
        path_prefix: &str,
        scope: Scope<'_>,
        story: &Story,
        is_root: bool,
        inherited_named_paths: &HashMap<String, String>,
    ) -> Result<Rc<PendingContainer>, CompilerError> {
        let root = PendingContainer::new(state, path_prefix, None, 0);
        let mut current = root.clone();
        let mut loose_ends: Vec<Rc<PendingContainer>> = Vec::new();
        let mut previous_choice: Option<Rc<PendingContainer>> = None;
        let mut add_to_previous_choice = false;
        let mut has_seen_choice_in_section = false;
        let mut choice_count = 0usize;
        let mut gather_count = 0usize;
        let mut named_paths = inherited_named_paths.clone();
        named_paths.extend(collect_current_level_named_paths(path_prefix, self.entries()));

        for entry in self.entries() {
            match entry.kind() {
                StructuredWeaveEntryKind::Content(nodes) => {
                    let target = if add_to_previous_choice {
                        previous_choice.clone().expect("previous choice container")
                    } else {
                        current.clone()
                    };
                    let content = export_nodes_with_paths(
                        state,
                        nodes,
                        scope,
                        story,
                        Some(&named_paths),
                        Some(&target.path),
                        0,
                    )?;
                    for item in content {
                        target.push_object(item);
                    }
                }
                StructuredWeaveEntryKind::Choice(choice) => {
                    let choice_key = format!("c-{choice_count}");
                    let choice_path = format!("{}.{}", current.path, choice_key);
                    let choice_container = PendingContainer::new(
                        state,
                        &choice_path,
                        Some(choice_key.clone()),
                        story.weave_count_flags(),
                    );
                    current.add_named(choice_key.clone(), choice_container.clone());

                    choice.export_runtime(
                        state,
                        choice_count,
                        &current.path,
                        current.clone(),
                        choice_container.clone(),
                        scope,
                        story,
                        &named_paths,
                    )?;

                    if let Some(name) = choice.identifier() {
                        register_named_path(&mut named_paths, scope, name, &choice_path);
                    }

                    if !choice.inner_content().is_empty() {
                        if has_weave_content(choice.inner_content()) {
                            let weave_structure = StructuredWeave::from_nodes(choice.inner_content())
                                .expect("nested choice weave structure");
                            let weave = weave_structure.build_runtime_pending(
                                state,
                                &format!(
                                    "{}.{}",
                                    choice_container.path,
                                    choice_container.next_content_index()
                                ),
                                scope,
                                story,
                                false,
                                &named_paths,
                            )?;
                            absorb_pending_container(&choice_container, &weave);
                        } else {
                            let child_content = export_nodes_with_paths(
                                state,
                                choice.inner_content(),
                                scope,
                                story,
                                Some(&named_paths),
                                Some(&choice_container.path),
                                0,
                            )?;
                            for item in child_content {
                                choice_container.push_object(item);
                            }
                        }
                    }

                    choice_container.push_object(crate::runtime_export::rt_value("\n"));

                    if choice.is_invisible_default() || !has_terminal(choice.inner_content()) {
                        loose_ends.push(choice_container.clone());
                        previous_choice = Some(choice_container);
                        add_to_previous_choice = true;
                    } else {
                        previous_choice = None;
                        add_to_previous_choice = false;
                    }

                    has_seen_choice_in_section = true;
                    choice_count += 1;
                }
                StructuredWeaveEntryKind::Gather(gather) => {
                    let auto_enter = !has_seen_choice_in_section;
                    let is_named_gather = gather.identifier().is_some();
                    let gather_name = gather
                        .identifier()
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| format!("g-{gather_count}"));
                    let gather_path = if auto_enter {
                        format!("{}.{}", current.path, gather_name)
                    } else {
                        format!("{path_prefix}.{}", gather_name)
                    };

                    for loose_end in &loose_ends {
                        loose_end.push_object(divert_object(&gather_path));
                    }
                    loose_ends.clear();

                    has_seen_choice_in_section = false;
                    add_to_previous_choice = false;
                    previous_choice = None;
                    let gather_container = PendingContainer::new(
                        state,
                        &gather_path,
                        Some(gather_name.clone()),
                        story.weave_count_flags(),
                    );

                    if auto_enter {
                        current.push_container(gather_container.clone());
                    } else {
                        root.add_named(gather_name.clone(), gather_container.clone());
                    }

                    if !gather.content().is_empty() {
                        if has_weave_content(gather.content()) {
                            let weave_structure = StructuredWeave::from_nodes(gather.content())
                                .expect("nested gather weave structure");
                            let weave = weave_structure.build_runtime_pending(
                                state,
                                &format!(
                                    "{}.{}",
                                    gather_container.path,
                                    gather_container.next_content_index()
                                ),
                                scope,
                                story,
                                false,
                                &named_paths,
                            )?;
                            absorb_pending_container(&gather_container, &weave);
                        } else {
                            let child_content = export_nodes_with_paths(
                                state,
                                gather.content(),
                                scope,
                                story,
                                Some(&named_paths),
                                Some(&gather_container.path),
                                0,
                            )?;
                            for item in child_content {
                                gather_container.push_object(item);
                            }
                        }
                    }

                    current = gather_container;
                    if let Some(name) = gather.identifier() {
                        register_named_path(&mut named_paths, scope, name, &gather_path);
                    }
                    if !is_named_gather {
                        gather_count += 1;
                    }
                }
            }

            if let Some(nested) = entry.nested() {
                let target = if add_to_previous_choice {
                    previous_choice.clone().expect("previous choice container")
                } else {
                    current.clone()
                };
                let nested_path = format!("{}.{}", target.path, target.next_content_index());
                let nested_pending = nested.build_runtime_pending(
                    state,
                    &nested_path,
                    scope,
                    story,
                    false,
                    &named_paths,
                )?;
                let bubbled_loose_ends = collect_loose_choice_ends(&nested_pending);
                target.push_container(nested_pending);

                if let Some(previous_choice_container) = previous_choice.clone() {
                    loose_ends.retain(|candidate| !Rc::ptr_eq(candidate, &previous_choice_container));
                    add_to_previous_choice = false;
                    previous_choice = None;
                }

                loose_ends.extend(bubbled_loose_ends);
            }
        }

        if is_root {
            let final_gather_name = format!("g-{gather_count}");
            let auto_enter = !has_seen_choice_in_section;
            let final_gather_path = if auto_enter {
                format!("{}.{}", current.path, final_gather_name)
            } else {
                format!("{path_prefix}.{}", final_gather_name)
            };

            for loose_end in &loose_ends {
                loose_end.push_object(divert_object(&final_gather_path));
            }

            let final_gather = PendingContainer::new(
                state,
                final_gather_path,
                Some(final_gather_name.clone()),
                story.weave_count_flags(),
            );
            final_gather.push_object(command(CommandType::Done));
            if auto_enter {
                current.push_container(final_gather);
            } else {
                root.add_named(final_gather_name, final_gather);
            }
        }

        Ok(root)
    }
}

fn absorb_pending_container(target: &Rc<PendingContainer>, source: &Rc<PendingContainer>) {
    for item in source.content.borrow().iter().cloned() {
        match item {
            PendingItem::Object(object) => target.push_object(object),
            PendingItem::Container(container) => target.push_container(container),
        }
    }
    for (key, container) in source.named.borrow().iter().cloned() {
        target.add_named(key, container);
    }
}

fn weave_depth(node: &ParsedNode) -> Option<usize> {
    if let Some(choice) = ChoiceNode::from_node(node) {
        return Some(choice.indentation_depth());
    }

    GatherNode::from_node(node).map(GatherNode::indentation_depth)
}

impl Weave {
    pub fn new(base_indentation_index: usize) -> Self {
        Self {
            object: ParsedObject::new(ObjectKind::Weave),
            base_indentation_index,
            elements: Vec::new(),
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        &mut self.object
    }

    pub fn base_indentation_index(&self) -> usize {
        self.base_indentation_index
    }

    pub fn elements(&self) -> &[WeaveElement] {
        &self.elements
    }

    pub fn push(&mut self, mut element: WeaveElement) {
        let parent = self.object.reference();
        let child_ref = match &mut element {
            WeaveElement::Content(content) => {
                content.object_mut().set_parent_ref(parent);
                content.object().reference()
            }
            WeaveElement::Choice(choice) => {
                choice.object_mut().set_parent_ref(parent);
                choice.object().reference()
            }
            WeaveElement::Gather(gather) => {
                gather.object_mut().set_parent_ref(parent);
                gather.object().reference()
            }
            WeaveElement::NestedWeave(weave) => {
                weave.object_mut().set_parent_ref(parent);
                weave.object().reference()
            }
        };
        self.object.add_content_ref(child_ref);
        self.elements.push(element);
    }
}

#[cfg(test)]
mod tests {
    use super::{Choice, Gather, Weave, WeaveElement};
    use crate::parsed_hierarchy::ContentList;

    #[test]
    fn choice_sets_parent_on_content_lists() {
        let mut start = ContentList::new();
        start.push_text("start");
        let mut inner = ContentList::new();
        inner.push_text("inner");

        let choice = Choice::new(1, true, None, Some(start), None, inner);

        assert_eq!(
            Some(choice.object().id()),
            choice
                .start_content()
                .map(|c| c.object().parent_id())
                .flatten()
        );
        assert_eq!(
            Some(choice.object().id()),
            choice.inner_content().object().parent_id()
        );
        assert!(choice.once_only());
    }

    #[test]
    fn gather_sets_parent_on_optional_content() {
        let mut content = ContentList::new();
        content.push_text("join");
        let gather = Gather::new(1, None, Some(content));
        assert_eq!(
            Some(gather.object().id()),
            gather.content().map(|c| c.object().parent_id()).flatten()
        );
    }

    #[test]
    fn weave_sets_parent_on_inserted_elements() {
        let mut weave = Weave::new(0);
        weave.push(WeaveElement::Gather(Gather::new(1, None, None)));
        assert_eq!(1, weave.elements().len());
    }
}
