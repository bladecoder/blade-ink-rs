use std::{cell::{Cell, RefCell}, collections::HashMap, rc::Rc};

use bladeink::{
    ChoicePoint, CommandType, Container, ControlCommand, Divert, Glue, InkList, InkListItem,
    ListDefinition, NativeFunctionCall, NativeOp, Path, PushPopType, RTObject, Value, Void, path_of,
    VariableAssignment, VariableReference, story::Story as RuntimeStory,
};

use crate::{
    error::CompilerError,
    parsed_hierarchy::{
        ListDefinition as ParsedListDefinition, ParsedExpression, ParsedFlow, ParsedNode,
        ParsedNodeKind, Story,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PendingContainerId(usize);

#[derive(Clone)]
enum PathFixupSource {
    Divert(Rc<Divert>),
    ChoicePoint(Rc<ChoicePoint>),
}

#[derive(Clone, Copy)]
enum PathFixupTarget {
    PendingContainer(PendingContainerId),
    RuntimeObject(*const dyn RTObject),
}

#[derive(Clone)]
struct PathFixup {
    source: PathFixupSource,
    target: PathFixupTarget,
}

struct ExportState {
    next_pending_container_id: Cell<usize>,
    pending_containers: RefCell<HashMap<PendingContainerId, Rc<Container>>>,
    runtime_objects: RefCell<HashMap<*const dyn RTObject, Rc<dyn RTObject>>>,
    path_fixups: RefCell<Vec<PathFixup>>,
}

impl ExportState {
    fn new() -> Self {
        Self {
            next_pending_container_id: Cell::new(0),
            pending_containers: RefCell::new(HashMap::new()),
            runtime_objects: RefCell::new(HashMap::new()),
            path_fixups: RefCell::new(Vec::new()),
        }
    }

    fn next_pending_container_id(&self) -> PendingContainerId {
        let id = PendingContainerId(self.next_pending_container_id.get());
        self.next_pending_container_id.set(id.0 + 1);
        id
    }

    fn register_pending_container(&self, id: PendingContainerId, container: Rc<Container>) {
        self.pending_containers.borrow_mut().insert(id, container);
    }

    fn register_runtime_object(&self, object: Rc<dyn RTObject>) -> *const dyn RTObject {
        let key = Rc::as_ptr(&object);
        self.runtime_objects.borrow_mut().insert(key, object);
        key
    }

    fn add_pending_target_fixup(&self, source: PathFixupSource, target: PendingContainerId) {
        self.path_fixups.borrow_mut().push(PathFixup {
            source,
            target: PathFixupTarget::PendingContainer(target),
        });
    }

    fn add_runtime_target_fixup(&self, source: PathFixupSource, target: Rc<dyn RTObject>) {
        let key = self.register_runtime_object(target);
        self.path_fixups.borrow_mut().push(PathFixup {
            source,
            target: PathFixupTarget::RuntimeObject(key),
        });
    }

    fn apply_path_fixups(&self) {
        for fixup in self.path_fixups.borrow().iter() {
            let target: Rc<dyn RTObject> = match fixup.target {
                PathFixupTarget::PendingContainer(id) => self
                    .pending_containers
                    .borrow()
                    .get(&id)
                    .cloned()
                    .expect("registered pending container") as Rc<dyn RTObject>,
                PathFixupTarget::RuntimeObject(key) => self
                    .runtime_objects
                    .borrow()
                    .get(&key)
                    .cloned()
                    .expect("registered runtime object"),
            };
            let path = path_of(target.as_ref());
            match &fixup.source {
                PathFixupSource::Divert(divert) => divert.set_target_path(path),
                PathFixupSource::ChoicePoint(choice_point) => choice_point.set_path_on_choice(path),
            }
        }
    }
}

pub(crate) fn export_story(story: &Story) -> Result<RuntimeStory, CompilerError> {
    let state = ExportState::new();
    let list_defs = export_list_defs(story.list_definitions());
    let mut named_content = HashMap::new();

    for flow in story.parsed_flows() {
        let name = flow.flow().identifier().unwrap_or_default().to_owned();
        named_content.insert(name.clone(), export_flow(&state, flow, story, &name)?);
    }

    if let Some(global_decl) = export_global_decl(&state, story)? {
        named_content.insert("global decl".to_owned(), global_decl);
    }

    let inner_root = export_weave(
        &state,
        "0",
        story.root_nodes(),
        Scope::Root,
        story,
        true,
        &HashMap::new(),
    )?;
    let root = Container::new(
        None,
        flow_count_flags(story),
        vec![inner_root, command(CommandType::Done)],
        named_content,
    );

    state.apply_path_fixups();

    RuntimeStory::from_compiled(root, list_defs)
        .map_err(|error| CompilerError::invalid_source(error.to_string()))
}

fn export_list_defs(list_definitions: &[ParsedListDefinition]) -> Vec<ListDefinition> {
    list_definitions
        .iter()
        .filter_map(|definition| {
            let name = definition.identifier()?.to_owned();
            let items = definition
                .item_definitions()
                .iter()
                .map(|item| (item.name().to_owned(), item.series_value()))
                .collect();
            Some(ListDefinition::new(name, items))
        })
        .collect()
}

fn export_global_decl(_state: &ExportState, story: &Story) -> Result<Option<Rc<Container>>, CompilerError> {
    if story.global_initializers().is_empty() && story.list_definitions().is_empty() {
        return Ok(None);
    }

    let mut content = vec![command(CommandType::EvalStart)];

    for list in story.list_definitions() {
        content.push(list_value_from_definition(list));
        if let Some(name) = list.identifier() {
            content.push(variable_assignment(name, true, true));
        }
    }

    for (name, expression) in story.global_initializers() {
        export_expression(expression, story, &mut content)?;
        content.push(variable_assignment(name, true, true));
    }

    content.push(command(CommandType::EvalEnd));
    content.push(command(CommandType::End));
    Ok(Some(Container::new(
        Some("global decl".to_owned()),
        0,
        content,
        HashMap::new(),
    )))
}

fn list_value_from_definition(definition: &ParsedListDefinition) -> Rc<dyn RTObject> {
    let mut list = InkList::new();
    if let Some(name) = definition.identifier() {
        list.set_initial_origin_names(vec![name.to_owned()]);
        for item in definition.item_definitions() {
            if item.in_initial_list() {
                list.items.insert(
                    InkListItem::new(Some(name.to_owned()), item.name().to_owned()),
                    item.series_value(),
                );
            }
        }
    }
    rt_value(list)
}

fn export_flow(
    state: &ExportState,
    flow: &ParsedFlow,
    story: &Story,
    full_path: &str,
) -> Result<Rc<Container>, CompilerError> {
    let mut flow_named_paths = HashMap::new();
    if let Some(name) = flow.flow().identifier() {
        flow_named_paths.insert(name.to_owned(), full_path.to_owned());
    }
    for child in flow.children() {
        if let Some(child_name) = child.flow().identifier() {
            flow_named_paths.insert(child_name.to_owned(), format!("{full_path}.{child_name}"));
        }
    }

    let mut content = if flow.content().is_empty() && !flow.children().is_empty() {
        vec![divert_object(&format!(
            "{}.{}",
            full_path,
            flow.children()[0].flow().identifier().unwrap_or_default()
        ))]
    } else if has_weave_content(flow.content()) {
        let weave_root_index = flow.flow().arguments().len();
        vec![export_weave(
            state,
            &format!("{full_path}.{weave_root_index}"),
            flow.content(),
            Scope::Flow(flow),
            story,
            false,
            &flow_named_paths,
        )? as Rc<dyn RTObject>]
    } else {
        let flow_content_index_offset = flow.flow().arguments().len();
        export_nodes_with_paths(
            state,
            flow.content(),
            Scope::Flow(flow),
            story,
            Some(&flow_named_paths),
            Some(full_path),
            flow_content_index_offset,
        )?
    };

    if flow.flow().has_parameters() {
        let mut assignments = Vec::new();
        for argument in flow.flow().arguments().iter().rev() {
            assignments.push(variable_assignment(&argument.identifier, false, true));
        }
        assignments.extend(content);
        content = assignments;
    }

    if !flow.content().is_empty() && !has_terminal(flow.content()) {
        if !flow.flow().is_function() {
            content.push(command(CommandType::Done));
        }
    }

    let mut named = HashMap::new();
    for child in flow.children() {
        let child_name = child.flow().identifier().unwrap_or_default().to_owned();
        named.insert(
            child_name.clone(),
            export_flow(state, child, story, &format!("{full_path}.{child_name}"))?,
        );
    }

    Ok(Container::new(
        Some(flow.flow().identifier().unwrap_or_default().to_owned()),
        flow_count_flags(story),
        content,
        named,
    ))
}

#[derive(Clone, Copy)]
enum Scope<'a> {
    Root,
    Flow(&'a ParsedFlow),
}

fn export_nodes(
    state: &ExportState,
    nodes: &[ParsedNode],
    scope: Scope<'_>,
    story: &Story,
) -> Result<Vec<Rc<dyn RTObject>>, CompilerError> {
    export_nodes_with_paths(state, nodes, scope, story, None, None, 0)
}

fn export_nodes_with_paths(
    state: &ExportState,
    nodes: &[ParsedNode],
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    container_path: Option<&str>,
    content_index_offset: usize,
) -> Result<Vec<Rc<dyn RTObject>>, CompilerError> {
    let mut content = Vec::new();

    for node in nodes {
        let node_path = container_path.map(|path| format!("{path}.{}", content_index_offset + content.len()));
        match node.kind() {
            ParsedNodeKind::Text => {
                if let Some(text) = node.text()
                    && !text.is_empty()
                {
                    content.push(rt_value(text));
                }
            }
            ParsedNodeKind::Newline => content.push(rt_value("\n")),
            ParsedNodeKind::Glue => content.push(Rc::new(Glue::new())),
            ParsedNodeKind::Divert => {
                let target = node.target().ok_or_else(|| {
                    CompilerError::unsupported_feature("runtime export divert missing target")
                })?;
                if let Some(condition) = node.condition() {
                    content.push(command(CommandType::EvalStart));
                    export_expression(condition, story, &mut content)?;
                    content.push(command(CommandType::EvalEnd));
                    content.push(export_divert_conditional(target, scope, story, named_paths)?);
                } else {
                    export_divert_by_kind(
                        target,
                        node.arguments(),
                        scope,
                        story,
                        named_paths,
                        DivertKind::Normal,
                        &mut content,
                    )?;
                }
            }
            ParsedNodeKind::TunnelDivert => {
                let target = node.target().ok_or_else(|| {
                    CompilerError::unsupported_feature(
                        "runtime export tunnel divert missing target",
                    )
                })?;
                export_divert_by_kind(
                    target,
                    node.arguments(),
                    scope,
                    story,
                    named_paths,
                    DivertKind::Tunnel,
                    &mut content,
                )?;
            }
            ParsedNodeKind::TunnelReturn => {
                content.push(command(CommandType::EvalStart));
                content.push(Rc::new(Void::new()));
                content.push(command(CommandType::EvalEnd));
                content.push(command(CommandType::PopTunnel));
            }
            ParsedNodeKind::TunnelOnwardsWithTarget => {
                let target = node.target().ok_or_else(|| {
                    CompilerError::unsupported_feature(
                        "runtime export tunnel onwards missing target",
                    )
                })?;
                let variable_target = resolve_variable_divert_name(target, scope, story, named_paths);
                content.push(command(CommandType::EvalStart));
                export_divert_arguments(
                    node.arguments(),
                    target,
                    scope,
                    story,
                    named_paths,
                    &mut content,
                )?;
                if let Some(variable_target) = variable_target {
                    content.push(Rc::new(VariableReference::new(&variable_target)));
                } else {
                    let resolved = resolve_target(target, scope, story, named_paths);
                    content.push(rt_value(Path::new_with_components_string(Some(&resolved))));
                }
                content.push(command(CommandType::EvalEnd));
                content.push(command(CommandType::PopTunnel));
            }
            ParsedNodeKind::ReturnExpression => {
                let expression = node.expression().ok_or_else(|| {
                    CompilerError::unsupported_feature(
                        "runtime export return expression missing expression",
                    )
                })?;
                content.push(command(CommandType::EvalStart));
                export_expression(expression, story, &mut content)?;
                content.push(command(CommandType::EvalEnd));
                content.push(command(CommandType::PopFunction));
            }
            ParsedNodeKind::ReturnVoid => {
                content.push(command(CommandType::EvalStart));
                content.push(Rc::new(Void::new()));
                content.push(command(CommandType::EvalEnd));
                content.push(command(CommandType::PopFunction));
            }
            ParsedNodeKind::VoidCall => {
                let expression = node.expression().ok_or_else(|| {
                    CompilerError::unsupported_feature(
                        "runtime export void call missing expression",
                    )
                })?;
                content.push(command(CommandType::EvalStart));
                export_expression(expression, story, &mut content)?;
                content.push(command(CommandType::PopEvaluatedValue));
                content.push(command(CommandType::EvalEnd));
                content.push(rt_value("\n"));
            }
            ParsedNodeKind::Conditional | ParsedNodeKind::SwitchConditional => {
                if conditional_is_simple(node) {
                    append_simple_conditional(
                        state,
                        node,
                        scope,
                        story,
                        named_paths,
                        container_path,
                        content_index_offset,
                        &mut content,
                    )?;
                } else {
                    let conditional = export_conditional(
                        state,
                        node,
                        scope,
                        story,
                        named_paths,
                        node_path.as_deref(),
                    )?;
                    content.push(conditional);
                }
            }
            ParsedNodeKind::OutputExpression => {
                let expression = node.expression().ok_or_else(|| {
                    CompilerError::unsupported_feature(
                        "runtime export output expression missing expression",
                    )
                })?;
                content.push(command(CommandType::EvalStart));
                export_output_expression(expression, scope, story, named_paths, &mut content)?;
                content.push(command(CommandType::EvalOutput));
                content.push(command(CommandType::EvalEnd));
            }
            ParsedNodeKind::Assignment => export_assignment(node, scope, story, &mut content)?,
            ParsedNodeKind::Tag => export_tag_node(state, node, scope, story, named_paths, &mut content)?,
            ParsedNodeKind::AuthorWarning => {}
            ParsedNodeKind::Sequence => {
                content.push(export_sequence(
                    state,
                    node,
                    scope,
                    story,
                    named_paths,
                    node_path.as_deref(),
                )?);
            }
            ParsedNodeKind::Choice
            | ParsedNodeKind::GatherPoint
            | ParsedNodeKind::GatherLabel
            | ParsedNodeKind::ReturnBool => {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support {:?} yet",
                    node.kind()
                )));
            }
            ParsedNodeKind::ThreadDivert => {
                let target = node.target().ok_or_else(|| {
                    CompilerError::unsupported_feature(
                        "runtime export thread divert missing target",
                    )
                })?;
                export_divert_by_kind(
                    target,
                    node.arguments(),
                    scope,
                    story,
                    named_paths,
                    DivertKind::Thread,
                    &mut content,
                )?;
            }
        }
    }

    Ok(content)
}

