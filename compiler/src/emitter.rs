use std::collections::{BTreeMap, BTreeSet};

use bladeink::story::INK_VERSION_CURRENT;
use serde_json::{Map, Value, json};

use crate::{
    ast::{
        AssignMode, BinaryOperator, Choice, Condition, Divert, DynamicString, DynamicStringPart,
        Expression, Flow, GlobalVariable, ListDeclaration, Node, ParsedStory, Sequence,
        SequenceMode,
    },
    error::CompilerError,
    parser::inline::{parse_dynamic_string, tokenize_inline_content},
};

pub fn story_to_json_string(
    story: &ParsedStory,
    count_all_visits: bool,
) -> Result<String, CompilerError> {
    let json = story_to_json_value(story, count_all_visits)?;
    serde_json::to_string(&json).map_err(|error| {
        CompilerError::invalid_source(format!("failed to serialize compiled ink: {error}"))
    })
}

pub fn story_to_json_value(
    story: &ParsedStory,
    count_all_visits: bool,
) -> Result<Value, CompilerError> {
    let context = EmitContext::new(story, count_all_visits);
    let root_scope = EmitScope::root(story.flows());

    let mut root_container = emit_nodes(story.root(), &root_scope, &context)?;
    // Only add the default done/g-0 container if choices haven't already created a g-0 continuation.
    if !root_container.named.contains_key("g-0") {
        root_container.push(json!(["done", {"#n": "g-0"}]));
    }

    let mut named_content = Map::new();

    for flow in story.flows() {
        named_content.insert(flow.name.clone(), emit_flow(flow, &context)?);
    }

    if !story.globals().is_empty() || !story.list_declarations().is_empty() {
        named_content.insert(
            "global decl".to_owned(),
            emit_global_declarations(story.globals(), story.list_declarations())?
                .into_json_array(None, None)?,
        );
    }

    // Build listDefs
    let mut list_defs = Map::new();
    for list_decl in story.list_declarations() {
        let mut items = Map::new();
        for (item_name, value, _selected) in &list_decl.items {
            items.insert(item_name.clone(), json!(value));
        }
        list_defs.insert(list_decl.name.clone(), Value::Object(items));
    }

    Ok(json!({
        "inkVersion": INK_VERSION_CURRENT,
        "root": [
            root_container.into_json_array(None, None)?,
            "done",
            if named_content.is_empty() {
                Value::Null
            } else {
                Value::Object(named_content)
            }
        ],
        "listDefs": Value::Object(list_defs)
    }))
}

#[derive(Debug, Default)]
struct EmittedContainer {
    content: Vec<Value>,
    named: Map<String, Value>,
}

#[derive(Clone)]
struct EmitScope {
    path: String,
    top_flow_name: Option<String>,
    child_flow_names: BTreeSet<String>,
    choice_label_targets: BTreeMap<String, String>,
    /// Number of tokens that will be prepended (e.g. function parameters)
    /// before the content emitted by emit_nodes. This offset is added when
    /// computing absolute indices (e.g. conditional rejoin targets).
    param_offset: usize,
    /// Parameter names for the current flow (used to detect variable diverts to params)
    param_names: BTreeSet<String>,
}

struct EmitContext {
    global_variables: BTreeSet<String>,
    top_flow_names: BTreeSet<String>,
    count_all_visits: bool,
    /// Set of LIST declaration names (to detect list-typed function calls)
    list_names: BTreeSet<String>,
    /// For each list name, a map from bare item name → (qualified name, value)
    list_items: std::collections::BTreeMap<String, Vec<(String, u32)>>,
    /// Set of EXTERNAL function declaration names
    external_functions: BTreeSet<String>,
}

impl EmittedContainer {
    fn push(&mut self, value: Value) {
        self.content.push(value);
    }

    fn insert_named(&mut self, name: String, value: Value) {
        self.named.insert(name, value);
    }

    fn into_json_array(
        self,
        name: Option<&str>,
        count_flags: Option<i32>,
    ) -> Result<Value, CompilerError> {
        let mut values = self.content;
        let has_name = name.is_some();
        let has_flags = count_flags.unwrap_or_default() > 0;

        if !self.named.is_empty() || has_name || has_flags {
            let mut terminator = self.named;

            if let Some(flags) = count_flags.filter(|flags| *flags > 0) {
                terminator.insert("#f".to_owned(), json!(flags));
            }

            if let Some(name) = name {
                terminator.insert("#n".to_owned(), json!(name));
            }

            values.push(Value::Object(terminator));
        } else {
            values.push(Value::Null);
        }

        Ok(Value::Array(values))
    }
}

impl EmitContext {
    fn new(story: &ParsedStory, count_all_visits: bool) -> Self {
        let mut list_items = std::collections::BTreeMap::new();
        for list_decl in story.list_declarations() {
            let items: Vec<(String, u32)> = list_decl
                .items
                .iter()
                .map(|(name, value, _)| (format!("{}.{}", list_decl.name, name), *value))
                .collect();
            list_items.insert(list_decl.name.clone(), items);
        }
        Self {
            global_variables: story.globals().iter().map(|var| var.name.clone()).collect(),
            top_flow_names: story.flows().iter().map(|flow| flow.name.clone()).collect(),
            count_all_visits,
            list_names: story
                .list_declarations()
                .iter()
                .map(|l| l.name.clone())
                .collect(),
            list_items,
            external_functions: story.external_functions.iter().cloned().collect(),
        }
    }

    /// Look up a bare item name (e.g. "b") across all lists.
    /// Returns the qualified name and value if found.
    fn resolve_list_item(&self, bare_name: &str) -> Option<(String, u32)> {
        // If already qualified (contains '.'), use as-is
        if bare_name.contains('.') {
            for items in self.list_items.values() {
                if let Some((qname, val)) = items.iter().find(|(q, _)| q == bare_name) {
                    return Some((qname.clone(), *val));
                }
            }
            return None;
        }
        for items in self.list_items.values() {
            if let Some((qname, val)) = items
                .iter()
                .find(|(q, _)| q.split('.').next_back() == Some(bare_name))
            {
                return Some((qname.clone(), *val));
            }
        }
        None
    }
}

impl EmitScope {
    fn root(flows: &[Flow]) -> Self {
        Self {
            path: "0".to_owned(),
            top_flow_name: None,
            child_flow_names: flows.iter().map(|flow| flow.name.clone()).collect(),
            choice_label_targets: BTreeMap::new(),
            param_offset: 0,
            param_names: BTreeSet::new(),
        }
    }

