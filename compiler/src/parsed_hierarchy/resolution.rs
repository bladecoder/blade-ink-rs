use std::collections::HashSet;

use crate::error::CompilerError;

use super::{ParsedExpression, ParsedFlow, ParsedNode, ParsedNodeKind, Story};

impl Story {
    pub fn resolve_references(&mut self) -> Result<(), CompilerError> {
        self.rebuild_parse_tree_refs();
        for node in &mut self.root_nodes {
            node.resolve_references();
        }
        for flow in &mut self.flows {
            flow.resolve_references();
        }

        validate_empty_diverts(self)?;

        let global_vars: HashSet<String> = self
            .global_initializers()
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        let declared_story_vars = collect_global_declared_vars(self.root_nodes(), self.parsed_flows());
        let mut global_vars = global_vars;
        global_vars.extend(declared_story_vars);
        let const_names: HashSet<String> = self
            .const_declarations()
            .iter()
            .map(|declaration| declaration.name().to_owned())
            .collect();
        let flow_names = collect_all_flow_names(self.parsed_flows());
        let top_level_flow_names: HashSet<String> = self
            .parsed_flows()
            .iter()
            .filter_map(|flow| flow.flow().identifier().map(ToOwned::to_owned))
            .collect();

        for flow in self.parsed_flows() {
            validate_flow(
                flow,
                &top_level_flow_names,
                &top_level_flow_names,
                &flow_names,
                &global_vars,
                &const_names,
                self,
            )?;
        }

        let root_temp_vars = collect_temp_vars(self.root_nodes());
        let mut root_visible_vars = global_vars.clone();
        root_visible_vars.extend(const_names.iter().cloned());
        root_visible_vars.extend(root_temp_vars.iter().cloned());

        let root_scope = ValidationScope {
            visible_vars: root_visible_vars,
            divert_target_vars: global_vars.clone(),
            top_level_flow_names: top_level_flow_names.clone(),
            sibling_flow_names: top_level_flow_names,
            local_labels: collect_named_labels(self.root_nodes())?,
            all_flow_names: flow_names.clone(),
        };
        for temp in &root_temp_vars {
            if flow_names.contains(temp) {
                return Err(CompilerError::invalid_source(format!(
                    "Variable '{}' already exists as a flow or function name",
                    temp
                )));
            }
        }
        validate_node_list(self.root_nodes(), &root_scope, self)?;
        let root_has_weave_context = self.root_nodes().iter().any(|node| {
            matches!(
                node.kind(),
                ParsedNodeKind::Choice | ParsedNodeKind::GatherPoint | ParsedNodeKind::GatherLabel
            )
        });
        for node in self.root_nodes() {
            if matches!(node.kind(), ParsedNodeKind::Conditional | ParsedNodeKind::SwitchConditional)
                && conditional_contains_choice(node)
                && !node.children().iter().any(|branch| branch.is_else)
                && !root_has_weave_context
            {
                return Err(CompilerError::invalid_source(
                    "Nested choice inside a top-level conditional requires a weave context",
                ));
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
struct ValidationScope {
    visible_vars: HashSet<String>,
    divert_target_vars: HashSet<String>,
    top_level_flow_names: HashSet<String>,
    sibling_flow_names: HashSet<String>,
    local_labels: HashSet<String>,
    all_flow_names: HashSet<String>,
}

fn validate_empty_diverts(story: &Story) -> Result<(), CompilerError> {
    for (index, line) in story.source().lines().enumerate() {
        if line.trim() == "->" {
            return Err(CompilerError::invalid_source(
                "Empty diverts (->) are only valid on choices",
            )
            .with_line(index + 1));
        }
    }

    Ok(())
}

fn validate_flow(
    flow: &ParsedFlow,
    top_level_flow_names: &HashSet<String>,
    sibling_flow_names: &HashSet<String>,
    all_flow_names: &HashSet<String>,
    global_vars: &HashSet<String>,
    const_names: &HashSet<String>,
    story: &Story,
) -> Result<(), CompilerError> {
    let flow_name = flow.flow().identifier().unwrap_or_default().to_owned();
    let child_flow_names: HashSet<String> = flow
        .children()
        .iter()
        .filter_map(|child| child.flow().identifier().map(ToOwned::to_owned))
        .collect();

    for child in flow.children() {
        if let Some(child_name) = child.flow().identifier()
            && global_vars.contains(child_name)
        {
            return Err(CompilerError::invalid_source(format!(
                "Flow '{}' collides with existing var '{}'",
                child_name, child_name
            )));
        }
    }

    let mut arg_names = HashSet::new();
    let mut typed_divert_args = HashSet::new();
    for argument in flow.flow().arguments() {
        if !arg_names.insert(argument.identifier.clone()) {
            return Err(CompilerError::invalid_source(format!(
                "Multiple arguments with the same name: '{}'",
                argument.identifier
            )));
        }

        if global_vars.contains(&argument.identifier)
            || const_names.contains(&argument.identifier)
            || all_flow_names.contains(&argument.identifier)
        {
            return Err(CompilerError::invalid_source(format!(
                "Argument '{}' is already used by a var or flow",
                argument.identifier
            )));
        }

        if argument.is_divert_target {
            typed_divert_args.insert(argument.identifier.clone());
        }
    }

    let temp_vars = collect_temp_vars(flow.content());
    for temp in &temp_vars {
        if arg_names.contains(temp) {
            return Err(CompilerError::invalid_source(format!(
                "Variable '{}' already exists as a parameter",
                temp
            )));
        }
        if all_flow_names.contains(temp) {
            return Err(CompilerError::invalid_source(format!(
                "Variable '{}' already exists as a flow or function name",
                temp
            )));
        }
    }

    let mut visible_vars = global_vars.clone();
    visible_vars.extend(const_names.iter().cloned());
    visible_vars.extend(arg_names.iter().cloned());
    visible_vars.extend(temp_vars.iter().cloned());

    let mut divert_target_vars = global_vars.clone();
    divert_target_vars.extend(typed_divert_args.iter().cloned());

    let mut sibling_names = top_level_flow_names.clone();
    sibling_names.extend(sibling_flow_names.iter().cloned());
    sibling_names.insert(flow_name);

    let scope = ValidationScope {
        visible_vars,
        divert_target_vars,
        top_level_flow_names: top_level_flow_names.clone(),
        sibling_flow_names: sibling_names,
        local_labels: collect_named_labels(flow.content())?,
        all_flow_names: all_flow_names.clone(),
    };

    validate_node_list(flow.content(), &scope, story)?;

    for child in flow.children() {
        validate_flow(
            child,
            top_level_flow_names,
            &child_flow_names,
            all_flow_names,
            global_vars,
            const_names,
            story,
        )?;
    }

    Ok(())
}

fn validate_node_list(
    nodes: &[ParsedNode],
    scope: &ValidationScope,
    story: &Story,
) -> Result<(), CompilerError> {
    let mut seen_gather_labels = HashSet::new();
    for node in nodes {
        if matches!(node.kind(), ParsedNodeKind::GatherLabel | ParsedNodeKind::GatherPoint)
            && let Some(name) = node.name()
            && !seen_gather_labels.insert(name.to_owned())
        {
            return Err(CompilerError::invalid_source(format!(
                "A gather label with the same name '{}' already exists in this scope",
                name
            )));
        }
    }

    for node in nodes {
        validate_node(node, scope, story)?;
    }

    Ok(())
}

fn validate_node(
    node: &ParsedNode,
    scope: &ValidationScope,
    story: &Story,
) -> Result<(), CompilerError> {
    if let Some(expression) = node.expression() {
        validate_expression(expression, scope, story)?;
    }
    if let Some(condition) = node.condition() {
        validate_expression(condition, scope, story)?;
    }

    for child in &node.start_content {
        validate_node(child, scope, story)?;
    }
    for child in &node.choice_only_content {
        validate_node(child, scope, story)?;
    }

    match node.kind() {
        ParsedNodeKind::Divert
        | ParsedNodeKind::TunnelDivert
        | ParsedNodeKind::TunnelOnwardsWithTarget
        | ParsedNodeKind::ThreadDivert => {
            if let Some(target) = node.target() {
                validate_divert_target(target, scope, story)?;
                validate_call_arguments(node.arguments(), target, scope, story)?;
            }
        }
        ParsedNodeKind::Conditional | ParsedNodeKind::SwitchConditional => {
            for child in node.children() {
                validate_node_list(child.children(), scope, story)?;
            }
            return Ok(());
        }
        _ => {}
    }

    validate_node_list(node.children(), scope, story)
}

fn validate_expression(
    expression: &ParsedExpression,
    scope: &ValidationScope,
    story: &Story,
) -> Result<(), CompilerError> {
    match expression {
        ParsedExpression::Variable(name) => {
            if name.contains('.') {
                return Ok(());
            }

            if story_has_named_label(story, name) {
                return Ok(());
            }

            if !scope.visible_vars.contains(name)
                && !scope.local_labels.contains(name)
                && !scope.sibling_flow_names.contains(name)
                && !scope.top_level_flow_names.contains(name)
                && !story
                    .list_definitions()
                    .iter()
                    .any(|list| list.identifier() == Some(name.as_str()))
                && story.resolve_list_item(name).is_none()
            {
                return Err(CompilerError::invalid_source(format!(
                    "Variable or read count '{}' not found in this scope",
                    name
                )));
            }
        }
        ParsedExpression::DivertTarget(target) => {
            validate_explicit_divert_target(target, scope, story)?;
        }
        ParsedExpression::Unary { expression, .. } => {
            validate_expression(expression, scope, story)?;
        }
        ParsedExpression::Binary { left, right, .. } => {
            validate_expression(left, scope, story)?;
            validate_expression(right, scope, story)?;
        }
        ParsedExpression::FunctionCall { name, arguments } => {
            validate_call_arguments(arguments, name, scope, story)?;
        }
        ParsedExpression::StringExpression(nodes) => {
            validate_node_list(nodes, scope, story)?;
        }
        ParsedExpression::Bool(_)
        | ParsedExpression::Int(_)
        | ParsedExpression::Float(_)
        | ParsedExpression::String(_)
        | ParsedExpression::ListItems(_)
        | ParsedExpression::EmptyList => {}
    }

    Ok(())
}

fn validate_call_arguments(
    arguments: &[ParsedExpression],
    target_name: &str,
    scope: &ValidationScope,
    story: &Story,
) -> Result<(), CompilerError> {
    if let Some(target_flow) = find_flow_by_name(story.parsed_flows(), target_name) {
        for (argument, parameter) in arguments.iter().zip(target_flow.flow().arguments().iter()) {
            if parameter.is_divert_target {
                match argument {
                    ParsedExpression::Variable(variable_name) => {
                        if !scope.divert_target_vars.contains(variable_name) {
                            return Err(CompilerError::invalid_source(format!(
                                "Since '{}' is used as a variable divert target, it should be marked as: -> {}",
                                variable_name, variable_name
                            )));
                        }
                    }
                    ParsedExpression::DivertTarget(target) => {
                        if scope.divert_target_vars.contains(target) {
                            return Err(CompilerError::invalid_source(format!(
                                "Can't pass '-> {}' to a parameter that already expects a divert target variable",
                                target
                            )));
                        }
                        validate_explicit_divert_target(target, scope, story)?;
                    }
                    _ => {
                        return Err(CompilerError::invalid_source(format!(
                            "Parameter '{}' expects a divert target",
                            parameter.identifier
                        )));
                    }
                }
            } else {
                validate_expression(argument, scope, story)?;
            }
        }
        for argument in arguments.iter().skip(target_flow.flow().arguments().len()) {
            validate_expression(argument, scope, story)?;
        }
    } else {
        for argument in arguments {
            validate_expression(argument, scope, story)?;
        }
    }

    Ok(())
}

fn validate_divert_target(
    target: &str,
    scope: &ValidationScope,
    story: &Story,
) -> Result<(), CompilerError> {
    if target == "END" || target == "DONE" {
        return Ok(());
    }

    if target.contains('.') {
        return validate_explicit_divert_target(target, scope, story);
    }

    if scope.local_labels.contains(target)
        || scope.sibling_flow_names.contains(target)
        || scope.top_level_flow_names.contains(target)
        || scope.all_flow_names.contains(target)
        || scope.divert_target_vars.contains(target)
    {
        return Ok(());
    }

    if scope.visible_vars.contains(target) {
        return Err(CompilerError::invalid_source(format!(
            "Since '{}' is used as a variable divert target, it should be marked as: -> {}",
            target, target
        )));
    }

    if story.resolve_list_item(target).is_some() {
        return Ok(());
    }

    Err(CompilerError::invalid_source(format!(
        "Divert target not found: '{}'",
        target
    )))
}

fn validate_explicit_divert_target(
    target: &str,
    scope: &ValidationScope,
    story: &Story,
) -> Result<(), CompilerError> {
    if target.contains('.') {
        return Ok(());
    }

    if scope.local_labels.contains(target) || scope.sibling_flow_names.contains(target) {
        return Ok(());
    }

    if has_flow_path(story.parsed_flows(), target) {
        return Ok(());
    }

    Err(CompilerError::invalid_source(format!(
        "Divert target not found: '{}'",
        target
    )))
}

fn collect_all_flow_names(flows: &[ParsedFlow]) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_all_flow_names_into(flows, &mut names);
    names
}

fn collect_all_flow_names_into(flows: &[ParsedFlow], names: &mut HashSet<String>) {
    for flow in flows {
        if let Some(name) = flow.flow().identifier() {
            names.insert(name.to_owned());
        }
        collect_all_flow_names_into(flow.children(), names);
    }
}

fn collect_named_labels(nodes: &[ParsedNode]) -> Result<HashSet<String>, CompilerError> {
    let mut names = HashSet::new();
    collect_named_labels_into(nodes, &mut names)?;
    Ok(names)
}

fn collect_named_labels_into(
    nodes: &[ParsedNode],
    names: &mut HashSet<String>,
) -> Result<(), CompilerError> {
    for node in nodes {
        if matches!(
            node.kind(),
            ParsedNodeKind::GatherLabel | ParsedNodeKind::GatherPoint | ParsedNodeKind::Choice
        ) && let Some(name) = node.name()
            && !names.insert(name.to_owned())
        {
            return Err(CompilerError::invalid_source(format!(
                "A label with the same name '{}' already exists in this scope",
                name
            )));
        }

        collect_named_labels_into(&node.start_content, names)?;
        collect_named_labels_into(&node.choice_only_content, names)?;
        collect_named_labels_into(node.children(), names)?;
    }

    Ok(())
}

fn collect_temp_vars(nodes: &[ParsedNode]) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_temp_vars_into(nodes, &mut names);
    names
}

fn collect_global_declared_vars(root_nodes: &[ParsedNode], flows: &[ParsedFlow]) -> HashSet<String> {
    let mut names = HashSet::new();
    collect_global_declared_vars_in_nodes(root_nodes, &mut names);
    for flow in flows {
        collect_global_declared_vars_in_flow(flow, &mut names);
    }
    names
}

fn collect_global_declared_vars_in_flow(flow: &ParsedFlow, names: &mut HashSet<String>) {
    collect_global_declared_vars_in_nodes(flow.content(), names);
    for child in flow.children() {
        collect_global_declared_vars_in_flow(child, names);
    }
}

fn collect_global_declared_vars_in_nodes(nodes: &[ParsedNode], names: &mut HashSet<String>) {
    for node in nodes {
        if node.kind() == ParsedNodeKind::Assignment
            && let Some(encoded) = node.name()
            && let Some((mode, name)) = encoded.split_once(':')
            && mode == "GlobalDecl"
        {
            names.insert(name.to_owned());
        }

        collect_global_declared_vars_in_nodes(&node.start_content, names);
        collect_global_declared_vars_in_nodes(&node.choice_only_content, names);
        collect_global_declared_vars_in_nodes(node.children(), names);
    }
}

fn collect_temp_vars_into(nodes: &[ParsedNode], names: &mut HashSet<String>) {
    for node in nodes {
        if node.kind() == ParsedNodeKind::Assignment
            && let Some(encoded) = node.name()
            && let Some((mode, name)) = encoded.split_once(':')
            && mode == "TempSet"
        {
            names.insert(name.to_owned());
        }

        collect_temp_vars_into(&node.start_content, names);
        collect_temp_vars_into(&node.choice_only_content, names);
        collect_temp_vars_into(node.children(), names);
    }
}

fn has_flow_path(flows: &[ParsedFlow], target: &str) -> bool {
    let mut parts = target.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    let Some(mut current) = flows.iter().find(|flow| flow.flow().identifier() == Some(first)) else {
        return false;
    };

    for part in parts {
        let Some(next) = current
            .children()
            .iter()
            .find(|flow| flow.flow().identifier() == Some(part))
        else {
            return false;
        };
        current = next;
    }

    true
}

fn find_flow_by_name<'a>(flows: &'a [ParsedFlow], name: &str) -> Option<&'a ParsedFlow> {
    for flow in flows {
        if flow.flow().identifier() == Some(name) {
            return Some(flow);
        }
        if let Some(found) = find_flow_by_name(flow.children(), name) {
            return Some(found);
        }
    }