fn export_tag_node(
    state: &ExportState,
    node: &ParsedNode,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    content.push(command(CommandType::BeginTag));
    let tag_content = export_nodes_with_paths(state, node.children(), scope, story, named_paths, None, 0)?;
    content.extend(tag_content);
    content.push(command(CommandType::EndTag));
    Ok(())
}

#[derive(Clone)]
enum PendingItem {
    Object(Rc<dyn RTObject>),
    Container(Rc<PendingContainer>),
}

struct PendingContainer {
    id: PendingContainerId,
    path: String,
    name: Option<String>,
    count_flags: i32,
    content: RefCell<Vec<PendingItem>>,
    named: RefCell<Vec<(String, Rc<PendingContainer>)>>,
}

impl PendingContainer {
    fn new(
        state: &ExportState,
        path: impl Into<String>,
        name: Option<String>,
        count_flags: i32,
    ) -> Rc<Self> {
        Rc::new(Self {
            id: state.next_pending_container_id(),
            path: path.into(),
            name,
            count_flags,
            content: RefCell::new(Vec::new()),
            named: RefCell::new(Vec::new()),
        })
    }

    fn push_object(&self, object: Rc<dyn RTObject>) {
        self.content.borrow_mut().push(PendingItem::Object(object));
    }

    fn push_container(&self, container: Rc<PendingContainer>) {
        self.content.borrow_mut().push(PendingItem::Container(container));
    }

    fn add_named(&self, key: impl Into<String>, container: Rc<PendingContainer>) {
        self.named.borrow_mut().push((key.into(), container));
    }

    fn next_content_index(&self) -> usize {
        self.content.borrow().len()
    }

    fn finalize(&self, state: &ExportState) -> Rc<Container> {
        let mut content = Vec::new();
        for item in self.content.borrow().iter() {
            match item {
                PendingItem::Object(object) => content.push(object.clone()),
                PendingItem::Container(container) => content.push(container.finalize(state)),
            }
        }

        let mut named = HashMap::new();
        for (key, container) in self.named.borrow().iter() {
            named.insert(key.clone(), container.finalize(state));
        }

        let finalized = Container::new(self.name.clone(), self.count_flags, content, named);
        state.register_pending_container(self.id, finalized.clone());
        finalized
    }
}

fn export_weave(
    state: &ExportState,
    path_prefix: &str,
    nodes: &[ParsedNode],
    scope: Scope<'_>,
    story: &Story,
    is_root: bool,
    inherited_named_paths: &HashMap<String, String>,
) -> Result<Rc<Container>, CompilerError> {
    Ok(build_weave_pending(
        state,
        path_prefix,
        nodes,
        scope,
        story,
        is_root,
        inherited_named_paths,
    )?
    .finalize(state))
}