    fn child_flow(&self, child: &Flow) -> Self {
        let path = if self.path == "0" {
            child.name.clone()
        } else {
            format!("{}.{}", self.path, child.name)
        };

        Self {
            path,
            top_flow_name: self
                .top_flow_name
                .clone()
                .or_else(|| Some(child.name.clone())),
            child_flow_names: child
                .children
                .iter()
                .map(|nested| nested.name.clone())
                .collect(),
            choice_label_targets: BTreeMap::new(),
            param_offset: child.parameters.len(),
            param_names: child.parameters.iter().cloned().collect(),
        }
    }

    fn choice_branch(&self, branch_name: &str) -> Self {
        Self {
            path: format!("{}.{}", self.path, branch_name),
            top_flow_name: self.top_flow_name.clone(),
            child_flow_names: self.child_flow_names.clone(),
            choice_label_targets: self.choice_label_targets.clone(),
            param_offset: 0,
            param_names: self.param_names.clone(),
        }
    }

    fn with_choice_labels(&self, labels: BTreeMap<String, String>) -> Self {
        // Merge: start with existing labels and overlay the new ones
        let mut merged = self.choice_label_targets.clone();
        merged.extend(labels);
        Self {
            path: self.path.clone(),
            top_flow_name: self.top_flow_name.clone(),
            child_flow_names: self.child_flow_names.clone(),
            choice_label_targets: merged,
            param_offset: self.param_offset,
            param_names: self.param_names.clone(),
        }
    }

    fn resolve_divert_target(&self, target: &str, context: &EmitContext) -> String {
        if target == "END" || target == "DONE" || target.contains('.') {
            return target.to_owned();
        }

        if let Some(choice_target) = self.resolve_choice_label(target) {
            return choice_target.to_owned();
        }

        if context.global_variables.contains(target) && !context.top_flow_names.contains(target) {
            return target.to_owned();
        }

        if self.child_flow_names.contains(target)
            && let Some(top_flow_name) = &self.top_flow_name
        {
            return format!("{top_flow_name}.{target}");
        }

        target.to_owned()
    }

    fn is_variable_divert(&self, target: &str, context: &EmitContext) -> bool {
        self.param_names.contains(target)
            || (context.global_variables.contains(target)
                && !context.top_flow_names.contains(target))
    }

    fn resolve_choice_label(&self, label: &str) -> Option<&str> {
        self.choice_label_targets.get(label).map(String::as_str)
    }
}

fn emit_global_declarations(
    globals: &[GlobalVariable],
    list_decls: &[ListDeclaration],
) -> Result<EmittedContainer, CompilerError> {
    let mut container = EmittedContainer::default();
    container.push(json!("ev"));

    for global in globals {
        emit_expression(&global.initial_value, &mut container.content);
        container.push(json!({ "VAR=": global.name }));
    }

    for list_decl in list_decls {
        // Emit the initial value: only the items that are marked as selected
        let mut selected = Map::new();
        for (item_name, value, initially_selected) in &list_decl.items {
            if *initially_selected {
                let key = format!("{}.{}", list_decl.name, item_name);
                selected.insert(key, json!(value));
            }
        }
        if selected.is_empty() {
            // No selected items → include origins so the runtime knows which list this belongs to
            container.push(json!({ "list": Value::Object(selected), "origins": [list_decl.name] }));
        } else {
            container.push(json!({ "list": Value::Object(selected) }));
        }
        container.push(json!({ "VAR=": list_decl.name }));
    }

    container.push(json!("/ev"));
    container.push(json!("end"));

    Ok(container)
}

fn emit_flow(flow: &Flow, context: &EmitContext) -> Result<Value, CompilerError> {
    let parent_scope = EmitScope::root(&[]);
    let scope = parent_scope.child_flow(flow);
    let mut container = emit_nodes(&flow.nodes, &scope, context)?;

    prepend_parameters(&mut container, &flow.parameters);

    if container.content.is_empty() && !flow.children.is_empty() {
        let target = if let Some(top_flow_name) = &scope.top_flow_name {
            format!("{top_flow_name}.{}", flow.children[0].name)
        } else {
            flow.children[0].name.clone()
        };
        container.push(json!({"->": target}));
    }

    for child in &flow.children {
        container.insert_named(
            child.name.clone(),
            emit_nested_flow(child, &scope, context)?,
        );
    }

    let count_flags = if flow.is_function {
        1
    } else if context.count_all_visits {
        3
    } else {
        0
    };

    container.into_json_array(None, Some(count_flags))
}

fn emit_nested_flow(
    flow: &Flow,
    parent_scope: &EmitScope,
    context: &EmitContext,
) -> Result<Value, CompilerError> {
    let scope = parent_scope.child_flow(flow);
    let mut container = emit_nodes(&flow.nodes, &scope, context)?;

    prepend_parameters(&mut container, &flow.parameters);

    if container.content.is_empty() && !flow.children.is_empty() {
        let target = if let Some(top_flow_name) = &scope.top_flow_name {
            format!("{top_flow_name}.{}", flow.children[0].name)
        } else {
            flow.children[0].name.clone()
        };
        container.push(json!({"->": target}));
    }

    for child in &flow.children {
        container.insert_named(
            child.name.clone(),
            emit_nested_flow(child, &scope, context)?,
        );
    }

    let count_flags = if flow.is_function {
        1
    } else if context.count_all_visits {
        3
    } else {
        0
    };

    container.into_json_array(None, Some(count_flags))
}

fn prepend_parameters(container: &mut EmittedContainer, parameters: &[String]) {
    if parameters.is_empty() {
        return;
    }

    let mut prefix: Vec<Value> = parameters
        .iter()
        .rev()
        .map(|parameter| json!({"temp=": parameter}))
        .collect();
    prefix.append(&mut container.content);
    container.content = prefix;
}

/// Pre-scan all nodes (including continuations) to collect every choice label
/// as an absolute path. This allows cross-block label references (e.g., {greet}
/// in a second choice block referencing a label from the first block).
fn collect_all_choice_labels(nodes: &[Node], scope: &EmitScope) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    collect_choice_labels_recursive(nodes, scope, &mut labels, &mut 0);
    labels
}

