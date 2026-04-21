use std::{cell::RefCell, collections::HashMap, rc::Rc};

use bladeink::{
    ChoicePoint, CommandType, Container, ControlCommand, Divert, Glue, InkList, InkListItem,
    ListDefinition, NativeFunctionCall, NativeOp, Path, PushPopType, RTObject, Value, Void,
    VariableAssignment, VariableReference, story::Story as RuntimeStory,
};

use crate::{
    error::CompilerError,
    parsed_hierarchy::{
        ListDefinition as ParsedListDefinition, ParsedExpression, ParsedFlow, ParsedNode,
        ParsedNodeKind, Story,
    },
};

pub(crate) fn export_story(story: &Story) -> Result<RuntimeStory, CompilerError> {
    let list_defs = export_list_defs(story.list_definitions());
    let mut named_content = HashMap::new();

    for flow in story.parsed_flows() {
        let name = flow.flow().identifier().unwrap_or_default().to_owned();
        named_content.insert(name.clone(), export_flow(flow, story, &name)?);
    }

    if let Some(global_decl) = export_global_decl(story)? {
        named_content.insert("global decl".to_owned(), global_decl);
    }

    let inner_root = export_weave(
        "0",
        story.root_nodes(),
        Scope::Root,
        story,
        true,
        &HashMap::new(),
    )?;
    let root = Container::new(
        None,
        visit_count_flags(story.count_all_visits),
        vec![inner_root, command(CommandType::Done)],
        named_content,
    );

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

fn export_global_decl(story: &Story) -> Result<Option<Rc<Container>>, CompilerError> {
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
    flow: &ParsedFlow,
    story: &Story,
    full_path: &str,
) -> Result<Rc<Container>, CompilerError> {
    if flow.flow().has_parameters() {
        return Err(CompilerError::unsupported_feature(format!(
            "runtime export does not support parameterised/function flow '{}'",
            flow.flow().identifier().unwrap_or_default()
        )));
    }

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
        vec![export_weave(
            &format!("{full_path}.0"),
            flow.content(),
            Scope::Flow(flow),
            story,
            false,
            &flow_named_paths,
        )? as Rc<dyn RTObject>]
    } else {
        export_nodes_with_paths(flow.content(), Scope::Flow(flow), story, Some(&flow_named_paths))?
    };

    if !flow.content().is_empty() && !has_terminal(flow.content()) {
        if flow.flow().is_function() {
            content.push(command(CommandType::PopFunction));
        } else {
            content.push(command(CommandType::Done));
        }
    }

    let mut named = HashMap::new();
    for child in flow.children() {
        let child_name = child.flow().identifier().unwrap_or_default().to_owned();
        named.insert(
            child_name.clone(),
            export_flow(child, story, &format!("{full_path}.{child_name}"))?,
        );
    }

    Ok(Container::new(
        Some(flow.flow().identifier().unwrap_or_default().to_owned()),
        visit_count_flags(story.count_all_visits),
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
    nodes: &[ParsedNode],
    scope: Scope<'_>,
    story: &Story,
) -> Result<Vec<Rc<dyn RTObject>>, CompilerError> {
    export_nodes_with_paths(nodes, scope, story, None)
}

fn export_nodes_with_paths(
    nodes: &[ParsedNode],
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
) -> Result<Vec<Rc<dyn RTObject>>, CompilerError> {
    let mut content = Vec::new();

    for node in nodes {
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
                    content.push(export_divert_conditional(target, scope, story, named_paths));
                } else {
                    content.push(export_divert(target, scope, story, named_paths));
                }
            }
            ParsedNodeKind::TunnelDivert => {
                let target = node.target().ok_or_else(|| {
                    CompilerError::unsupported_feature(
                        "runtime export tunnel divert missing target",
                    )
                })?;
                content.push(export_tunnel_divert(target, scope, story, named_paths));
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
                let resolved = resolve_target(target, scope, story, named_paths);
                content.push(command(CommandType::EvalStart));
                content.push(rt_value(Path::new_with_components_string(Some(&resolved))));
                content.push(command(CommandType::EvalEnd));
                content.push(command(CommandType::PopTunnel));
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
            ParsedNodeKind::Assignment => export_assignment(node, story, &mut content)?,
            ParsedNodeKind::Tag => {
                return Err(CompilerError::unsupported_feature(
                    "runtime export does not support tags yet",
                ));
            }
            ParsedNodeKind::Sequence => {
                content.push(export_sequence(node, scope, story, named_paths)?);
            }
            ParsedNodeKind::Choice
            | ParsedNodeKind::GatherPoint
            | ParsedNodeKind::GatherLabel
            | ParsedNodeKind::Conditional
            | ParsedNodeKind::SwitchConditional
            | ParsedNodeKind::ThreadDivert
            | ParsedNodeKind::ReturnBool
            | ParsedNodeKind::ReturnExpression
            | ParsedNodeKind::ReturnVoid
            | ParsedNodeKind::VoidCall => {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support {:?} yet",
                    node.kind()
                )));
            }
        }
    }

    Ok(content)
}