    None
}

fn conditional_contains_choice(node: &ParsedNode) -> bool {
    node.children().iter().any(branch_contains_choice)
}

fn branch_contains_choice(node: &ParsedNode) -> bool {
    node.children().iter().any(|child| {
        child.kind() == ParsedNodeKind::Choice
            || branch_contains_choice(child)
            || child.start_content.iter().any(branch_contains_choice)
            || child.choice_only_content.iter().any(branch_contains_choice)
    })
}

fn story_has_named_label(story: &Story, target: &str) -> bool {
    story.root_nodes().iter().any(|node| node_has_named_label(node, target))
        || story
            .parsed_flows()
            .iter()
            .any(|flow| flow_has_named_label(flow, target))
}

fn flow_has_named_label(flow: &ParsedFlow, target: &str) -> bool {
    flow.content().iter().any(|node| node_has_named_label(node, target))
        || flow.children().iter().any(|child| flow_has_named_label(child, target))
}

fn node_has_named_label(node: &ParsedNode, target: &str) -> bool {
    node.name() == Some(target)
        || node.start_content.iter().any(|child| node_has_named_label(child, target))
        || node.choice_only_content.iter().any(|child| node_has_named_label(child, target))
        || node.children().iter().any(|child| node_has_named_label(child, target))
}