fn build_weave_pending(
    state: &ExportState,
    path_prefix: &str,
    nodes: &[ParsedNode],
    scope: Scope<'_>,
    story: &Story,
    is_root: bool,
    inherited_named_paths: &HashMap<String, String>,
) -> Result<Rc<PendingContainer>, CompilerError> {
    let base_depth = nodes
        .iter()
        .filter_map(weave_depth)
        .min()
        .unwrap_or(1);

    let root = PendingContainer::new(state, path_prefix, None, 0);
    let mut current = root.clone();
    let mut loose_ends: Vec<Rc<PendingContainer>> = Vec::new();
    let mut previous_choice: Option<Rc<PendingContainer>> = None;
    let mut add_to_previous_choice = false;
    let mut has_seen_choice_in_section = false;
    let mut choice_count = 0usize;
    let mut gather_count = 0usize;
    let mut named_paths = inherited_named_paths.clone();
    named_paths.extend(collect_current_level_named_paths(path_prefix, nodes));

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

            let target = if add_to_previous_choice {
                previous_choice.clone().expect("previous choice container")
            } else {
                current.clone()
            };
            let nested_path = format!("{}.{}", target.path, target.next_content_index());
            let nested_pending = build_weave_pending(
                state,
                &nested_path,
                &nodes[nested_start..index],
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

            continue;
        }

        match node.kind() {
            ParsedNodeKind::Choice => {
                let choice_key = format!("c-{choice_count}");
                let choice_path = format!("{}.{}", current.path, choice_key);
                let choice_container = PendingContainer::new(
                    state,
                    &choice_path,
                    Some(choice_key.clone()),
                    weave_count_flags(story),
                );
                current.add_named(choice_key.clone(), choice_container.clone());

                export_choice(
                    state,
                    node,
                    choice_count,
                    &current.path,
                    current.clone(),
                    choice_container.clone(),
                    scope,
                    story,
                    &named_paths,
                )?;

                if let Some(name) = node.name() {
                    register_named_path(&mut named_paths, scope, name, &choice_path);
                }

                if !node.children().is_empty() {
                    if has_weave_content(node.children()) {
                        let weave = build_weave_pending(
                            state,
                            &format!("{}.{}", choice_container.path, choice_container.next_content_index()),
                            node.children(),
                            scope,
                            story,
                            false,
                            &named_paths,
                        )?;
                        for item in weave.content.borrow().iter().cloned() {
                            match item {
                                PendingItem::Object(object) => choice_container.push_object(object),
                                PendingItem::Container(container) => choice_container.push_container(container),
                            }
                        }
                        for (key, container) in weave.named.borrow().iter().cloned() {
                            choice_container.add_named(key, container);
                        }
                    } else {
                        let child_content = export_nodes_with_paths(
                            state,
                            node.children(),
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

                choice_container.push_object(rt_value("\n"));

                if node.is_invisible_default || !has_terminal(node.children()) {
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
            ParsedNodeKind::GatherPoint | ParsedNodeKind::GatherLabel => {
                let auto_enter = !has_seen_choice_in_section;
                let is_named_gather = node.name().is_some();
                let gather_name = node
                    .name()
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
                    weave_count_flags(story),
                );

                if auto_enter {
                    current.push_container(gather_container.clone());
                } else {
                    root.add_named(gather_name.clone(), gather_container.clone());
                }

                if !node.children().is_empty() {
                    if has_weave_content(node.children()) {
                        let weave = build_weave_pending(
                            state,
                            &format!("{}.{}", gather_container.path, gather_container.next_content_index()),
                            node.children(),
                            scope,
                            story,
                            false,
                            &named_paths,
                        )?;
                        for item in weave.content.borrow().iter().cloned() {
                            match item {
                                PendingItem::Object(object) => gather_container.push_object(object),
                                PendingItem::Container(container) => gather_container.push_container(container),
                            }
                        }
                        for (key, container) in weave.named.borrow().iter().cloned() {
                            gather_container.add_named(key, container);
                        }
                    } else {
                        let child_content = export_nodes_with_paths(
                            state,
                            node.children(),
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
                if let Some(name) = node.name() {
                    register_named_path(&mut named_paths, scope, name, &gather_path);
                }
                if !is_named_gather {
                    gather_count += 1;
                }
            }
            _ => {
                let target = if add_to_previous_choice {
                    previous_choice.clone().expect("previous choice container")
                } else {
                    current.clone()
                };
                let content = export_nodes_with_paths(
                    state,
                    std::slice::from_ref(node),
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
        }

        index += 1;
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
            weave_count_flags(story),
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

fn export_choice(
    state: &ExportState,
    node: &ParsedNode,
    choice_index: usize,
    path_prefix: &str,
    current: Rc<PendingContainer>,
    choice_container: Rc<PendingContainer>,
    scope: Scope<'_>,
    story: &Story,
    named_paths: &HashMap<String, String>,
) -> Result<(), CompilerError> {
    let choice_key = format!("c-{choice_index}");
    let relative_start_choice = !node.start_content.is_empty() && current.path.starts_with('.') ;
    let has_start = !node.start_content.is_empty();
    let has_choice_only = !node.choice_only_content.is_empty();
    let has_condition = node.condition().is_some();
    let flags = (has_condition as i32)
        + ((has_start as i32) * 2)
        + ((has_choice_only as i32) * 4)
        + ((node.is_invisible_default as i32) * 8)
        + ((node.once_only as i32) * 16);

    if has_start {
        let sub_index = current.next_content_index();
        let (outer_path, r1_path, r2_path) = if relative_start_choice {
            let outer = format!(".^.^.{sub_index}");
            (
                outer.clone(),
                format!("{}.{}.$r1", current.path, sub_index),
                format!("{}.$r2", choice_container.path),
            )
        } else {
            let outer = format!("{}.{}", current.path, sub_index);
            (
                outer.clone(),
                format!("{outer}.$r1"),
                format!("{}.{}.$r2", path_prefix, choice_key),
            )
        };
        let outer = PendingContainer::new(state, &outer_path, None, 0);
        outer.push_object(command(CommandType::EvalStart));
        outer.push_object(rt_value(Path::new_with_components_string(Some(&r1_path))));
        outer.push_object(variable_assignment("$r", false, true));
        outer.push_object(command(CommandType::BeginString));
        let divert_to_start_outer = Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            false,
            None,
            None,
        ));
        outer.push_object(divert_to_start_outer.clone());
        outer.push_container(PendingContainer::new(state, r1_path.clone(), Some("$r1".to_owned()), 0));
        outer.push_object(command(CommandType::EndString));

        if has_choice_only {
            outer.push_object(command(CommandType::BeginString));
            for item in export_nodes(state, &node.choice_only_content, scope, story)? {
                outer.push_object(item);
            }
            outer.push_object(command(CommandType::EndString));
        }

        if let Some(condition) = node.condition() {
            export_condition_expression(condition, story, named_paths, &mut outer.content.borrow_mut())?;
        }

        outer.push_object(command(CommandType::EvalEnd));
        let choice_point = Rc::new(ChoicePoint::new(flags, ""));
        state.add_pending_target_fixup(PathFixupSource::ChoicePoint(choice_point.clone()), choice_container.id);
        outer.push_object(choice_point);

        let start_container = PendingContainer::new(
            state,
            format!("{outer_path}.s"),
            Some("s".to_owned()),
            0,
        );
        state.add_pending_target_fixup(
            PathFixupSource::Divert(divert_to_start_outer),
            start_container.id,
        );
        for item in export_nodes(state, &node.start_content, scope, story)? {
            start_container.push_object(item);
        }
        start_container.push_object(variable_divert("$r"));
        outer.add_named("s", start_container.clone());

        current.push_container(outer);

        choice_container.push_object(command(CommandType::EvalStart));
        choice_container.push_object(rt_value(Path::new_with_components_string(Some(&r2_path))));
        choice_container.push_object(command(CommandType::EvalEnd));
        choice_container.push_object(variable_assignment("$r", false, true));
        let divert_to_start_inner = Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            false,
            None,
            None,
        ));
        state.add_pending_target_fixup(
            PathFixupSource::Divert(divert_to_start_inner.clone()),
            start_container.id,
        );
        choice_container.push_object(divert_to_start_inner);
        choice_container.push_container(PendingContainer::new(
            state,
            r2_path,
            Some("$r2".to_owned()),
            0,
        ));
    } else {
        current.push_object(command(CommandType::EvalStart));
        if has_choice_only {
            current.push_object(command(CommandType::BeginString));
            for item in export_nodes(state, &node.choice_only_content, scope, story)? {
                current.push_object(item);
            }
            current.push_object(command(CommandType::EndString));
        }
        if let Some(condition) = node.condition() {
            export_condition_expression(condition, story, named_paths, &mut current.content.borrow_mut())?;
        }
        current.push_object(command(CommandType::EvalEnd));
        let choice_point = Rc::new(ChoicePoint::new(flags, ""));
        state.add_pending_target_fixup(PathFixupSource::ChoicePoint(choice_point.clone()), choice_container.id);
        current.push_object(choice_point);
    }

    Ok(())
}

fn export_condition_expression(
    expression: &ParsedExpression,
    story: &Story,
    named_paths: &HashMap<String, String>,
    content: &mut Vec<PendingItem>,
) -> Result<(), CompilerError> {
    match expression {
        ParsedExpression::Variable(name) => {
            if let Some(path) = resolve_count_path(name, named_paths) {
                content.push(PendingItem::Object(Rc::new(VariableReference::from_path_for_count(
                    &path,
                ))));
            } else {
                let mut tmp = Vec::new();
                export_expression(expression, story, &mut tmp)?;
                content.extend(tmp.into_iter().map(PendingItem::Object));
            }
        }
        ParsedExpression::Unary { operator, expression } => {
            export_condition_expression(expression, story, named_paths, content)?;
            let op = match operator.as_str() {
                "!" => NativeOp::Not,
                "-" => NativeOp::Negate,
                other => {
                    return Err(CompilerError::unsupported_feature(format!(
                        "runtime export does not support unary operator '{other}'"
                    )))
                }
            };
            content.push(PendingItem::Object(native(op)));
        }
        ParsedExpression::Binary {
            left,
            operator,
            right,
        } => {
            export_condition_expression(left, story, named_paths, content)?;
            export_condition_expression(right, story, named_paths, content)?;
            content.push(PendingItem::Object(native(operator_token(operator)?)));
        }
        _ => {
            let mut tmp = Vec::new();
            export_expression(expression, story, &mut tmp)?;
            content.extend(tmp.into_iter().map(PendingItem::Object));
        }
    }

    Ok(())
}

fn export_output_expression(
    expression: &ParsedExpression,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    match expression {
        ParsedExpression::Variable(name) => {
            if let Some(path) = resolve_output_count_path(name, scope, named_paths) {
                content.push(Rc::new(VariableReference::from_path_for_count(&path)));
            } else {
                export_expression_scoped(expression, scope, story, named_paths, content)?;
            }
        }
        _ => export_expression_scoped(expression, scope, story, named_paths, content)?,
    }

    Ok(())
}

fn export_expression_scoped(
    expression: &ParsedExpression,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    match expression {
        ParsedExpression::Bool(value) => content.push(rt_value(*value)),
        ParsedExpression::Int(value) => content.push(rt_value(*value)),
        ParsedExpression::Float(value) => content.push(rt_value(*value)),
        ParsedExpression::String(value) => {
            content.push(command(CommandType::BeginString));
            content.push(rt_value(value.as_str()));
            content.push(command(CommandType::EndString));
        }
        ParsedExpression::StringExpression(nodes) => {
            content.push(command(CommandType::BeginString));
            export_string_expression(nodes, story, content)?;
            content.push(command(CommandType::EndString));
        }
        ParsedExpression::Variable(name) => {
            if let Some(constant) = story.const_declaration(name) {
                export_expression_node(constant.expression(), story, content)?;
            } else {
                content.push(Rc::new(VariableReference::new(name)));
            }
        }
        ParsedExpression::DivertTarget(target) => {
            let resolved = resolve_target(target, scope, story, named_paths);
            content.push(rt_value(Path::new_with_components_string(Some(&resolved))));
        }
        ParsedExpression::ListItems(items) => {
            let mut list = InkList::new();
            for item in items {
                insert_resolved_list_item(story, item, &mut list)?;
            }
            content.push(rt_value(list));
        }
        ParsedExpression::EmptyList => content.push(rt_value(InkList::new())),
        ParsedExpression::Unary { operator, expression } => match operator.as_str() {
            "-" => match expression.as_ref() {
                ParsedExpression::Int(value) => content.push(rt_value(-value)),
                ParsedExpression::Float(value) => content.push(rt_value(-value)),
                other => {
                    export_expression_scoped(other, scope, story, named_paths, content)?;
                    content.push(native(NativeOp::Negate));
                }
            },
            "!" => {
                export_expression_scoped(expression, scope, story, named_paths, content)?;
                content.push(native(NativeOp::Not));
            }
            _ => {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support unary operator '{operator}'"
                )));
            }
        },
        ParsedExpression::Binary {
            left,
            operator,
            right,
        } => {
            export_expression_scoped(left, scope, story, named_paths, content)?;
            export_expression_scoped(right, scope, story, named_paths, content)?;
            content.push(native(operator_token(operator)?));
        }
        ParsedExpression::FunctionCall { name, arguments } => {
            if story
                .list_definitions()
                .iter()
                .any(|list| list.identifier() == Some(name.as_str()))
            {
                match arguments.as_slice() {
                    [] => {
                        let list = InkList::new();
                        list.set_initial_origin_names(vec![name.to_owned()]);
                        content.push(rt_value(list));
                    }
                    [argument] => {
                        content.push(rt_value(name.as_str()));
                        export_expression_scoped(argument, scope, story, named_paths, content)?;
                        content.push(command(CommandType::ListFromInt));
                    }
                    _ => {
                        return Err(CompilerError::unsupported_feature(format!(
                            "runtime export does not support list call '{name}' with {} arguments",
                            arguments.len()
                        )));
                    }
                }
            } else if let Some(command_type) = builtin_command(name) {
                for argument in arguments {
                    export_expression_scoped(argument, scope, story, named_paths, content)?;
                }
                content.push(command(command_type));
            } else if let Some(native_op) = native_function(name) {
                for argument in arguments {
                    export_expression_scoped(argument, scope, story, named_paths, content)?;
                }
                content.push(native(native_op));
            } else {
                export_expression(expression, story, content)?;
            }
        }
    }

    Ok(())
}

fn register_named_path(
    named_paths: &mut HashMap<String, String>,
    scope: Scope<'_>,
    name: &str,
    runtime_path: &str,
) {
    named_paths.insert(name.to_owned(), runtime_path.to_owned());

    if let Some(alias) = weave_label_alias(runtime_path, name) {
        named_paths.insert(alias, runtime_path.to_owned());
    }

    if let Scope::Flow(flow) = scope
        && let Some(flow_name) = flow.flow().identifier()
    {
        named_paths.insert(format!("{flow_name}.{name}"), runtime_path.to_owned());
    }
}

fn flow_runtime_path(story: &Story, target: &ParsedFlow) -> Option<String> {
    flow_runtime_path_in(story.parsed_flows(), target.object().id(), None)
}

fn flow_runtime_path_in(
    flows: &[ParsedFlow],
    target_id: usize,
    prefix: Option<&str>,
) -> Option<String> {
    for flow in flows {
        let flow_name = flow.flow().identifier()?;
        let path = if let Some(prefix) = prefix {
            format!("{prefix}.{flow_name}")
        } else {
            flow_name.to_owned()
        };

        if flow.object().id() == target_id {
            return Some(path);
        }

        if let Some(found) = flow_runtime_path_in(flow.children(), target_id, Some(&path)) {
            return Some(found);
        }
    }

    None
}

fn weave_label_alias(runtime_path: &str, name: &str) -> Option<String> {
    runtime_path
        .strip_suffix(&format!(".0.{name}"))
        .map(|prefix| format!("{prefix}.{name}"))
}

fn resolve_count_path(name: &str, named_paths: &HashMap<String, String>) -> Option<String> {
    named_paths.get(name).cloned().or_else(|| {
        let mut parts: Vec<&str> = name.split('.').collect();
        if parts.len() < 2 {
            return None;
        }
        let last = parts.pop()?;
        Some(format!("{}.0.{last}", parts.join(".")))
    })
}

fn resolve_output_count_path(
    name: &str,
    scope: Scope<'_>,
    named_paths: Option<&HashMap<String, String>>,
) -> Option<String> {
    if let Some(named_paths) = named_paths
        && let Some(path) = resolve_count_path(name, named_paths)
    {
        return Some(path);
    }

    let Scope::Flow(flow) = scope else {
        return None;
    };

    if flow.flow().identifier() == Some(name) {
        return Some(".^".to_owned());
    }

    None
}

fn collect_current_level_named_paths(
    path_prefix: &str,
    nodes: &[ParsedNode],
) -> HashMap<String, String> {
    let base_depth = nodes
        .iter()
        .filter_map(weave_depth)
        .min()
        .unwrap_or(1);
    let mut current_path = path_prefix.to_owned();
    let mut has_seen_choice_in_section = false;
    let mut choice_count = 0usize;
    let mut gather_count = 0usize;
    let mut result = HashMap::new();

    for node in nodes {
        let Some(depth) = weave_depth(node) else {
            continue;
        };
        if depth > base_depth {
            continue;
        }

        match node.kind() {
            ParsedNodeKind::Choice => {
                if let Some(name) = node.name() {
                    let choice_path = format!("{}.c-{choice_count}", current_path);
                    result.insert(name.to_owned(), choice_path.clone());
                    if let Some(alias) = weave_label_alias(&choice_path, name) {
                        result.insert(alias, choice_path);
                    }
                }
                has_seen_choice_in_section = true;
                choice_count += 1;
            }
            ParsedNodeKind::GatherPoint | ParsedNodeKind::GatherLabel => {
                let is_named_gather = node.name().is_some();
                let gather_name = node
                    .name()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| format!("g-{gather_count}"));
                let gather_path = if !has_seen_choice_in_section {
                    format!("{}.{}", current_path, gather_name)
                } else {
                    format!("{path_prefix}.{}", gather_name)
                };
                if node.name().is_some() {
                    result.insert(gather_name.clone(), gather_path.clone());
                    if let Some(alias) = weave_label_alias(&gather_path, &gather_name) {
                        result.insert(alias, gather_path.clone());
                    }
                }
                current_path = gather_path;
                has_seen_choice_in_section = false;
                if !is_named_gather {
                    gather_count += 1;
                }
            }
            _ => {}
        }
    }

    result
}

fn collect_loose_choice_ends(container: &Rc<PendingContainer>) -> Vec<Rc<PendingContainer>> {
    let mut result = Vec::new();

    if container.count_flags == 5 && !pending_container_has_terminal(container) {
        result.push(container.clone());
    }

    for (_, named) in container.named.borrow().iter() {
        result.extend(collect_loose_choice_ends(named));
    }

    for item in container.content.borrow().iter() {
        if let PendingItem::Container(child) = item {
            result.extend(collect_loose_choice_ends(child));
        }
    }

    result
}

fn pending_container_has_terminal(container: &Rc<PendingContainer>) -> bool {
    let content = container.content.borrow();
    let Some(last) = content.last() else {
        return false;
    };

    match last {
        PendingItem::Object(object) => {
            if object.as_any().is::<Divert>() {
                return true;
            }
            if let Some(command) = object.as_any().downcast_ref::<ControlCommand>() {
                return matches!(command.command_type, CommandType::End | CommandType::Done);
            }
            false
        }
        PendingItem::Container(_) => false,
    }
}

fn variable_divert(name: &str) -> Rc<dyn RTObject> {
    Rc::new(Divert::new(
        false,
        PushPopType::Tunnel,
        false,
        0,
        false,
        Some(name.to_owned()),
        None,
    ))
}

fn has_weave_content(nodes: &[ParsedNode]) -> bool {
    nodes.iter().any(|node| weave_depth(node).is_some())
}

fn weave_depth(node: &ParsedNode) -> Option<usize> {
    match node.kind() {
        ParsedNodeKind::Choice | ParsedNodeKind::GatherPoint | ParsedNodeKind::GatherLabel => {
            Some(node.indentation_depth)
        }
        _ => None,
    }
}

fn export_divert(
    target: &str,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
) -> Result<Rc<dyn RTObject>, CompilerError> {
    match target {
        "END" => Ok(command(CommandType::End)),
        "DONE" => Ok(command(CommandType::Done)),
        _ => {
            let resolved = resolve_target(target, scope, story, named_paths);
            if story
                .parsed_flows()
                .iter()
                .any(|flow| flow.flow().identifier() == Some(resolved.as_str()) && flow.flow().is_function())
            {
                return Err(CompilerError::unsupported_feature(format!(
                    "cannot divert to function '{resolved}'"
                )));
            }
            if is_global_variable_divert(&resolved, story) {
                Ok(Rc::new(Divert::new(
                    false,
                    PushPopType::Tunnel,
                    false,
                    0,
                    false,
                    Some(resolved),
                    None,
                )))
            } else {
                Ok(divert_object(&resolved))
            }
        }
    }
}

#[derive(Clone, Copy)]
enum DivertKind {
    Normal,
    Tunnel,
    Thread,
}

fn export_divert_by_kind(
    target: &str,
    arguments: &[ParsedExpression],
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    kind: DivertKind,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    if matches!(kind, DivertKind::Normal) {
        match target {
            "END" => {
                content.push(command(CommandType::End));
                return Ok(());
            }
            "DONE" => {
                content.push(command(CommandType::Done));
                return Ok(());
            }
            _ => {}
        }
    }

    let variable_target = resolve_variable_divert_name(target, scope, story, named_paths);
    let resolved = variable_target
        .clone()
        .unwrap_or_else(|| resolve_target(target, scope, story, named_paths));

    if variable_target.is_none()
        && story
            .parsed_flows()
            .iter()
            .any(|flow| flow.flow().identifier() == Some(resolved.as_str()) && flow.flow().is_function())
    {
        return Err(CompilerError::unsupported_feature(format!(
            "cannot divert to function '{resolved}'"
        )));
    }

    if variable_target.is_some() && !arguments.is_empty() {
        return Err(CompilerError::unsupported_feature(format!(
            "can't store arguments in a divert target variable '{resolved}'"
        )));
    }

    if !arguments.is_empty() {
        content.push(command(CommandType::EvalStart));
        export_divert_arguments(arguments, &resolved, scope, story, named_paths, content)?;
        content.push(command(CommandType::EvalEnd));
    }

    if matches!(kind, DivertKind::Thread) {
        content.push(command(CommandType::StartThread));
    }

    let divert = if let Some(variable_name) = variable_target {
        Rc::new(Divert::new(
            matches!(kind, DivertKind::Tunnel),
            PushPopType::Tunnel,
            false,
            0,
            false,
            Some(variable_name),
            None,
        )) as Rc<dyn RTObject>
    } else {
        match kind {
            DivertKind::Normal => divert_object(&resolved),
            DivertKind::Tunnel => export_tunnel_divert(&resolved),
            DivertKind::Thread => divert_object(&resolved),
        }
    };

    content.push(divert);
    Ok(())
}

fn export_divert_conditional(
    target: &str,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
) -> Result<Rc<dyn RTObject>, CompilerError> {
    match target {
        "END" => Ok(Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            true,
            None,
            Some("END"),
        ))),
        "DONE" => Ok(Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            true,
            None,
            Some("DONE"),
        ))),
        _ => Ok(Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            true,
            None,
            Some(&resolve_target(target, scope, story, named_paths)),
        ))),
    }
}