fn collect_choice_labels_recursive(
    nodes: &[Node],
    scope: &EmitScope,
    labels: &mut BTreeMap<String, String>,
    choice_index: &mut usize,
) {
    let mut i = 0;
    while i < nodes.len() {
        match &nodes[i] {
            Node::Choice(choice) => {
                if let Some(label) = &choice.label {
                    labels.insert(label.clone(), format!("{}.c-{}", scope.path, *choice_index));
                }
                *choice_index += 1;
                i += 1;

                // Find where the choice block ends (non-Choice node)
                let block_start_ci = *choice_index - 1; // index of first choice in block
                // collect any remaining adjacent choices
                while i < nodes.len() && matches!(nodes[i], Node::Choice(_)) {
                    if let Node::Choice(c) = &nodes[i]
                        && let Some(label) = &c.label
                    {
                        labels.insert(label.clone(), format!("{}.c-{}", scope.path, *choice_index));
                    }
                    *choice_index += 1;
                    i += 1;
                }
                // Recurse into the continuation as g-<first_choice_in_block>
                let continuation = &nodes[i..];
                if !continuation.is_empty() {
                    let g_name = format!("g-{}", block_start_ci);
                    let child_scope = scope.choice_branch(&g_name);
                    // If the continuation starts with a GatherLabel, add it as an alias for g-N
                    if let Some(Node::GatherLabel(lbl)) = continuation.first() {
                        labels.insert(lbl.clone(), format!("{}.{}", scope.path, g_name));
                    }
                    let mut child_index = 0;
                    collect_choice_labels_recursive(
                        continuation,
                        &child_scope,
                        labels,
                        &mut child_index,
                    );
                }
                return; // choice block consumes the rest via continuation
            }
            Node::GatherLabel(label) => {
                // Standalone gather label (loop-back point): record its path so
                // DivertTarget expressions can resolve it.
                labels.insert(label.clone(), format!("{}.{}", scope.path, label));
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
}

fn emit_nodes(
    nodes: &[Node],
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<EmittedContainer, CompilerError> {
    emit_nodes_with_continuation(nodes, scope, context, None)
}

fn emit_nodes_with_continuation(
    nodes: &[Node],
    scope: &EmitScope,
    context: &EmitContext,
    fallback_continuation: Option<&str>,
) -> Result<EmittedContainer, CompilerError> {
    let mut out = EmittedContainer::default();
    let mut next_choice_index = 0;

    // Pre-scan all choice blocks to collect labels with absolute paths,
    // so that labels from earlier blocks are available in later blocks.
    let all_labels = collect_all_choice_labels(nodes, scope);
    let scope = &scope.with_choice_labels(all_labels);

    let mut index = 0;
    while index < nodes.len() {
        match &nodes[index] {
            Node::Text(text) => out.push(json!(format!("^{text}"))),
            Node::OutputExpression(expression) => {
                out.push(json!("ev"));
                emit_expression_ctx(expression, &mut out.content, Some(context));
                out.push(json!("out"));
                out.push(json!("/ev"));
            }
            Node::Newline => out.push(json!("\n")),
            Node::Tag(tag) => emit_tag(tag, &mut out.content, scope, context)?,
            Node::Glue => out.push(json!("<>")),
            Node::Sequence(_) => {
                let block_start = index;
                while index < nodes.len() && matches!(nodes[index], Node::Sequence(_)) {
                    index += 1;
                }
                emit_sequence_block(
                    &mut out,
                    &nodes[block_start..index],
                    scope,
                    next_choice_index,
                    context,
                )?;
                continue;
            }
            Node::Divert(divert) => emit_divert(&mut out, divert, scope, context),
            Node::TunnelDivert { target, args, .. } => {
                let is_var = !context.top_flow_names.contains(target);
                if !args.is_empty() {
                    out.push(json!("ev"));
                    for arg in args {
                        emit_expression_ctx(arg, &mut out.content, Some(context));
                    }
                    out.push(json!("/ev"));
                }
                if is_var {
                    out.push(json!({"->t->": target, "var": true}));
                } else {
                    out.push(json!({"->t->": target}));
                }
            }
            Node::TunnelReturn => {
                out.push(json!("ev"));
                out.push(json!("void"));
                out.push(json!("/ev"));
                out.push(json!("->->"));
            }
            Node::ThreadDivert(divert) => {
                // <- target(args): ev, arg1, arg2, ..., /ev, "thread", {->: target}
                if !divert.arguments.is_empty() {
                    out.push(json!("ev"));
                    for arg in &divert.arguments {
                        // Resolve DivertTarget arguments with full scope path
                        if let Expression::DivertTarget(target) = arg {
                            let resolved = scope.resolve_divert_target(target, context);
                            out.push(json!({"^->": resolved}));
                        } else {
                            emit_expression_ctx(arg, &mut out.content, Some(context));
                        }
                    }
                    out.push(json!("/ev"));
                }
                out.push(json!("thread"));
                let resolved = scope.resolve_divert_target(&divert.target, context);
                out.push(json!({"->": resolved}));
            }
            Node::ReturnBool(value) => {
                out.push(json!("ev"));
                out.push(json!(value));
                out.push(json!("/ev"));
                out.push(json!("~ret"));
            }
            Node::ReturnVoid => {
                out.push(json!("ev"));
                out.push(json!("void"));
                out.push(json!("/ev"));
                out.push(json!("~ret"));
            }
            Node::ReturnExpr(expression) => {
                out.push(json!("ev"));
                emit_expression_ctx(expression, &mut out.content, Some(context));
                out.push(json!("/ev"));
                out.push(json!("~ret"));
            }
            Node::Conditional {
                condition,
                when_true,
                when_false,
            } => out.content.extend(emit_conditional(
                condition,
                when_true,
                when_false.as_deref(),
                scope,
                out.content.len() + scope.param_offset,
                context,
            )?),
            Node::SwitchConditional { value, branches } => {
                let switch_index = out.content.len() + scope.param_offset;
                out.content.extend(emit_switch_conditional(
                    value,
                    branches,
                    scope,
                    switch_index,
                    context,
                )?)
            }
            Node::Assignment {
                variable_name,
                expression,
                mode,
            } => emit_assignment(variable_name, expression, mode, &mut out, context),
            Node::VoidCall { name, args } => {
                out.push(json!("ev"));
                for arg in args {
                    emit_expression_ctx(arg, &mut out.content, Some(context));
                }
                // Only SEED_RANDOM makes sense as a void call (it has a side effect).
                // All other built-ins return a value and are meaningless as void statements.
                let builtin_token: Option<&str> = match name.as_str() {
                    "SEED_RANDOM" => Some("srnd"),
                    _ => None,
                };
                if let Some(token) = builtin_token {
                    out.push(json!(token));
                } else if context.external_functions.contains(name) {
                    let ex_args = args.len() as i32;
                    if ex_args > 0 {
                        out.push(json!({"x()": name, "exArgs": ex_args}));
                    } else {
                        out.push(json!({"x()": name}));
                    }
                } else {
                    out.push(json!({"f()": name}));
                }
                out.push(json!("pop"));
                out.push(json!("/ev"));
            }
            Node::Choice(_) => {
                let block_start = index;
                while index < nodes.len() && matches!(nodes[index], Node::Choice(_)) {
                    index += 1;
                }
                let continuation = &nodes[index..];
                emit_choice_block(
                    &mut out,
                    &nodes[block_start..index],
                    continuation,
                    scope,
                    &mut next_choice_index,
                    context,
                    fallback_continuation,
                )?;
                break;
            }
            Node::GatherLabel(label) => {
                // Standalone gather label (loop-back point before choices):
                // Emit remaining nodes as an indexed sub-container with #n: label in its
                // terminator. This makes the runtime enter it sequentially AND allows the
                // label to be resolved as scope.path + "." + label via named_content lookup.
                let remaining = &nodes[index + 1..];
                let sub_scope = scope.choice_branch(label);
                let sub_container = emit_nodes_with_continuation(
                    remaining,
                    &sub_scope,
                    context,
                    fallback_continuation,
                )?;
                let sub_value = sub_container.into_json_array(Some(label), Some(7))?;
                out.push(sub_value); // push as indexed content (sequential execution)
                break; // all remaining nodes consumed by the sub-container
            }
        }

        index += 1;
    }

    Ok(out)
}

fn emit_choice_block(
    out: &mut EmittedContainer,
    choices: &[Node],
    continuation: &[Node],
    scope: &EmitScope,
    next_choice_index: &mut usize,
    context: &EmitContext,
    fallback_continuation: Option<&str>,
) -> Result<(), CompilerError> {
    let mut choice_labels = BTreeMap::new();
    for (offset, choice) in choices.iter().enumerate() {
        let Node::Choice(choice) = choice else {
            continue;
        };
        if let Some(label) = &choice.label {
            // Store the absolute path so diverts to labels work regardless of scope depth
            choice_labels.insert(
                label.clone(),
                format!("{}.c-{}", scope.path, *next_choice_index + offset),
            );
        }
    }
    // Detect gather label for the continuation so it can be added to choice_labels
    let gather_label: Option<String> = if let Some(Node::GatherLabel(lbl)) = continuation.first() {
        let g_name = format!("g-{}", *next_choice_index);
        let path = format!("{}.{}", scope.path, g_name);
        choice_labels.insert(lbl.clone(), path);
        Some(lbl.clone())
    } else {
        None
    };

    let block_scope = scope.with_choice_labels(choice_labels);

    let continuation_path: Option<String> = if continuation.is_empty() {
        // No explicit continuation nodes — use fallback (parent's continuation) if available
        fallback_continuation.map(|s| s.to_owned())
    } else {
        let name = format!("g-{}", *next_choice_index);
        let continuation_scope = block_scope.choice_branch(&name);
        // Strip leading GatherLabel — it's already handled via into_json_array(gather_label)
        let continuation_body = if gather_label.is_some() {
            &continuation[1..]
        } else {
            continuation
        };
        // Pass fallback through so the gather content can inherit the outer continuation
        let mut gather_container = emit_nodes_with_continuation(
            continuation_body,
            &continuation_scope,
            context,
            fallback_continuation,
        )?;
        // If the gather doesn't end terminally AND doesn't have nested choices,
        // append the fallback continuation divert.
        // If it has nested choices, those will carry the fallback themselves.
        let has_nested_choices_in_continuation = continuation_body
            .iter()
            .any(|n| matches!(n, Node::Choice(_)));
        if let Some(fb) = fallback_continuation
            && !branch_has_terminal_content(continuation_body)
            && !has_nested_choices_in_continuation
        {
            gather_container.push(json!({"->": fb}));
        }
        let continuation_value = gather_container.into_json_array(gather_label.as_deref(), None)?;
        out.insert_named(name.clone(), continuation_value);
        Some(format!("{}.{}", scope.path, name))
    };

    for choice in choices {
        let Node::Choice(choice) = choice else {
            continue;
        };
        emit_choice(
            out,
            choice,
            &block_scope,
            *next_choice_index,
            continuation_path.as_deref(),
            context,
        )?;
        *next_choice_index += 1;
    }

    Ok(())
}

fn emit_sequence_block(
    out: &mut EmittedContainer,
    sequences: &[Node],
    scope: &EmitScope,
    _next_index: usize,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    for sequence in sequences {
        let Node::Sequence(sequence) = sequence else {
            continue;
        };
        out.push(emit_sequence(sequence, scope, context)?);
    }

    Ok(())
}

fn emit_sequence(
    sequence: &Sequence,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<Value, CompilerError> {
    let has_once_fallthrough = matches!(
        sequence.mode,
        SequenceMode::Once | SequenceMode::ShuffleOnce
    );
    let authored_branch_count = sequence.branches.len();
    let branch_count = authored_branch_count + usize::from(has_once_fallthrough);
    let max_index = branch_count.saturating_sub(1) as i32;
    let mut out = vec![json!("ev"), json!("visit")];

    match sequence.mode {
        SequenceMode::Stopping | SequenceMode::Once => {
            out.push(json!(max_index));
            out.push(json!("MIN"));
        }
        SequenceMode::Cycle => {
            out.push(json!(branch_count as i32));
            out.push(json!("%"));
        }
        SequenceMode::Shuffle => {
            out.push(json!(branch_count as i32));
            out.push(json!("seq"));
        }
        SequenceMode::ShuffleOnce | SequenceMode::ShuffleStopping => {
            out.push(json!(max_index));
            out.push(json!("MIN"));
            out.push(json!("du"));
            out.push(json!(max_index));
            out.push(json!("=="));
            out.push(json!({"->": ".^.10", "c": true}));
            out.push(json!(max_index));
            out.push(json!("seq"));
            out.push(json!("nop"));
        }
    }
    out.push(json!("/ev"));

    for (index, _) in sequence.branches.iter().enumerate() {
        out.push(json!("ev"));
        out.push(json!("du"));
        out.push(json!(index as i32));
        out.push(json!("=="));
        out.push(json!("/ev"));
        out.push(json!({"->": format!(".^.s{index}"), "c": true}));
    }
    let rejoin_index = out.len();
    out.push(json!("nop"));

    let mut named = Map::new();
    for (index, branch) in sequence.branches.iter().enumerate() {
        let branch_scope = scope.choice_branch(&format!("s{index}"));
        let mut branch_container = emit_nodes(branch, &branch_scope, context)?;
        branch_container.content.insert(0, json!("pop"));
        branch_container.push(json!({"->": format!(".^.^.{rejoin_index}")}));
        named.insert(
            format!("s{index}"),
            branch_container.into_json_array(None, None)?,
        );
    }
    if has_once_fallthrough {
        let mut branch_container = EmittedContainer::default();
        branch_container.push(json!("pop"));
        branch_container.push(json!({"->": format!(".^.^.{rejoin_index}")}));
        named.insert(
            format!("s{authored_branch_count}"),
            branch_container.into_json_array(None, None)?,
        );
    }
    named.insert("#f".to_owned(), json!(5));

    out.push(Value::Object(named));
    Ok(Value::Array(out))
}

fn emit_divert(
    out: &mut EmittedContainer,
    divert: &Divert,
    scope: &EmitScope,
    context: &EmitContext,
) {
    let resolved_target = scope.resolve_divert_target(&divert.target, context);

    if resolved_target == "END" {
        out.push(json!("end"));
        return;
    }

    if resolved_target == "DONE" {
        out.push(json!("done"));
        return;
    }

    if !divert.arguments.is_empty() {
        out.push(json!("ev"));
        for argument in &divert.arguments {
            emit_expression_ctx(argument, &mut out.content, Some(context));
        }
        out.push(json!("/ev"));
    }

    if scope.is_variable_divert(&resolved_target, context) {
        out.push(json!({"->": resolved_target, "var": true}));
    } else {
        out.push(json!({"->": resolved_target}));
    }
}

fn emit_assignment(
    variable_name: &str,
    expression: &Expression,
    mode: &AssignMode,
    out: &mut EmittedContainer,
    context: &EmitContext,
) {
    match mode {
        AssignMode::Set => {
            out.push(json!("ev"));
            emit_expression_ctx(expression, &mut out.content, Some(context));
            out.push(json!("/ev"));
            out.push(json!({"VAR=": variable_name, "re": true}));
        }
        AssignMode::TempSet => {
            out.push(json!("ev"));
            emit_expression_ctx(expression, &mut out.content, Some(context));
            out.push(json!("/ev"));
            out.push(json!({"temp=": variable_name}));
        }
        AssignMode::AddAssign => {
            out.push(json!("ev"));
            emit_expression_ctx(
                &Expression::Variable(variable_name.to_owned()),
                &mut out.content,
                Some(context),
            );
            emit_expression_ctx(expression, &mut out.content, Some(context));
            out.push(json!("+"));
            out.push(json!({"VAR=": variable_name, "re": true}));
            out.push(json!("/ev"));
        }
        AssignMode::SubtractAssign => {
            out.push(json!("ev"));
            emit_expression_ctx(
                &Expression::Variable(variable_name.to_owned()),
                &mut out.content,
                Some(context),
            );
            emit_expression_ctx(expression, &mut out.content, Some(context));
            out.push(json!("-"));
            out.push(json!({"VAR=": variable_name, "re": true}));
            out.push(json!("/ev"));
        }
    }
}

fn emit_choice(
    out: &mut EmittedContainer,
    choice: &Choice,
    scope: &EmitScope,
    choice_index: usize,
    continuation_path: Option<&str>,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    let has_ev_content = !choice.start_text.is_empty()
        || !choice.start_tags.is_empty()
        || !choice.choice_only_text.is_empty()
        || !choice.choice_only_tags.is_empty()
        || !choice.conditions.is_empty();

    if has_ev_content {
        out.push(json!("ev"));
        emit_choice_text_segment(
            &choice.start_text,
            &choice.start_tags,
            &mut out.content,
            scope,
            context,
        )?;
        emit_choice_text_segment(
            &choice.choice_only_text,
            &choice.choice_only_tags,
            &mut out.content,
            scope,
            context,
        )?;
        for (index, condition) in choice.conditions.iter().enumerate() {
            emit_condition(condition, &mut out.content, scope, context)?;
            if index > 0 {
                out.push(json!("&&"));
            }
        }
        out.push(json!("/ev"));
    }

    let branch_name = format!("c-{choice_index}");
    let branch_scope = scope.choice_branch(&branch_name);
    out.push(json!({"*": format!(".^.{}", branch_name), "flg": choice_flags(choice)}));

    let mut branch_nodes = Vec::new();
    let mut body_already_emitted = false;
    if let Some(selected_text) = &choice.selected_text {
        let recovered_inline_divert = if choice.body.is_empty() {
            recover_selected_text_inline_divert(selected_text)
        } else {
            None
        };

        if choice.has_choice_only_content
            && !choice.has_start_content
            && matches!(choice.body.as_slice(), [Node::Divert(_)])
        {
            branch_nodes.extend(tokenize_inline_content(&format!(" {selected_text}"))?);
            branch_nodes.extend(choice.body.clone());
            branch_nodes.push(Node::Newline);
            body_already_emitted = true;
        } else if let Some((text, target)) = recovered_inline_divert {
            if !text.is_empty() {
                branch_nodes.extend(tokenize_inline_content(&text)?);
            }
            branch_nodes.push(Node::Divert(Divert {
                target,
                arguments: Vec::new(),
            }));
            body_already_emitted = true;
        } else {
            branch_nodes.extend(tokenize_inline_content(selected_text)?);
        }
        branch_nodes.extend(choice.selected_tags.iter().cloned().map(Node::Tag));
        if !body_already_emitted {
            // Only skip the newline if the body is a terminal divert to END/DONE
            let body_is_terminal_divert = matches!(
                choice.body.as_slice(),
                [Node::Divert(d)] if d.target == "END" || d.target == "DONE"
            );
            if !body_is_terminal_divert {
                branch_nodes.push(Node::Newline);
            }
        }
    } else if choice.has_choice_only_content
        && !choice.has_start_content
        && matches!(choice.body.as_slice(), [Node::Divert(_)])
    {
        branch_nodes.push(Node::Text(" ".to_owned()));
        branch_nodes.extend(choice.body.clone());
        branch_nodes.push(Node::Newline);
        body_already_emitted = true;
    } else if choice.has_choice_only_content
        && !choice.has_start_content
        && branch_nodes.is_empty()
        && !choice.body.is_empty()
    {
        branch_nodes.push(Node::Text(" ".to_owned()));
        branch_nodes.extend(choice.body.clone());
        body_already_emitted = true;
    }
    if !body_already_emitted {
        branch_nodes.extend(choice.body.clone());
    }

    // Check if branch body contains nested choices (at any position)
    let has_nested_choices = branch_nodes.iter().any(|n| matches!(n, Node::Choice(_)));
    let mut branch_container = if has_nested_choices {
        // Pass continuation_path as fallback so nested choice blocks and their
        // gather continuations can inherit the outer continuation.
        emit_nodes_with_continuation(&branch_nodes, &branch_scope, context, continuation_path)?
    } else {
        emit_nodes(&branch_nodes, &branch_scope, context)?
    };
    if let Some(path) = continuation_path
        .filter(|_| !branch_has_terminal_content(&branch_nodes) && !has_nested_choices)
    {
        branch_container.push(json!({"->": path}));
    }
    out.insert_named(
        branch_name,
        branch_container.into_json_array(None, Some(5))?,
    );

    Ok(())
}

fn emit_expression(expression: &Expression, out: &mut Vec<Value>) {
    emit_expression_ctx(expression, out, None);
}

fn emit_expression_ctx(
    expression: &Expression,
    out: &mut Vec<Value>,
    context: Option<&EmitContext>,
) {
    match expression {
        Expression::Bool(value) => out.push(json!(value)),
        Expression::Int(value) => out.push(json!(value)),
        Expression::Float(value) => out.push(json!(value)),
        Expression::Str(value) => {
            // Parse the string content for {expr} interpolations
            let dynamic = parse_dynamic_string(value).unwrap_or_else(|_| DynamicString {
                parts: vec![DynamicStringPart::Text(value.clone())],
            });
            out.push(json!("str"));
            // If there are no expression parts (plain string or empty), emit as literal text
            let has_expressions = dynamic.parts.iter().any(|p| {
                matches!(
                    p,
                    DynamicStringPart::Expression(_) | DynamicStringPart::Sequence(_)
                )
            });
            if !has_expressions {
                // Plain string (possibly empty) — emit as single text token
                let text: String = dynamic
                    .parts
                    .iter()
                    .filter_map(|p| {
                        if let DynamicStringPart::Text(t) = p {
                            Some(t.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                out.push(json!(format!("^{text}")));
            } else {
                for part in &dynamic.parts {
                    match part {
                        DynamicStringPart::Text(t) => {
                            if !t.is_empty() {
                                out.push(json!(format!("^{t}")));
                            }
                        }
                        DynamicStringPart::Expression(expr) => {
                            out.push(json!("ev"));
                            emit_expression_ctx(expr, out, context);
                            out.push(json!("out"));
                        }
                        DynamicStringPart::Sequence(_) => {
                            // Sequences inside string literals not supported here; emit raw
                            out.push(json!(format!("^{value}")));
                        }
                    }
                }
            }
            out.push(json!("/str"));
        }
        Expression::Variable(name) => {
            if context.is_some_and(|ctx| ctx.top_flow_names.contains(name)) {
                out.push(json!({"CNT?": name}))
            } else {
                out.push(json!({"VAR?": name}))
            }
        }
        Expression::DivertTarget(target) => out.push(json!({"^->": target})),
        Expression::Negate(expr) => {
            emit_expression_ctx(expr, out, context);
            out.push(json!("_"));
        }
        Expression::Not(expr) => {
            emit_expression_ctx(expr, out, context);
            out.push(json!("!"));
        }
        Expression::FunctionCall { name, args } => {
            // Check if this is a list-typed call: list_name(n) or list_name()
            if context.is_some_and(|ctx| ctx.list_names.contains(name)) {
                if args.is_empty() {
                    // list() → empty list with origins
                    out.push(json!({"list": {}, "origins": [name]}));
                } else if args.len() == 1 {
                    // list(n) → "^list_name", n, "listInt"
                    out.push(json!(format!("^{name}")));
                    emit_expression_ctx(&args[0], out, context);
                    out.push(json!("listInt"));
                } else {
                    // Fallback: treat as user function call
                    for arg in args {
                        emit_expression_ctx(arg, out, context);
                    }
                    out.push(json!({"f()": name}));
                }
                return;
            }
            // Map built-in Ink function names to runtime tokens
            // Built-ins are emitted as plain strings; user functions as {"f()": name}
            let builtin_token: Option<&str> = match name.as_str() {
                "RANDOM" => Some("rnd"),
                "SEED_RANDOM" => Some("srnd"),
                "POW" => Some("POW"),
                "FLOOR" => Some("FLOOR"),
                "CEILING" => Some("CEILING"),
                "INT" => Some("INT"),
                "FLOAT" => Some("FLOAT"),
                "MIN" => Some("MIN"),
                "MAX" => Some("MAX"),
                "READ_COUNT" => Some("readc"),
                "TURNS_SINCE" => Some("turns"),
                "CHOICE_COUNT" => Some("choiceCnt"),
                "TURNS" => Some("turn"),
                "LIST_VALUE" => Some("LIST_VALUE"),
                "LIST_ALL" => Some("LIST_ALL"),
                "LIST_INVERT" => Some("LIST_INVERT"),
                "LIST_COUNT" => Some("LIST_COUNT"),
                "LIST_MIN" => Some("LIST_MIN"),
                "LIST_MAX" => Some("LIST_MAX"),
                "LIST_RANGE" => Some("range"),
                "LIST_RANDOM" => Some("lrnd"),
                _ => None,
            };
            for arg in args {
                emit_expression_ctx(arg, out, context);
            }
            if let Some(token) = builtin_token {
                out.push(json!(token));
            } else if context.is_some_and(|ctx| ctx.external_functions.contains(name)) {
                let ex_args = args.len() as i32;
                if ex_args > 0 {
                    out.push(json!({"x()": name, "exArgs": ex_args}));
                } else {
                    out.push(json!({"x()": name}));
                }
            } else if context.is_some_and(|ctx| ctx.global_variables.contains(name)) {
                // Variable holding a divert target — call it as a variable function
                out.push(json!({"f()": name, "var": true}));
            } else {
                out.push(json!({"f()": name}));
            }
        }
        Expression::ListItems(items) => {
            let mut list_map = serde_json::Map::new();
            for bare_name in items {
                if let Some((qname, val)) = context.and_then(|ctx| ctx.resolve_list_item(bare_name))
                {
                    list_map.insert(qname, json!(val));
                } else {
                    // Fallback: use bare name with value 0 (unknown list)
                    list_map.insert(bare_name.clone(), json!(0));
                }
            }
            out.push(json!({"list": list_map}));
        }
        Expression::EmptyList => {
            out.push(json!({"list": {}}));
        }
        Expression::Binary {
            left,
            operator,
            right,
        } => {
            emit_expression_ctx(left, out, context);
            emit_expression_ctx(right, out, context);
            out.push(json!(match operator {
                BinaryOperator::Add => "+",
                BinaryOperator::Subtract => "-",
                BinaryOperator::Multiply => "*",
                BinaryOperator::Divide => "/",
                BinaryOperator::Modulo => "%",
                BinaryOperator::Equal => "==",
                BinaryOperator::NotEqual => "!=",
                BinaryOperator::And => "&&",
                BinaryOperator::Or => "||",
                BinaryOperator::Greater => ">",
                BinaryOperator::GreaterEqual => ">=",
                BinaryOperator::Less => "<",
                BinaryOperator::LessEqual => "<=",
                BinaryOperator::Has => "?",
                BinaryOperator::Hasnt => "!?",
                BinaryOperator::Intersect => "L^",
            }));
        }
    }
}

fn emit_dynamic_string(
    dynamic: &DynamicString,
    out: &mut Vec<Value>,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    emit_dynamic_string_parts(&dynamic.parts, out, scope, context)
}

fn emit_dynamic_string_parts(
    parts: &[DynamicStringPart],
    out: &mut Vec<Value>,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    if parts.is_empty() {
        return Ok(());
    }

    match &parts[0] {
        DynamicStringPart::Text(text) => {
            if !text.is_empty() {
                out.push(json!(format!("^{text}")));
            }
            emit_dynamic_string_parts(&parts[1..], out, scope, context)
        }
        DynamicStringPart::Expression(expression) => {
            out.push(json!("ev"));
            emit_expression_ctx(expression, out, Some(context));
            out.push(json!("out"));
            out.push(json!("/ev"));
            emit_dynamic_string_parts(&parts[1..], out, scope, context)
        }
        DynamicStringPart::Sequence(sequence) => {
            out.push(emit_sequence(sequence, scope, context)?);
            emit_dynamic_string_parts(&parts[1..], out, scope, context)
        }
    }
}

fn emit_tag(
    tag: &DynamicString,
    out: &mut Vec<Value>,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    out.push(json!("#"));
    emit_dynamic_string(tag, out, scope, context)?;
    out.push(json!("/#"));
    Ok(())
}

fn emit_choice_text_segment(
    text: &str,
    tags: &[DynamicString],
    out: &mut Vec<Value>,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    if text.is_empty() && tags.is_empty() {
        return Ok(());
    }

    out.push(json!("str"));
    if !text.is_empty() {
        // Parse the text for inline {expr} and {&sequence} interpolations
        let dynamic = parse_dynamic_string(text).unwrap_or_else(|_| DynamicString {
            parts: vec![DynamicStringPart::Text(text.to_owned())],
        });
        let has_inline = dynamic
            .parts
            .iter()
            .any(|p| !matches!(p, DynamicStringPart::Text(_)));
        if has_inline {
            emit_dynamic_string_parts(&dynamic.parts, out, scope, context)?;
        } else {
            out.push(json!(format!("^{text}")));
        }
    }
    for tag in tags {
        emit_tag(tag, out, scope, context)?;
    }
    out.push(json!("/str"));
    Ok(())
}

fn emit_condition(
    condition: &Condition,
    out: &mut Vec<Value>,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    match condition {
        Condition::Bool(value) => out.push(json!(value)),
        Condition::FunctionCall(name) => {
            let mut call = BTreeMap::new();
            call.insert(format!("{name}()"), Value::String(name.clone()));
            out.push(serde_json::to_value(call).map_err(|error| {
                CompilerError::invalid_source(format!("failed to serialize function call: {error}"))
            })?);
        }
        Condition::Expression(Expression::Variable(name))
            if scope.resolve_choice_label(name).is_some() =>
        {
            // Labels are stored as absolute paths now
            out.push(json!({"CNT?": scope.resolve_choice_label(name).unwrap()}));
        }
        Condition::Expression(Expression::Variable(name))
            if context.top_flow_names.contains(name) || scope.child_flow_names.contains(name) =>
        {
            out.push(json!({"CNT?": scope.resolve_divert_target(name, context)}));
        }
        // Fully-qualified path like knot.stitch.label — treat as CNT? visit count
        Condition::Expression(Expression::Variable(name)) if name.contains('.') => {
            out.push(json!({"CNT?": name}));
        }
        Condition::Expression(expression) => emit_expression_ctx(expression, out, Some(context)),
    }

    Ok(())
}

fn branch_has_terminal_content(nodes: &[Node]) -> bool {
    nodes
        .iter()
        .rev()
        .find(|node| !matches!(node, Node::Newline))
        .is_some_and(node_is_terminal)
}

fn node_is_terminal(node: &Node) -> bool {
    match node {
        Node::Divert(_) | Node::TunnelReturn | Node::ReturnBool(_) | Node::ReturnExpr(_) => true,
        Node::Choice(choice) => branch_has_terminal_content(&choice.body),
        _ => false,
    }
}

fn recover_selected_text_inline_divert(selected_text: &str) -> Option<(String, String)> {
    let (text, target) = selected_text.rsplit_once("->")?;
    let target = target.trim();
    if target.is_empty() || target.contains(' ') {
        return None;
    }

    Some((text.trim_end().to_owned(), target.to_owned()))
}

fn choice_flags(choice: &Choice) -> i32 {
    let mut flags = 0;
    if !choice.conditions.is_empty() {
        flags |= 1;
    }
    if choice.has_start_content {
        flags |= 2;
    }
    if choice.has_choice_only_content {
        flags |= 4;
    }
    if choice.is_invisible_default {
        flags |= 8;
    }
    if choice.once_only {
        flags |= 16;
    }
    flags
}

fn emit_conditional(
    condition: &Condition,
    when_true: &[Node],
    when_false: Option<&[Node]>,
    scope: &EmitScope,
    conditional_index: usize,
    context: &EmitContext,
) -> Result<Vec<Value>, CompilerError> {
    let branches = flatten_conditional_branches(condition, when_true, when_false);

    // First pass: emit all condition token sequences to know their sizes,
    // so we can compute the correct absolute index of "nop".
    struct BranchEmit {
        cond_tokens: Option<Vec<Value>>,
        branch_content: EmittedContainer,
    }
    let mut branch_emits: Vec<BranchEmit> = Vec::new();
    for (branch_index, (branch_condition, branch_nodes)) in branches.iter().enumerate() {
        let cond_tokens = if let Some(cond) = branch_condition {
            let mut tokens = Vec::new();
            emit_condition(cond, &mut tokens, scope, context)?;
            Some(tokens)
        } else {
            None
        };
        let branch_scope = scope.choice_branch(&format!("cond-{branch_index}"));
        let branch_content = emit_nodes(branch_nodes, &branch_scope, context)?;
        branch_emits.push(BranchEmit {
            cond_tokens,
            branch_content,
        });
    }

    // Count total tokens emitted before "nop":
    // For each branch: (ev + cond_tokens + /ev) if has condition, then 1 array token.
    let tokens_before_nop: usize = branch_emits
        .iter()
        .map(|b| {
            let cond_overhead = if let Some(ref ct) = b.cond_tokens {
                ct.len() + 2
            } else {
                0
            };
            cond_overhead + 1 // +1 for the array token
        })
        .sum();
    let nop_index = conditional_index + tokens_before_nop;
    let rejoin_target = format!("{}.{}", scope.path, nop_index);

    // Second pass: build the output with the correct rejoin target.
    let mut out = Vec::new();
    for BranchEmit {
        cond_tokens,
        mut branch_content,
    } in branch_emits
    {
        let has_condition = cond_tokens.is_some();
        if let Some(tokens) = cond_tokens {
            out.push(json!("ev"));
            out.extend(tokens);
            out.push(json!("/ev"));
        }

        branch_content.push(json!({"->": rejoin_target}));
        let mut named = Map::new();
        named.insert("b".to_owned(), branch_content.into_json_array(None, None)?);

        let selector = if has_condition {
            json!({"->": ".^.b", "c": true})
        } else {
            json!({"->": ".^.b"})
        };
        out.push(Value::Array(vec![selector, Value::Object(named)]));
    }

    out.push(json!("nop"));

    Ok(out)
}

fn emit_switch_conditional(
    value: &Expression,
    branches: &[(Option<Expression>, Vec<Node>)],
    scope: &EmitScope,
    switch_index: usize,
    context: &EmitContext,
) -> Result<Vec<Value>, CompilerError> {
    if branches.is_empty() {
        return Ok(Vec::new());
    }

    // Build value expression tokens: ev, <value_tokens>, /ev
    let mut value_tokens = Vec::new();
    emit_expression_ctx(value, &mut value_tokens, Some(context));
    let preamble_len = value_tokens.len() + 2; // ev + value_tokens + /ev

    let num_branches = branches.len();
    // Layout: [preamble_len tokens] [N branch arrays] [nop]
    let nop_index = switch_index + preamble_len + num_branches;
    let exit_target = format!("{}.{}", scope.path, nop_index);

    // Emit all branch bodies first (they all reference exit_target)
    let mut branch_bodies: Vec<EmittedContainer> = Vec::new();
    for (_, body_nodes) in branches {
        let branch_scope = scope.choice_branch("b");
        let mut body = emit_nodes(body_nodes, &branch_scope, context)?;
        body.push(json!({"->": exit_target}));
        branch_bodies.push(body);
    }

    // Build output
    let mut out = Vec::new();
    // Preamble: put the switch value on the stack
    out.push(json!("ev"));
    out.extend(value_tokens);
    out.push(json!("/ev"));

    // Each branch
    for ((case_expr, _), body) in branches.iter().zip(branch_bodies) {
        let mut named = Map::new();
        let body_array = body.into_json_array(None, None)?;
        // Insert pop at the start of the body array
        let body_with_pop = if let Value::Array(mut arr) = body_array {
            arr.insert(0, json!("pop"));
            Value::Array(arr)
        } else {
            return Err(CompilerError::invalid_source(
                "switch branch body should be an array".to_owned(),
            ));
        };
        named.insert("b".to_owned(), body_with_pop);

        if let Some(case_expr) = case_expr {
            // Case branch: [du, ev, case_tokens, ==, /ev, {->:.^.b, c:true}, {b:[...]}]
            let mut case_tokens = Vec::new();
            emit_expression_ctx(case_expr, &mut case_tokens, Some(context));
            let mut branch_array = vec![json!("du"), json!("ev")];
            branch_array.extend(case_tokens);
            branch_array.push(json!("=="));
            branch_array.push(json!("/ev"));
            branch_array.push(json!({"->": ".^.b", "c": true}));
            branch_array.push(Value::Object(named));
            out.push(Value::Array(branch_array));
        } else {
            // Else branch: [{->:.^.b}, {b:[...]}]
            let branch_array = vec![json!({"->": ".^.b"}), Value::Object(named)];
            out.push(Value::Array(branch_array));
        }
    }

    out.push(json!("nop"));
    Ok(out)
}

fn flatten_conditional_branches<'a>(
    condition: &'a Condition,
    when_true: &'a [Node],
    when_false: Option<&'a [Node]>,
) -> Vec<(Option<&'a Condition>, &'a [Node])> {
    let mut branches = vec![(Some(condition), when_true)];
    let mut current_false = when_false;

    while let Some(nodes) = current_false {
        if let [
            Node::Conditional {
                condition,
                when_true,
                when_false,
            },
        ] = nodes
        {
            branches.push((Some(condition), when_true));
            current_false = when_false.as_deref();
        } else {
            branches.push((None, nodes));
            break;
        }
    }

    branches
}