#[derive(Clone)]
enum PendingItem {
    Object(Rc<dyn RTObject>),
    Container(Rc<PendingContainer>),
}

struct PendingContainer {
    path: String,
    name: Option<String>,
    count_flags: i32,
    content: RefCell<Vec<PendingItem>>,
    named: RefCell<Vec<(String, Rc<PendingContainer>)>>,
}

impl PendingContainer {
    fn new(path: impl Into<String>, name: Option<String>, count_flags: i32) -> Rc<Self> {
        Rc::new(Self {
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

    fn finalize(&self) -> Rc<Container> {
        let mut content = Vec::new();
        for item in self.content.borrow().iter() {
            match item {
                PendingItem::Object(object) => content.push(object.clone()),
                PendingItem::Container(container) => content.push(container.finalize()),
            }
        }

        let mut named = HashMap::new();
        for (key, container) in self.named.borrow().iter() {
            named.insert(key.clone(), container.finalize());
        }

        Container::new(self.name.clone(), self.count_flags, content, named)
    }
}

fn export_weave(
    path_prefix: &str,
    nodes: &[ParsedNode],
    scope: Scope<'_>,
    story: &Story,
    is_root: bool,
    inherited_named_paths: &HashMap<String, String>,
) -> Result<Rc<Container>, CompilerError> {
    let base_depth = nodes
        .iter()
        .filter_map(weave_depth)
        .min()
        .unwrap_or(1);

    let root = PendingContainer::new(path_prefix, None, 0);
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
            let nested = export_weave(
                &nested_path,
                &nodes[nested_start..index],
                scope,
                story,
                false,
                &named_paths,
            )?;
            let nested_pending = PendingContainer::new(nested_path, None, 0);
            for (content_index, item) in nested.content.iter().enumerate() {
                if let Ok(container) = item.clone().into_any().downcast::<Container>() {
                    let child_path = if container.has_valid_name() {
                        format!("{}.{}", nested_pending.path, container.name.as_deref().unwrap_or_default())
                    } else {
                        format!("{}.{}", nested_pending.path, content_index)
                    };
                    nested_pending.push_container(pending_from_container(&container, &child_path));
                } else {
                    nested_pending.push_object(item.clone());
                }
            }
            for (key, value) in &nested.named_content {
                nested_pending.add_named(key.clone(), pending_from_container(value, &format!("{}.{}", nested_pending.path, key)));
            }
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
                let choice_container = PendingContainer::new(&choice_path, Some(choice_key.clone()), 5);
                current.add_named(choice_key.clone(), choice_container.clone());

                export_choice(
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
                    let child_content = export_nodes_with_paths(
                        node.children(),
                        scope,
                        story,
                        Some(&named_paths),
                    )?;
                    for item in child_content {
                        choice_container.push_object(item);
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
            ParsedNodeKind::GatherPoint => {
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
                let gather_container = PendingContainer::new(&gather_path, Some(gather_name.clone()), 5);

                if auto_enter {
                    current.push_container(gather_container.clone());
                } else {
                    root.add_named(gather_name.clone(), gather_container.clone());
                }

                if !node.children().is_empty() {
                    let child_content = export_nodes_with_paths(
                        node.children(),
                        scope,
                        story,
                        Some(&named_paths),
                    )?;
                    for item in child_content {
                        gather_container.push_object(item);
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
                    std::slice::from_ref(node),
                    scope,
                    story,
                    Some(&named_paths),
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
            final_gather_path,
            Some(final_gather_name.clone()),
            5,
        );
        final_gather.push_object(command(CommandType::Done));
        if auto_enter {
            current.push_container(final_gather);
        } else {
            root.add_named(final_gather_name, final_gather);
        }
    }

    Ok(root.finalize())
}

fn export_choice(
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
    let choice_path = format!("{path_prefix}.{choice_key}");
    let has_start = !node.start_content.is_empty();
    let has_choice_only = !node.choice_only_content.is_empty();
    let has_condition = node.condition().is_some();
    let flags = (has_condition as i32)
        + ((has_start as i32) * 2)
        + ((has_choice_only as i32) * 4)
        + ((node.is_invisible_default as i32) * 8)
        + ((node.once_only as i32) * 16);

    if node.is_invisible_default {
        current.push_object(Rc::new(ChoicePoint::new(flags, &choice_path)));
    } else if has_start {
        let sub_index = current.next_content_index();
        let outer_path = format!("{path_prefix}.{sub_index}");
        let outer = PendingContainer::new(&outer_path, None, 0);
        outer.push_object(command(CommandType::EvalStart));
        outer.push_object(rt_value(Path::new_with_components_string(Some(&format!(
            "{outer_path}.$r1"
        )))));
        outer.push_object(variable_assignment("$r", false, true));
        outer.push_object(command(CommandType::BeginString));
        outer.push_object(divert_object(&format!("{outer_path}.s")));
        outer.push_container(PendingContainer::new(format!("{outer_path}.$r1"), Some("$r1".to_owned()), 0));
        outer.push_object(command(CommandType::EndString));

        if has_choice_only {
            outer.push_object(command(CommandType::BeginString));
            for item in export_nodes(&node.choice_only_content, scope, story)? {
                outer.push_object(item);
            }
            outer.push_object(command(CommandType::EndString));
        }

        if let Some(condition) = node.condition() {
            export_condition_expression(condition, story, named_paths, &mut outer.content.borrow_mut())?;
        }

        outer.push_object(command(CommandType::EvalEnd));
        outer.push_object(Rc::new(ChoicePoint::new(flags, &choice_path)));

        let start_container = PendingContainer::new(
            format!("{outer_path}.s"),
            Some("s".to_owned()),
            0,
        );
        for item in export_nodes(&node.start_content, scope, story)? {
            start_container.push_object(item);
        }
        start_container.push_object(variable_divert("$r"));
        outer.add_named("s", start_container);

        current.push_container(outer);

        choice_container.push_object(command(CommandType::EvalStart));
        choice_container.push_object(rt_value(Path::new_with_components_string(Some(&format!(
            "{choice_path}.$r2"
        )))));
        choice_container.push_object(command(CommandType::EvalEnd));
        choice_container.push_object(variable_assignment("$r", false, true));
        choice_container.push_object(divert_object(&format!("{outer_path}.s")));
        choice_container.push_container(PendingContainer::new(
            format!("{choice_path}.$r2"),
            Some("$r2".to_owned()),
            0,
        ));
    } else {
        current.push_object(command(CommandType::EvalStart));
        if has_choice_only {
            current.push_object(command(CommandType::BeginString));
            for item in export_nodes(&node.choice_only_content, scope, story)? {
                current.push_object(item);
            }
            current.push_object(command(CommandType::EndString));
        }
        if let Some(condition) = node.condition() {
            export_condition_expression(condition, story, named_paths, &mut current.content.borrow_mut())?;
        }
        current.push_object(command(CommandType::EvalEnd));
        current.push_object(Rc::new(ChoicePoint::new(flags, &choice_path)));
    }

    Ok(())
}

fn pending_from_container(container: &Rc<Container>, path: &str) -> Rc<PendingContainer> {
    let pending = PendingContainer::new(path, container.name.clone(), container.get_count_flags());
    for (content_index, item) in container.content.iter().enumerate() {
        if let Ok(child_container) = item.clone().into_any().downcast::<Container>() {
            let child_path = if child_container.has_valid_name() {
                format!("{}.{}", path, child_container.name.as_deref().unwrap_or_default())
            } else {
                format!("{}.{}", path, content_index)
            };
            pending.push_container(pending_from_container(&child_container, &child_path));
        } else {
            pending.push_object(item.clone());
        }
    }
    for (key, value) in container.get_named_only_content() {
        pending.add_named(key.clone(), pending_from_container(&value, &format!("{path}.{key}")));
    }
    pending
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
                export_expression(expression, story, content)?;
            }
        }
        _ => export_expression(expression, story, content)?,
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
            ParsedNodeKind::GatherPoint => {
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
        ParsedNodeKind::Choice | ParsedNodeKind::GatherPoint => Some(node.indentation_depth),
        _ => None,
    }
}

fn export_divert(
    target: &str,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
) -> Rc<dyn RTObject> {
    match target {
        "END" => command(CommandType::End),
        "DONE" => command(CommandType::Done),
        _ => {
            let resolved = resolve_target(target, scope, story, named_paths);
            if is_global_variable_divert(&resolved, story) {
                Rc::new(Divert::new(
                    false,
                    PushPopType::Tunnel,
                    false,
                    0,
                    false,
                    Some(resolved),
                    None,
                ))
            } else {
                divert_object(&resolved)
            }
        }
    }
}

fn export_divert_conditional(
    target: &str,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
) -> Rc<dyn RTObject> {
    match target {
        "END" => Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            true,
            None,
            Some("END"),
        )),
        "DONE" => Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            true,
            None,
            Some("DONE"),
        )),
        _ => Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            true,
            None,
            Some(&resolve_target(target, scope, story, named_paths)),
        )),
    }
}