fn export_tunnel_divert(target: &str) -> Rc<dyn RTObject> {
    Rc::new(Divert::new(
        true,
        PushPopType::Tunnel,
        false,
        0,
        false,
        None,
        Some(target),
    ))
}

fn export_divert_arguments(
    arguments: &[ParsedExpression],
    target: &str,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    let expected_arguments = find_flow_by_path(story.parsed_flows(), target).map(|flow| flow.flow().arguments());

    for (index, argument) in arguments.iter().enumerate() {
        let expected_argument = expected_arguments.and_then(|args| args.get(index));
        if expected_argument.is_some_and(|arg| arg.is_by_reference) {
            let ParsedExpression::Variable(var_name) = argument else {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export divert to '{target}' requires variable arguments for by-reference parameters"
                )));
            };
            content.push(Rc::new(Value::new_variable_pointer(var_name, -1)));
        } else {
            export_divert_argument_expression(argument, scope, story, named_paths, content)?;
        }
    }

    Ok(())
}

fn export_divert_argument_expression(
    argument: &ParsedExpression,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    match argument {
        ParsedExpression::DivertTarget(target) => {
            let resolved = resolve_target(target, scope, story, named_paths);
            content.push(rt_value(Path::new_with_components_string(Some(&resolved))));
            Ok(())
        }
        _ => export_expression(argument, story, content),
    }
}

fn find_flow_by_path<'a>(flows: &'a [ParsedFlow], path: &str) -> Option<&'a ParsedFlow> {
    let mut parts = path.split('.');
    let first = parts.next()?;
    let mut current = flows.iter().find(|flow| flow.flow().identifier() == Some(first))?;

    for part in parts {
        current = current
            .children()
            .iter()
            .find(|flow| flow.flow().identifier() == Some(part))?;
    }

    Some(current)
}

fn is_global_variable_divert(target: &str, story: &Story) -> bool {
    story
        .global_initializers()
        .iter()
        .any(|(name, _)| name == target)
        && !story
            .parsed_flows()
            .iter()
            .any(|flow| flow.flow().identifier() == Some(target))
}

fn resolve_variable_divert_name(
    target: &str,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
) -> Option<String> {
    if target.contains('.')
        || named_paths.is_some_and(|paths| paths.contains_key(target))
    {
        return None;
    }

    if is_global_variable_divert(target, story) {
        return Some(target.to_owned());
    }

    let Scope::Flow(flow) = scope else {
        return None;
    };

    flow.flow()
        .arguments()
        .iter()
        .any(|arg| arg.identifier == target && arg.is_divert_target)
        .then_some(target.to_owned())
}

fn resolve_target(
    target: &str,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
) -> String {
    if let Some(named_paths) = named_paths
        && let Some(path) = named_paths.get(target)
    {
        return path.clone();
    }

    if target.contains('.') {
        if let Some(path) = resolve_explicit_weave_target(target, story) {
            return path;
        }
        if let Some(expanded) = expand_weave_point_path(target, story, named_paths) {
            return expanded;
        }
        return target.to_owned();
    }

    let Scope::Flow(flow) = scope else {
        if let Some(resolved) = resolve_unique_nested_flow_target(target, story) {
            return resolved;
        }
        return target.to_owned();
    };

    if flow
        .children()
        .iter()
        .any(|child| child.flow().identifier() == Some(target))
    {
        return format!(
            "{}.{target}",
            flow.flow().identifier().unwrap_or_default()
        );
    }

    if let Some(resolved) = resolve_sibling_or_ancestor_flow_target(target, flow, story) {
        return resolved;
    }

    if story
        .parsed_flows()
        .iter()
        .any(|candidate| candidate.flow().identifier() == Some(target))
    {
        return target.to_owned();
    }

    target.to_owned()
}

fn resolve_unique_nested_flow_target(target: &str, story: &Story) -> Option<String> {
    let mut matches = Vec::new();
    collect_nested_flow_target_paths(story.parsed_flows(), target, None, &mut matches);
    (matches.len() == 1).then(|| matches.pop().unwrap())
}

fn collect_nested_flow_target_paths(
    flows: &[ParsedFlow],
    target: &str,
    prefix: Option<&str>,
    matches: &mut Vec<String>,
) {
    for flow in flows {
        let Some(flow_name) = flow.flow().identifier() else {
            continue;
        };
        let path = prefix
            .map(|prefix| format!("{prefix}.{flow_name}"))
            .unwrap_or_else(|| flow_name.to_owned());

        if flow
            .children()
            .iter()
            .any(|child| child.flow().identifier() == Some(target))
        {
            matches.push(format!("{path}.{target}"));
        }

        collect_nested_flow_target_paths(flow.children(), target, Some(&path), matches);
    }
}

fn resolve_sibling_or_ancestor_flow_target(
    target: &str,
    current_flow: &ParsedFlow,
    story: &Story,
) -> Option<String> {
    resolve_sibling_or_ancestor_flow_target_in(
        story.parsed_flows(),
        current_flow.object().id(),
        target,
        None,
    )
}

fn resolve_sibling_or_ancestor_flow_target_in(
    flows: &[ParsedFlow],
    current_flow_id: usize,
    target: &str,
    prefix: Option<&str>,
) -> Option<String> {
    for flow in flows {
        let flow_name = flow.flow().identifier()?;
        let path = prefix
            .map(|prefix| format!("{prefix}.{flow_name}"))
            .unwrap_or_else(|| flow_name.to_owned());

        if flow.children().iter().any(|child| child.object().id() == current_flow_id)
            && flow
                .children()
                .iter()
                .any(|child| child.flow().identifier() == Some(target))
        {
            return Some(format!("{path}.{target}"));
        }

        if let Some(found) = resolve_sibling_or_ancestor_flow_target_in(
            flow.children(),
            current_flow_id,
            target,
            Some(&path),
        ) {
            return Some(found);
        }
    }

    None
}

fn resolve_explicit_weave_target(target: &str, story: &Story) -> Option<String> {
    let mut parts: Vec<&str> = target.split('.').collect();
    if parts.len() < 2 {
        return None;
    }

    let weave_name = parts.pop()?;
    let mut flows = story.parsed_flows();
    let mut prefix = String::new();
    let mut current_flow: Option<&ParsedFlow> = None;

    for flow_name in parts {
        let flow = flows.iter().find(|flow| flow.flow().identifier() == Some(flow_name))?;
        if !prefix.is_empty() {
            prefix.push('.');
        }
        prefix.push_str(flow_name);
        current_flow = Some(flow);
        flows = flow.children();
    }

    let flow = current_flow?;
    let path_prefix = format!("{prefix}.0");
    let named_paths = collect_current_level_named_paths(&path_prefix, flow.content());
    named_paths.get(weave_name).cloned()
}

fn expand_weave_point_path(
    target: &str,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
) -> Option<String> {
    if let Some(named_paths) = named_paths
        && let Some(path) = named_paths.get(target)
    {
        return Some(path.clone());
    }

    let mut parts: Vec<&str> = target.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let last = parts.pop()?;
    let candidate = format!("{}.0.{last}", parts.join("."));
    if has_named_content_path(story.parsed_flows(), &candidate) {
        Some(candidate)
    } else {
        None
    }
}

fn has_named_content_path(flows: &[ParsedFlow], path: &str) -> bool {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return false;
    }

    for flow in flows {
        if flow.flow().identifier() == Some(parts[0]) {
            return has_named_content_path_in_flow(flow, &parts[1..]);
        }
    }

    false
}

fn has_named_content_path_in_flow(flow: &ParsedFlow, parts: &[&str]) -> bool {
    if parts.is_empty() {
        return true;
    }

    if parts[0] == "0" {
        return has_named_content_path_in_nodes(flow.content(), &parts[1..]);
    }

    for child in flow.children() {
        if child.flow().identifier() == Some(parts[0]) {
            return has_named_content_path_in_flow(child, &parts[1..]);
        }
    }

    false
}

fn has_named_content_path_in_nodes(nodes: &[ParsedNode], parts: &[&str]) -> bool {
    if parts.is_empty() {
        return true;
    }

    let target = parts[0];
    for node in nodes {
        if matches!(node.kind(), ParsedNodeKind::Choice | ParsedNodeKind::GatherLabel)
            && node.name() == Some(target)
        {
            return true;
        }
        if node.kind() == ParsedNodeKind::GatherPoint && node.name().is_none() {
            continue;
        }
    }

    false
}