fn export_tunnel_divert(
    target: &str,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
) -> Rc<dyn RTObject> {
    Rc::new(Divert::new(
        true,
        PushPopType::Tunnel,
        false,
        0,
        false,
        None,
        Some(&resolve_target(target, scope, story, named_paths)),
    ))
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
        return target.to_owned();
    }

    let Scope::Flow(flow) = scope else {
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

    if story
        .parsed_flows()
        .iter()
        .any(|candidate| candidate.flow().identifier() == Some(target))
    {
        return target.to_owned();
    }

    target.to_owned()
}

fn export_sequence(
    node: &ParsedNode,
    scope: Scope<'_>,
    story: &Story,
    named_paths: Option<&HashMap<String, String>>,
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
    // For shuffle: treat as stopping for now (TODO: full shuffle)

    seq_items.push(command(CommandType::EvalEnd));

    // For each branch: ev, du, index, ==, /ev, conditional divert to .^.sN
    for el_index in 0..seq_branch_count {
        seq_items.push(command(CommandType::EvalStart));
        seq_items.push(command(CommandType::Duplicate));
        seq_items.push(rt_int(el_index as i32));
        seq_items.push(Rc::new(NativeFunctionCall::new(NativeOp::Equal)));
        seq_items.push(command(CommandType::EvalEnd));

        // Conditional divert to .^.sN (sibling named container)
        let branch_path = format!(".^.s{}", el_index);
        seq_items.push(Rc::new(Divert::new(
            false,
            PushPopType::Function,
            false,
            0,
            true, // is_conditional
            None,
            Some(&branch_path),
        )));
    }

    // The nop sits at seq_items.len()
    let nop_index = seq_items.len();
    let nop_path = format!(".^.{}", nop_index);
    seq_items.push(command(CommandType::NoOp));

    // Build named branch containers with correct back-divert path
    let mut named_branches: HashMap<String, Rc<Container>> = HashMap::new();
    for el_index in 0..seq_branch_count {
        let branch_content: Vec<Rc<dyn RTObject>> = if el_index < num_elements {
            let element_nodes = elements[el_index].children();
            export_nodes_with_paths(element_nodes, scope, story, named_paths)?
        } else {
            // Extra empty branch for "once"
            Vec::new()
        };

        let back_divert = Rc::new(Divert::new(
            false, PushPopType::Function, false, 0, false, None,
            Some(&nop_path),
        ));

        let mut branch_items: Vec<Rc<dyn RTObject>> = Vec::new();
        branch_items.push(command(CommandType::PopEvaluatedValue));
        branch_items.extend(branch_content);
        branch_items.push(back_divert);

        let branch_name = format!("s{}", el_index);
        named_branches.insert(
            branch_name.clone(),
            Container::new(Some(branch_name), 0, branch_items, HashMap::new()),
        );
    }

    Ok(Container::new(
        None,
        5, // visits_should_be_counted | counting_at_start_only
        seq_items,
        named_branches,
    ))
}