fn export_sequence(
    state: &ExportState,
    node: &ParsedNode,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    container_path: Option<&str>,
) -> Result<Rc<dyn RTObject>, CompilerError> {
    use crate::parsed_hierarchy::SequenceType;

    let seq_type = node.sequence_type;
    let once     = (seq_type & SequenceType::Once    as u8) != 0;
    let cycle    = (seq_type & SequenceType::Cycle   as u8) != 0;
    let stopping = (seq_type & SequenceType::Stopping as u8) != 0;
    let shuffle  = (seq_type & SequenceType::Shuffle  as u8) != 0;
    // Default: if no flag set, treat as stopping
    let stopping = stopping || (!once && !cycle && !shuffle);

    let elements = node.children(); // each child is a wrapper with children = element nodes
    let num_elements = elements.len();
    let empty_named_paths = HashMap::new();
    let named_paths = named_paths.unwrap_or(&empty_named_paths);

    // Number of branches (once gets an extra empty branch)
    let seq_branch_count = if once { num_elements + 1 } else { num_elements };

    // Build seq_items (the list of items before the nop)
    let mut seq_items: Vec<Rc<dyn RTObject>> = Vec::new();

    // Eval block: compute chosen index
    seq_items.push(command(CommandType::EvalStart));
    seq_items.push(command(CommandType::VisitIndex));

    if stopping || once {
        seq_items.push(rt_int(seq_branch_count as i32 - 1));
        seq_items.push(Rc::new(NativeFunctionCall::new(NativeOp::Min)));
    } else if cycle {
        seq_items.push(rt_int(num_elements as i32));
        seq_items.push(Rc::new(NativeFunctionCall::new(NativeOp::Mod)));
    }

    let mut post_shuffle_no_op: Option<Rc<dyn RTObject>> = None;

    if shuffle {
        if once || stopping {
            let last_idx = if stopping {
                num_elements as i32 - 1
            } else {
                num_elements as i32
            };
            let skip_shuffle_divert = Rc::new(Divert::new(
                false,
                PushPopType::Tunnel,
                false,
                0,
                true,
                None,
                None,
            ));
            let post_shuffle_nop = command(CommandType::NoOp);
            state.add_runtime_target_fixup(
                PathFixupSource::Divert(skip_shuffle_divert.clone()),
                post_shuffle_nop.clone(),
            );
            seq_items.push(command(CommandType::Duplicate));
            seq_items.push(rt_int(last_idx));
            seq_items.push(native(NativeOp::Equal));
            seq_items.push(skip_shuffle_divert);
            post_shuffle_no_op = Some(post_shuffle_nop);
        }

        let element_count_to_shuffle = if stopping {
            num_elements.saturating_sub(1)
        } else {
            num_elements
        };
        seq_items.push(rt_int(element_count_to_shuffle as i32));
        seq_items.push(command(CommandType::SequenceShuffleIndex));
        if let Some(post_shuffle_nop) = post_shuffle_no_op.clone() {
            seq_items.push(post_shuffle_nop);
        }
    }

    seq_items.push(command(CommandType::EvalEnd));

    let mut branch_diverts = Vec::new();

    for el_index in 0..seq_branch_count {
        seq_items.push(command(CommandType::EvalStart));
        seq_items.push(command(CommandType::Duplicate));
        seq_items.push(rt_int(el_index as i32));
        seq_items.push(Rc::new(NativeFunctionCall::new(NativeOp::Equal)));
        seq_items.push(command(CommandType::EvalEnd));

        let branch_divert = Rc::new(Divert::new(
            false,
            PushPopType::Function,
            false,
            0,
            true,
            None,
            None,
        ));
        branch_diverts.push(branch_divert.clone());
        seq_items.push(branch_divert);
    }

    let post_sequence_nop = command(CommandType::NoOp);
    seq_items.push(post_sequence_nop.clone());

    let mut named_branches: HashMap<String, Rc<Container>> = HashMap::new();
    for el_index in 0..seq_branch_count {
        let (branch_content, branch_named, branch_flags): (Vec<Rc<dyn RTObject>>, HashMap<String, Rc<Container>>, i32) = if el_index < num_elements {
            let element_nodes = elements[el_index].children();
            if has_weave_content(element_nodes) {
                let branch_path_prefix = container_path
                    .map(|path| format!("{path}.s{el_index}"))
                    .unwrap_or_else(|| ".^".to_owned());
                let weave = unwrap_weave_root_container(&export_weave(
                    state,
                    &branch_path_prefix,
                    element_nodes,
                    scope,
                    story,
                    false,
                    named_paths,
                )?);
                (weave.content.clone(), weave.named_content.clone(), weave.get_count_flags())
            } else {
                (
                    export_nodes_with_paths(
                        state,
                        element_nodes,
                        scope,
                        story,
                        Some(named_paths),
                        container_path.map(|path| format!("{path}.s{el_index}")).as_deref(),
                        0,
                    )?,
                    HashMap::new(),
                    0,
                )
            }
        } else {
            (Vec::new(), HashMap::new(), 0)
        };

        let back_divert = Rc::new(Divert::new(
            false, PushPopType::Function, false, 0, false, None, None,
        ));
        state.add_runtime_target_fixup(
            PathFixupSource::Divert(back_divert.clone()),
            post_sequence_nop.clone(),
        );

        let mut branch_items: Vec<Rc<dyn RTObject>> = Vec::new();
        branch_items.push(command(CommandType::PopEvaluatedValue));
        branch_items.extend(branch_content);
        branch_items.push(back_divert);

        let branch_name = format!("s{}", el_index);
        let branch_container = Container::new(Some(branch_name.clone()), branch_flags, branch_items, branch_named);
        state.add_runtime_target_fixup(
            PathFixupSource::Divert(branch_diverts[el_index].clone()),
            branch_container.clone(),
        );
        named_branches.insert(branch_name, branch_container);
    }

    Ok(Container::new(
        None,
        5, // visits_should_be_counted | counting_at_start_only
        seq_items,
        named_branches,
    ))
}

fn unwrap_weave_root_container(container: &Rc<Container>) -> Rc<Container> {
    if !container.has_valid_name()
        && container.named_content.is_empty()
        && container.content.len() == 1
        && let Ok(child) = container.content[0].clone().into_any().downcast::<Container>()
        && !child.has_valid_name()
    {
        return child;
    }

    container.clone()
}

fn conditional_is_simple(node: &ParsedNode) -> bool {
    node.children().iter().all(|branch| {
        branch.children().iter().all(|child| {
            !matches!(child.kind(), ParsedNodeKind::Conditional | ParsedNodeKind::SwitchConditional)
        })
    })
}

fn append_simple_conditional(
    state: &ExportState,
    node: &ParsedNode,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    parent_container_path: Option<&str>,
    content_index_offset: usize,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    let empty_named_paths = HashMap::new();
    let named_paths = named_paths.unwrap_or(&empty_named_paths);
    let is_switch = node.kind() == ParsedNodeKind::SwitchConditional;

    if let Some(initial) = node.condition() {
        content.push(command(CommandType::EvalStart));
        export_condition_expression_runtime(initial, scope, story, named_paths, content)?;
        content.push(command(CommandType::EvalEnd));
    }

    let needs_final_pop = is_switch
        && node.children().first().is_some_and(|branch| branch.condition().is_some())
        && !node.children().last().is_some_and(|branch| branch.is_else);
    let rejoin_nop = command(CommandType::NoOp);

    for branch in node.children() {
        let duplicates_stack_value = branch.matching_equality && !branch.is_else;
        let mut branch_control: Vec<Rc<dyn RTObject>> = Vec::new();
        let mut branch_nodes = branch.children().to_vec();
        if !branch.is_inline && has_weave_content(branch.children()) {
            branch_nodes.insert(0, ParsedNode::new(ParsedNodeKind::Newline));
        }
        if duplicates_stack_value {
            branch_control.push(command(CommandType::Duplicate));
        }
        if !branch.is_true_branch && !branch.is_else {
            if branch.condition().is_some() {
                branch_control.push(command(CommandType::EvalStart));
            }
            if let Some(condition) = branch.condition() {
                export_condition_expression_runtime(condition, scope, story, named_paths, &mut branch_control)?;
            }
            if branch.matching_equality {
                branch_control.push(native(NativeOp::Equal));
            }
            if branch.condition().is_some() {
                branch_control.push(command(CommandType::EvalEnd));
            }
        }
        let branch_divert = Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            !branch.is_else,
            None,
            None,
        ));
        branch_control.push(branch_divert.clone());

        let (mut branch_content, mut branch_named) = if has_weave_content(branch.children()) {
            let branch_container_index = content_index_offset + content.len();
            let branch_path = parent_container_path
                .map(|path| format!("{path}.{branch_container_index}.b"))
                .unwrap_or_else(|| "b".to_owned());
            let weave = unwrap_weave_root_container(&export_weave(
                state,
                &branch_path,
                &branch_nodes,
                scope,
                story,
                false,
                named_paths,
            )?);
            (weave.content.clone(), weave.named_content.clone())
        } else {
            (
                export_nodes_with_paths(
                    state,
                    branch.children(),
                    scope,
                    story,
                    Some(named_paths),
                    None,
                    0,
                )?,
                HashMap::new(),
            )
        };
        if !branch.is_inline && !has_weave_content(branch.children()) {
            branch_content.insert(0, rt_value("\n"));
        }
        if duplicates_stack_value || (branch.is_else && branch.matching_equality) {
            branch_content.insert(0, command(CommandType::PopEvaluatedValue));
        }
        let return_divert = Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            false,
            None,
            None,
        ));
        state.add_runtime_target_fixup(
            PathFixupSource::Divert(return_divert.clone()),
            rejoin_nop.clone(),
        );
        branch_content.push(return_divert);

        let branch_b = Container::new(Some("b".to_owned()), 0, branch_content, branch_named);
        state.add_runtime_target_fixup(
            PathFixupSource::Divert(branch_divert),
            branch_b.clone(),
        );
        let mut branch_named = HashMap::new();
        branch_named.insert("b".to_owned(), branch_b);
        content.push(Container::new(None, 0, branch_control, branch_named) as Rc<dyn RTObject>);
    }

    if needs_final_pop {
        content.push(command(CommandType::PopEvaluatedValue));
    }
    content.push(rejoin_nop);
    Ok(())
}

fn export_conditional(
    state: &ExportState,
    node: &ParsedNode,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
    container_path: Option<&str>,
) -> Result<Rc<Container>, CompilerError> {
    let mut content: Vec<Rc<dyn RTObject>> = Vec::new();
    let is_switch = node.kind() == ParsedNodeKind::SwitchConditional;
    let empty_named_paths = HashMap::new();
    let named_paths = named_paths.unwrap_or(&empty_named_paths);

    if let Some(initial) = node.condition() {
        content.push(command(CommandType::EvalStart));
        export_condition_expression_runtime(initial, scope, story, named_paths, &mut content)?;
        content.push(command(CommandType::EvalEnd));
    }

    let branch_container_base_index = content.len();

    let mut branch_specs: Vec<(bool, bool, Vec<Rc<dyn RTObject>>, HashMap<String, Rc<Container>>, i32)> = Vec::new();

    for (idx, branch) in node.children().iter().enumerate() {
        let duplicates_stack_value = branch.matching_equality && !branch.is_else;
        let mut branch_nodes = branch.children().to_vec();
        if !branch.is_inline {
            branch_nodes.insert(0, ParsedNode::new(ParsedNodeKind::Newline));
        }

        let branch_b_path = container_path.map(|path| format!("{path}.{}.b", branch_container_base_index + idx));

        let (mut branch_content, branch_named, branch_flags) = if has_weave_content(&branch_nodes) {
            let branch_path_prefix = branch_b_path.clone().unwrap_or_else(|| {
                if branch
                    .children()
                    .iter()
                    .any(|node| node.kind() == ParsedNodeKind::Choice && !node.start_content.is_empty())
                {
                    ".^.^".to_owned()
                } else {
                    ".^".to_owned()
                }
            });
            let weave = unwrap_weave_root_container(&export_weave(
                state,
                &branch_path_prefix,
                &branch_nodes,
                scope,
                story,
                false,
                named_paths,
            )?);
            (weave.content.clone(), weave.named_content.clone(), weave.get_count_flags())
        } else {
            (
                export_nodes_with_paths(
                    state,
                    &branch_nodes,
                    scope,
                    story,
                    Some(named_paths),
                    branch_b_path.as_deref(),
                    0,
                )?,
                HashMap::new(),
                0,
            )
        };
        if duplicates_stack_value || (branch.is_else && branch.matching_equality) {
            branch_content.insert(0, command(CommandType::PopEvaluatedValue));
        }

        branch_specs.push((
            !branch.is_else,
            branch.matching_equality && !branch.is_else,
            branch_content,
            branch_named,
            branch_flags,
        ));
    }

    let needs_final_pop = is_switch
        && node.children().first().is_some_and(|branch| branch.condition().is_some())
        && !node.children().last().is_some_and(|branch| branch.is_else);
    let rejoin_nop = command(CommandType::NoOp);

    for (idx, branch) in node.children().iter().enumerate() {
        let (is_conditional, duplicates_stack_value, mut branch_content, branch_named, branch_flags) = branch_specs[idx].clone();

        let mut branch_control: Vec<Rc<dyn RTObject>> = Vec::new();
        if duplicates_stack_value {
            branch_control.push(command(CommandType::Duplicate));
        }

        if !branch.is_true_branch && !branch.is_else {
            if branch.condition().is_some() {
                branch_control.push(command(CommandType::EvalStart));
            }
            if let Some(condition) = branch.condition() {
                export_condition_expression_runtime(condition, scope, story, named_paths, &mut branch_control)?;
            }
            if branch.matching_equality {
                branch_control.push(native(NativeOp::Equal));
            }
            if branch.condition().is_some() {
                branch_control.push(command(CommandType::EvalEnd));
            }
        }

        let branch_divert = Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            is_conditional,
            None,
            None,
        ));
        branch_control.push(branch_divert.clone());

        let return_divert = Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            false,
            None,
            None,
        ));
        state.add_runtime_target_fixup(
            PathFixupSource::Divert(return_divert.clone()),
            rejoin_nop.clone(),
        );
        branch_content.push(return_divert);

        let branch_b = Container::new(Some("b".to_owned()), branch_flags, branch_content, branch_named);
        state.add_runtime_target_fixup(
            PathFixupSource::Divert(branch_divert),
            branch_b.clone(),
        );
        let mut branch_named_content = HashMap::new();
        branch_named_content.insert("b".to_owned(), branch_b);
        let branch_container = Container::new(None, 0, branch_control, branch_named_content);
        content.push(branch_container as Rc<dyn RTObject>);
    }

    if needs_final_pop {
        content.push(command(CommandType::PopEvaluatedValue));
    }

    content.push(rejoin_nop);
    Ok(Container::new(None, 0, content, HashMap::new()))
}

fn export_condition_expression_runtime(
    expression: &ParsedExpression,
    scope: Scope<'_>,
    story: &Story,
    named_paths: &HashMap<String, String>,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    match expression {
        ParsedExpression::Variable(name) => {
            if let Some(path) = resolve_condition_count_path(name, scope, story, named_paths) {
                content.push(Rc::new(VariableReference::from_path_for_count(&path)));
            } else {
                export_expression(expression, story, content)?;
            }
        }
        ParsedExpression::Unary { operator, expression } => {
            export_condition_expression_runtime(expression, scope, story, named_paths, content)?;
            let op = match operator.as_str() {
                "!" => NativeOp::Not,
                "-" => NativeOp::Negate,
                other => {
                    return Err(CompilerError::unsupported_feature(format!(
                        "runtime export does not support unary operator '{other}'"
                    )))
                }
            };
            content.push(native(op));
        }
        ParsedExpression::Binary { left, operator, right } => {
            export_condition_expression_runtime(left, scope, story, named_paths, content)?;
            export_condition_expression_runtime(right, scope, story, named_paths, content)?;
            content.push(native(operator_token(operator)?));
        }
        _ => export_expression(expression, story, content)?,
    }

    Ok(())
}

fn resolve_condition_count_path(
    name: &str,
    scope: Scope<'_>,
    story: &Story,
    named_paths: &HashMap<String, String>,
) -> Option<String> {
    if let Some(path) = resolve_count_path(name, named_paths) {
        return Some(path);
    }

    if story
        .parsed_flows()
        .iter()
        .any(|flow| flow.flow().identifier() == Some(name))
    {
        return Some(name.to_owned());
    }

    let Scope::Flow(flow) = scope else {
        return None;
    };

    if flow
        .children()
        .iter()
        .any(|child| child.flow().identifier() == Some(name))
    {
        return Some(format!("{}.{}", flow.flow().identifier().unwrap_or_default(), name));
    }

    None
}

fn export_assignment(
    node: &ParsedNode,
    scope: Scope<'_>,
    story: &Story,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    let encoded = node.name().ok_or_else(|| {
        CompilerError::unsupported_feature("runtime export assignment missing target")
    })?;
    let (mode, name) = encoded.split_once(':').ok_or_else(|| {
        CompilerError::unsupported_feature(format!(
            "runtime export assignment target has invalid shape '{encoded}'"
        ))
    })?;
    let expression = node.expression().ok_or_else(|| {
        CompilerError::unsupported_feature("runtime export assignment missing expression")
    })?;
    let is_temporary = variable_is_temporary_in_scope(name, scope);

    match mode {
        "Set" => {
            content.push(command(CommandType::EvalStart));
            export_expression(expression, story, content)?;
            content.push(command(CommandType::EvalEnd));
            content.push(variable_assignment(name, !is_temporary, false));
            if expression_contains_function_call(expression) {
                content.push(rt_value("\n"));
            }
        }
        "GlobalDecl" => {
            content.push(command(CommandType::EvalStart));
            export_expression(expression, story, content)?;
            content.push(command(CommandType::EvalEnd));
            content.push(variable_assignment(name, !is_temporary, true));
            if expression_contains_function_call(expression) {
                content.push(rt_value("\n"));
            }
        }
        "TempSet" => {
            content.push(command(CommandType::EvalStart));
            export_expression(expression, story, content)?;
            content.push(command(CommandType::EvalEnd));
            content.push(variable_assignment(name, false, true));
            if expression_contains_function_call(expression) {
                content.push(rt_value("\n"));
            }
        }
        "AddAssign" => {
            content.push(command(CommandType::EvalStart));
            content.push(Rc::new(VariableReference::new(name)));
            export_expression(expression, story, content)?;
            content.push(native(NativeOp::Add));
            content.push(variable_assignment(name, !is_temporary, false));
            content.push(command(CommandType::EvalEnd));
            if expression_contains_function_call(expression) {
                content.push(rt_value("\n"));
            }
        }
        "SubtractAssign" => {
            content.push(command(CommandType::EvalStart));
            content.push(Rc::new(VariableReference::new(name)));
            export_expression(expression, story, content)?;
            content.push(native(NativeOp::Subtract));
            content.push(variable_assignment(name, !is_temporary, false));
            content.push(command(CommandType::EvalEnd));
            if expression_contains_function_call(expression) {
                content.push(rt_value("\n"));
            }
        }
        other => {
            return Err(CompilerError::unsupported_feature(format!(
                "runtime export does not support assignment mode '{other}'"
            )));
        }
    }

    Ok(())
}

fn variable_is_temporary_in_scope(name: &str, scope: Scope<'_>) -> bool {
    let Scope::Flow(flow) = scope else {
        return false;
    };

    flow.flow().arguments().iter().any(|arg| arg.identifier == name)
}

fn expression_contains_function_call(expression: &ParsedExpression) -> bool {
    match expression {
        ParsedExpression::FunctionCall { .. } => true,
        ParsedExpression::Unary { expression, .. } => expression_contains_function_call(expression),
        ParsedExpression::Binary { left, right, .. } => {
            expression_contains_function_call(left) || expression_contains_function_call(right)
        }
        _ => false,
    }
}

fn export_expression_node(
    expression: &crate::parsed_hierarchy::ExpressionNode,
    story: &Story,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    use crate::parsed_hierarchy::{ExpressionNode, NumberValue};

    match expression {
        ExpressionNode::Number(number) => match number.value() {
            NumberValue::Int(value) => content.push(rt_value(*value)),
            NumberValue::Float(value) => content.push(rt_value(*value)),
            NumberValue::Bool(value) => content.push(rt_value(*value)),
        },
        ExpressionNode::StringExpression(string) => {
            content.push(command(CommandType::BeginString));
            for item in string.content().content() {
                let crate::parsed_hierarchy::Content::Text(text) = item;
                content.push(rt_value(text.text()));
            }
            content.push(command(CommandType::EndString));
        }
        ExpressionNode::VariableReference(reference) => {
            let name = reference.name();
            if let Some(constant) = story.const_declaration(&name) {
                export_expression_node(constant.expression(), story, content)?;
            } else {
                content.push(Rc::new(VariableReference::new(&name)));
            }
        }
        ExpressionNode::List(list) => {
            let mut runtime_list = InkList::new();
            if let Some(list_items) = list.item_identifier_list() {
                for item in list_items {
                    insert_resolved_list_item(story, item, &mut runtime_list)?;
                }
            }
            content.push(rt_value(runtime_list));
        }
    }
    Ok(())
}