fn export_assignment(
    node: &ParsedNode,
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

    match mode {
        "Set" => {
            content.push(command(CommandType::EvalStart));
            export_expression(expression, story, content)?;
            content.push(command(CommandType::EvalEnd));
            content.push(variable_assignment(name, true, false));
        }
        "GlobalDecl" => {
            content.push(command(CommandType::EvalStart));
            export_expression(expression, story, content)?;
            content.push(command(CommandType::EvalEnd));
            content.push(variable_assignment(name, true, true));
        }
        "TempSet" => {
            content.push(command(CommandType::EvalStart));
            export_expression(expression, story, content)?;
            content.push(command(CommandType::EvalEnd));
            content.push(variable_assignment(name, false, true));
        }
        "AddAssign" => {
            content.push(command(CommandType::EvalStart));
            content.push(Rc::new(VariableReference::new(name)));
            export_expression(expression, story, content)?;
            content.push(native(NativeOp::Add));
            content.push(variable_assignment(name, true, false));
            content.push(command(CommandType::EvalEnd));
        }
        "SubtractAssign" => {
            content.push(command(CommandType::EvalStart));
            content.push(Rc::new(VariableReference::new(name)));
            export_expression(expression, story, content)?;
            content.push(native(NativeOp::Subtract));
            content.push(variable_assignment(name, true, false));
            content.push(command(CommandType::EvalEnd));
        }
        other => {
            return Err(CompilerError::unsupported_feature(format!(
                "runtime export does not support assignment mode '{other}'"
            )));
        }
    }

    Ok(())
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
                    if !arguments.is_empty() {
                        return Err(CompilerError::unsupported_feature(format!(
                            "runtime export does not support function call '{name}' with arguments"
                        )));
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

fn visit_count_flags(count_all_visits: bool) -> i32 {
    if count_all_visits { 1 } else { 0 }
}