fn export_expression(
    expression: &ParsedExpression,
    story: &Story,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    match expression {
        ParsedExpression::Bool(value) => content.push(rt_value(*value)),
        ParsedExpression::Int(value) => content.push(rt_value(*value)),
        ParsedExpression::Float(value) => content.push(rt_value(*value)),
        ParsedExpression::String(value) => {
            content.push(command(CommandType::BeginString));
            content.push(rt_value(value.as_str()));
            content.push(command(CommandType::EndString));
        }
        ParsedExpression::StringExpression(nodes) => {
            content.push(command(CommandType::BeginString));
            export_string_expression(nodes, story, content)?;
            content.push(command(CommandType::EndString));
        }
        ParsedExpression::Variable(name) => {
            if let Some(constant) = story.const_declaration(name) {
                export_expression_node(constant.expression(), story, content)?;
            } else {
                content.push(Rc::new(VariableReference::new(name)));
            }
        }
        ParsedExpression::DivertTarget(target) => {
            content.push(rt_value(Path::new_with_components_string(Some(target))));
        }
        ParsedExpression::ListItems(items) => {
            let mut list = InkList::new();
            for item in items {
                insert_resolved_list_item(story, item, &mut list)?;
            }
            content.push(rt_value(list));
        }
        ParsedExpression::EmptyList => content.push(rt_value(InkList::new())),
        ParsedExpression::Unary { operator, expression } => match operator.as_str() {
            "-" => match expression.as_ref() {
                ParsedExpression::Int(value) => content.push(rt_value(-value)),
                ParsedExpression::Float(value) => content.push(rt_value(-value)),
                other => {
                    export_expression(other, story, content)?;
                    content.push(native(NativeOp::Negate));
                }
            },
            "!" => {
                export_expression(expression, story, content)?;
                content.push(native(NativeOp::Not));
            }
            _ => {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support unary operator '{operator}'"
                )));
            }
        },
        ParsedExpression::Binary {
            left,
            operator,
            right,
        } => {
            export_expression(left, story, content)?;
            export_expression(right, story, content)?;
            content.push(native(operator_token(operator)?));
        }
        ParsedExpression::FunctionCall { name, arguments } => {
            if story
                .list_definitions()
                .iter()
                .any(|list| list.identifier() == Some(name.as_str()))
            {
                match arguments.as_slice() {
                    [] => {
                        let list = InkList::new();
                        list.set_initial_origin_names(vec![name.to_owned()]);
                        content.push(rt_value(list));
                    }
                    [argument] => {
                        content.push(rt_value(name.as_str()));
                        export_expression(argument, story, content)?;
                        content.push(command(CommandType::ListFromInt));
                    }
                    _ => {
                        return Err(CompilerError::unsupported_feature(format!(
                            "runtime export does not support list call '{name}' with {} arguments",
                            arguments.len()
                        )));
                    }
                }
            } else if let Some(command_type) = builtin_command(name) {
                for argument in arguments {
                    export_expression(argument, story, content)?;
                }
                content.push(command(command_type));
            } else if let Some(native_op) = native_function(name) {
                for argument in arguments {
                    export_expression(argument, story, content)?;
                }
                content.push(native(native_op));
            } else if let Some(function_flow) = story.parsed_flows().iter().find(|flow| {
                flow.flow().identifier() == Some(name.as_str()) && flow.flow().is_function()
            }) {
                if arguments.is_empty()
                    && let Some(text) = simple_function_text(function_flow)
                {
                    content.push(rt_value(text.as_str()));
                } else {
                    let params = function_flow.flow().arguments();
                    if params.len() != arguments.len() {
                        return Err(CompilerError::unsupported_feature(format!(
                            "runtime export function call '{name}' has {} arguments but expected {}",
                            arguments.len(),
                            params.len()
                        )));
                    }

                    for (argument, parameter) in arguments.iter().zip(params.iter()) {
                        if parameter.is_by_reference {
                            let ParsedExpression::Variable(var_name) = argument else {
                                return Err(CompilerError::unsupported_feature(format!(
                                    "runtime export by-reference function call '{name}' requires variable arguments"
                                )));
                            };
                            content.push(Rc::new(Value::new_variable_pointer(var_name, -1)));
                        } else {
                            export_expression(argument, story, content)?;
                        }
                    }

                    content.push(Rc::new(Divert::new(
                        true,
                        PushPopType::Function,
                        false,
                        0,
                        false,
                        None,
                        Some(name),
                    )));
                }
            } else {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support function call '{name}'"
                )));
            }
        }
    }

    Ok(())
}

fn export_string_expression(
    nodes: &[ParsedNode],
    story: &Story,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    for node in nodes {
        match node.kind() {
            ParsedNodeKind::Text => {
                if let Some(text) = node.text() {
                    content.push(rt_value(text));
                }
            }
            ParsedNodeKind::Newline => content.push(rt_value("\n")),
            ParsedNodeKind::Glue => content.push(Rc::new(Glue::new())),
            ParsedNodeKind::OutputExpression => {
                let expression = node.expression().ok_or_else(|| {
                    CompilerError::unsupported_feature(
                        "runtime export string expression missing expression",
                    )
                })?;
                content.push(command(CommandType::EvalStart));
                export_expression(expression, story, content)?;
                content.push(command(CommandType::EvalOutput));
                content.push(command(CommandType::EvalEnd));
            }
            other => {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support {:?} inside string expressions yet",
                    other
                )))
            }
        }
    }

    Ok(())
}

fn insert_resolved_list_item(
    story: &Story,
    item: &str,
    list: &mut InkList,
) -> Result<(), CompilerError> {
    if let Some((qualified, value)) = story.resolve_list_item(item) {
        list.items.insert(InkListItem::from_full_name(&qualified), value);
        Ok(())
    } else {
        Err(CompilerError::unsupported_feature(format!(
            "runtime export cannot resolve list item '{item}'"
        )))
    }
}

fn simple_function_text(flow: &ParsedFlow) -> Option<String> {
    let mut text = String::new();

    for node in flow.content() {
        match node.kind() {
            ParsedNodeKind::Text => text.push_str(node.text()?),
            ParsedNodeKind::Newline => {}
            _ => return None,
        }
    }

    Some(text)
}

fn operator_token(operator: &str) -> Result<NativeOp, CompilerError> {
    match operator {
        "Add" => Ok(NativeOp::Add),
        "Subtract" => Ok(NativeOp::Subtract),
        "Multiply" => Ok(NativeOp::Multiply),
        "Divide" => Ok(NativeOp::Divide),
        "Modulo" => Ok(NativeOp::Mod),
        "Equal" => Ok(NativeOp::Equal),
        "NotEqual" => Ok(NativeOp::NotEquals),
        "Less" => Ok(NativeOp::Less),
        "LessEqual" => Ok(NativeOp::LessThanOrEquals),
        "Greater" => Ok(NativeOp::Greater),
        "GreaterEqual" => Ok(NativeOp::GreaterThanOrEquals),
        "And" => Ok(NativeOp::And),
        "Or" => Ok(NativeOp::Or),
        "Has" => Ok(NativeOp::Has),
        "Hasnt" => Ok(NativeOp::Hasnt),
        "Intersect" => Ok(NativeOp::Intersect),
        other => Err(CompilerError::unsupported_feature(format!(
            "runtime export does not support binary operator '{other}'"
        ))),
    }
}

fn native_function(name: &str) -> Option<NativeOp> {
    match name {
        "MIN" => Some(NativeOp::Min),
        "MAX" => Some(NativeOp::Max),
        "POW" => Some(NativeOp::Pow),
        "FLOOR" => Some(NativeOp::Floor),
        "CEILING" => Some(NativeOp::Ceiling),
        "INT" => Some(NativeOp::Int),
        "FLOAT" => Some(NativeOp::Float),
        "LIST_VALUE" => Some(NativeOp::ValueOfList),
        "LIST_COUNT" => Some(NativeOp::Count),
        "LIST_MIN" => Some(NativeOp::ListMin),
        "LIST_MAX" => Some(NativeOp::ListMax),
        "LIST_ALL" => Some(NativeOp::All),
        "LIST_INVERT" => Some(NativeOp::Invert),
        _ => None,
    }
}

fn builtin_command(name: &str) -> Option<CommandType> {
    match name {
        "CHOICE_COUNT" => Some(CommandType::ChoiceCount),
        "TURNS" => Some(CommandType::Turns),
        "TURNS_SINCE" => Some(CommandType::TurnsSince),
        "READ_COUNT" => Some(CommandType::ReadCount),
        "RANDOM" => Some(CommandType::Random),
        "SEED_RANDOM" => Some(CommandType::SeedRandom),
        "LIST_RANGE" => Some(CommandType::ListRange),
        "LIST_RANDOM" => Some(CommandType::ListRandom),
        _ => None,
    }
}

fn has_terminal(nodes: &[ParsedNode]) -> bool {
    let last = nodes
        .iter()
        .rev()
        .find(|node| node.kind() != ParsedNodeKind::Newline);

    matches!(
        last.map(|node| node.kind()),
        Some(ParsedNodeKind::Divert | ParsedNodeKind::TunnelReturn)
    )
}

fn container(
    name: Option<String>,
    content: Vec<Rc<dyn RTObject>>,
    named_content: HashMap<String, Rc<Container>>,
    count_flags: i32,
) -> Rc<dyn RTObject> {
    Container::new(name, count_flags, content, named_content)
}

fn named_container(
    name: &str,
    content: Vec<Rc<dyn RTObject>>,
    named_content: HashMap<String, Rc<Container>>,
    count_flags: i32,
) -> Rc<dyn RTObject> {
    Container::new(Some(name.to_owned()), count_flags, content, named_content)
}

fn command(command_type: CommandType) -> Rc<dyn RTObject> {
    Rc::new(ControlCommand::new(command_type))
}

fn rt_value<T: Into<Value>>(value: T) -> Rc<dyn RTObject> {
    Rc::new(Value::new(value))
}

fn rt_int(value: i32) -> Rc<dyn RTObject> {
    Rc::new(Value::new::<i32>(value))
}

fn native(op: NativeOp) -> Rc<dyn RTObject> {
    Rc::new(NativeFunctionCall::new(op))
}

fn variable_assignment(name: &str, is_global: bool, is_new_declaration: bool) -> Rc<dyn RTObject> {
    Rc::new(VariableAssignment::new(
        name,
        is_new_declaration,
        is_global,
    ))
}

fn divert_object(target: &str) -> Rc<dyn RTObject> {
    Rc::new(Divert::new(
        false,
        PushPopType::Tunnel,
        false,
        0,
        false,
        None,
        Some(target),
    ))
}

fn flow_count_flags(story: &Story) -> i32 {
    if !story.count_all_visits {
        0
    } else if story_uses_turn_or_read_count(story) {
        3
    } else {
        1
    }
}

fn weave_count_flags(story: &Story) -> i32 {
    if story_uses_turn_or_read_count(story) {
        7
    } else {
        5
    }
}

fn story_uses_turn_or_read_count(story: &Story) -> bool {
    story.root_nodes().iter().any(node_uses_turn_or_read_count)
        || story
            .parsed_flows()
            .iter()
            .any(flow_uses_turn_or_read_count)
}

fn flow_uses_turn_or_read_count(flow: &ParsedFlow) -> bool {
    flow.content().iter().any(node_uses_turn_or_read_count)
        || flow.children().iter().any(flow_uses_turn_or_read_count)
}

fn node_uses_turn_or_read_count(node: &ParsedNode) -> bool {
    node.expression().is_some_and(expression_uses_turn_or_read_count)
        || node.condition().is_some_and(expression_uses_turn_or_read_count)
        || node.start_content.iter().any(node_uses_turn_or_read_count)
        || node.choice_only_content.iter().any(node_uses_turn_or_read_count)
        || node.children().iter().any(node_uses_turn_or_read_count)
}

fn expression_uses_turn_or_read_count(expression: &ParsedExpression) -> bool {
    match expression {
        ParsedExpression::FunctionCall { name, arguments } => {
            matches!(name.as_str(), "TURNS_SINCE" | "READ_COUNT")
                || arguments.iter().any(expression_uses_turn_or_read_count)
        }
        ParsedExpression::Unary { expression, .. } => expression_uses_turn_or_read_count(expression),
        ParsedExpression::Binary { left, right, .. } => {
            expression_uses_turn_or_read_count(left) || expression_uses_turn_or_read_count(right)
        }
        ParsedExpression::StringExpression(nodes) => nodes.iter().any(node_uses_turn_or_read_count),
        _ => false,
    }
}
