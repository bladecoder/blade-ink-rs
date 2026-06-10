fn emit_nodes(
    nodes: &[Node],
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<EmittedContainer, CompilerError> {
    emit_nodes_with_continuation(nodes, scope, context, None)
}

/// Recursively replace `old_path` and its descendants with `new_path` in
/// path-bearing runtime fields within a JSON value tree.
fn fix_divert_paths(value: &mut Value, old_path: &str, new_path: &str) {
    match value {
        Value::Object(map) => {
            for field in ["->", "x->", "*", "^->", "CNT?"] {
                if let Some(v) = map.get_mut(field)
                    && let Some(path) = v.as_str()
                {
                    if path == old_path {
                        *v = Value::String(new_path.to_owned());
                    } else if let Some(suffix) = path.strip_prefix(old_path)
                        && suffix.starts_with('.')
                    {
                        *v = Value::String(format!("{new_path}{suffix}"));
                    }
                }
            }
            for v in map.values_mut() {
                fix_divert_paths(v, old_path, new_path);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                fix_divert_paths(v, old_path, new_path);
            }
        }
        _ => {}
    }
}

fn threaded_loop_label_for_choice_block(continuation: &[Node]) -> Option<(String, bool)> {
    let mut nodes = continuation;
    while !nodes.is_empty() && matches!(nodes[0], Node::Newline) {
        nodes = &nodes[1..];
    }

    if let Some(Node::GatherLabel { label, .. }) = nodes.first()
        && label == "loop"
    {
        return Some((label.clone(), true));
    }

    None
}

fn should_use_wrapped_choice_for_label(loop_label: &str, scope: &EmitScope) -> bool {
    loop_label == "loop" && !scope.path.contains('.')
}

fn split_nodes_at_first_choice(nodes: &[Node]) -> (&[Node], &[Node], &[Node]) {
    let mut idx = 0usize;
    while idx < nodes.len() && !matches!(nodes[idx], Node::Choice(_)) {
        idx += 1;
    }
    if idx >= nodes.len() {
        return (nodes, &[], &[]);
    }

    let level = match &nodes[idx] {
        Node::Choice(c) => c.nesting_level,
        _ => unreachable!(),
    };
    let mut end = idx;
    while end < nodes.len() {
        match &nodes[end] {
            Node::Choice(c) if c.nesting_level == level => end += 1,
            _ => break,
        }
    }

    (&nodes[..idx], &nodes[idx..end], &nodes[end..])
}

enum ChoiceEmissionMode {
    Flat,
    ThreadedLoopLabel { loop_label: String },
    ThreadedAnonGather,
}

struct WeaveChoiceSection<'a> {
    prefix_nodes: &'a [Node],
    choices: &'a [Node],
    continuation_nodes: &'a [Node],
    mode: ChoiceEmissionMode,
}

fn skip_leading_newlines(nodes: &[Node]) -> &[Node] {
    let mut index = 0;
    while index < nodes.len() && matches!(nodes[index], Node::Newline) {
        index += 1;
    }
    &nodes[index..]
}

fn nodes_contain_choice(nodes: &[Node]) -> bool {
    for node in nodes {
        match node {
            Node::Choice(_) => return true,
            Node::Conditional {
                when_true,
                when_false,
                ..
            } if nodes_contain_choice(when_true)
                || when_false.as_deref().is_some_and(nodes_contain_choice) =>
            {
                return true;
            }
            Node::SwitchConditional { branches, .. }
                if branches
                    .iter()
                    .any(|(_, branch_nodes)| nodes_contain_choice(branch_nodes)) =>
            {
                return true;
            }
            _ => {}
        }
    }

    false
}

fn choice_block_contains_nested_choices(choices: &[Node]) -> bool {
    choices.iter().any(|node| {
        if let Node::Choice(choice) = node {
            nodes_contain_choice(&choice.body)
        } else {
            false
        }
    })
}

fn choice_is_invisible_default(choice: &Choice) -> bool {
    choice.start_text.trim().is_empty() && choice.choice_only_text.trim().is_empty()
}

fn choice_block_has_invisible_default(choices: &[Node]) -> bool {
    choices.iter().any(|node| {
        if let Node::Choice(choice) = node {
            choice_is_invisible_default(choice)
        } else {
            false
        }
    })
}

fn should_use_threaded_anon_gather(
    choices: &[Node],
    continuation: &[Node],
    scope: &EmitScope,
) -> bool {
    let continuation = skip_leading_newlines(continuation);

    if !matches!(continuation.first(), Some(Node::GatherPoint)) {
        return false;
    }

    if scope.path.contains(".c-") {
        return false;
    }

    if !scope.path.contains('.')
        || choice_block_contains_nested_choices(choices)
        || choice_block_has_invisible_default(choices)
    {
        return false;
    }

    true
}

fn analyze_weave_choice_section<'a>(
    choices: &'a [Node],
    continuation: &'a [Node],
    scope: &EmitScope,
) -> WeaveChoiceSection<'a> {
    if let Some((loop_label, strip_label)) = threaded_loop_label_for_choice_block(continuation)
        && should_use_wrapped_choice_for_label(&loop_label, scope)
    {
        let mut continuation_tail = skip_leading_newlines(continuation);
        if strip_label
            && matches!(
                continuation_tail.first(),
                Some(Node::GatherLabel { .. })
            )
        {
            continuation_tail = &continuation_tail[1..];
        }

        let (prefix, loop_choices, continuation_after_choices) =
            split_nodes_at_first_choice(continuation_tail);
        if !loop_choices.is_empty() {
            return WeaveChoiceSection {
                prefix_nodes: prefix,
                choices: loop_choices,
                continuation_nodes: continuation_after_choices,
                mode: ChoiceEmissionMode::ThreadedLoopLabel { loop_label },
            };
        }

        return WeaveChoiceSection {
            prefix_nodes: &[],
            choices,
            continuation_nodes: continuation_tail,
            mode: ChoiceEmissionMode::ThreadedLoopLabel { loop_label },
        };
    }

    if should_use_threaded_anon_gather(choices, continuation, scope) {
        return WeaveChoiceSection {
            prefix_nodes: &[],
            choices,
            continuation_nodes: continuation,
            mode: ChoiceEmissionMode::ThreadedAnonGather,
        };
    }

    WeaveChoiceSection {
        prefix_nodes: &[],
        choices,
        continuation_nodes: continuation,
        mode: ChoiceEmissionMode::Flat,
    }
}

/// Replace any divert to `self_path` immediately before the terminator of a
/// JSON array with `"done"`, to avoid self-referential loops after hoisting.
fn replace_self_divert_with_done(value: &mut Value, self_path: &str) {
    let Value::Array(arr) = value else { return };
    // The array ends with either null or a terminator object (last element).
    // Check the element just before the terminator.
    let n = arr.len();
    if n < 2 {
        return;
    }
    let candidate = n - 2; // element just before terminator
    if let Value::Object(map) = &arr[candidate]
        && map.get("->").and_then(Value::as_str) == Some(self_path)
    {
        arr[candidate] = json!("done");
    }
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
                emit_expression_ctx(expression, &mut out.content, Some(context), Some(scope));
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
                let resolved_target = scope.resolve_divert_target(target, context);
                let is_var = scope.is_variable_divert(target, context);
                if !args.is_empty() {
                    out.push(json!("ev"));
                    for arg in args {
                        emit_expression_ctx(arg, &mut out.content, Some(context), Some(scope));
                    }
                    out.push(json!("/ev"));
                }
                if is_var {
                    out.push(json!({"->t->": target, "var": true}));
                } else {
                    out.push(json!({"->t->": resolved_target}));
                }
            }
            Node::TunnelReturn => {
                out.push(json!("ev"));
                out.push(json!("void"));
                out.push(json!("/ev"));
                out.push(json!("->->"));
            }
            Node::TunnelOnwardsWithTarget { target, args } => {
                // ->-> target(args): push args + divert target, then ->->
                out.push(json!("ev"));
                for arg in args {
                    emit_expression_ctx(arg, &mut out.content, Some(context), Some(scope));
                }
                let resolved = scope.resolve_divert_target(target, context);
                out.push(json!({"^->": resolved}));
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
                            emit_expression_ctx(arg, &mut out.content, Some(context), Some(scope));
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
                emit_expression_ctx(expression, &mut out.content, Some(context), Some(scope));
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
            } => emit_assignment(
                variable_name,
                expression,
                mode,
                &mut out,
                context,
                Some(scope),
            ),
            Node::VoidCall { name, args } => {
                out.push(json!("ev"));
                for (index, arg) in args.iter().enumerate() {
                    emit_call_argument(
                        name,
                        index,
                        arg,
                        &mut out.content,
                        Some(context),
                        Some(scope),
                    );
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
                out.push(json!("\n"));
            }
            Node::Choice(first_choice) => {
                let block_start = index;
                let level = first_choice.nesting_level;
                while index < nodes.len() {
                    match &nodes[index] {
                        Node::Choice(c) if c.nesting_level == level => index += 1,
                        _ => break,
                    }
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
            Node::GatherPoint => {
                // Anonymous gather with no content — acts only as a separator between
                // choice blocks at different nesting levels.  Emits nothing itself.
            }
            Node::GatherLabel { .. } => {
                let Node::GatherLabel { label, indent, .. } = &nodes[index] else {
                    unreachable!();
                };
                if *indent > 0 {
                    let remaining = &nodes[index + 1..];
                    let sub_scope = scope.choice_branch(label);
                    let mut sub_container = emit_nodes_with_continuation(
                        remaining,
                        &sub_scope,
                        context,
                        fallback_continuation,
                    )?;
                    let g_keys: Vec<String> = sub_container
                        .named
                        .keys()
                        .filter(|key| key.starts_with("g-"))
                        .cloned()
                        .collect();
                    for key in &g_keys {
                        let old_path = format!("{}.{}", sub_scope.path, key);
                        let new_path = format!("{}.{}", scope.path, key);
                        for value in sub_container.content.iter_mut() {
                            fix_divert_paths(value, &old_path, &new_path);
                        }
                        for value in sub_container.named.values_mut() {
                            fix_divert_paths(value, &old_path, &new_path);
                        }
                    }
                    for key in g_keys {
                        let mut value = sub_container.named.remove(&key).unwrap();
                        let hoisted_path = format!("{}.{}", scope.path, key);
                        replace_self_divert_with_done(&mut value, &hoisted_path);
                        out.insert_named(key, value);
                    }
                    let count_flags = context.count_all_visits.then_some(5);
                    out.push(sub_container.into_json_array(Some(label), count_flags)?);
                    break;
                }

                let mut gather_index = index;
                let mut is_first = true;

                loop {
                    let Node::GatherLabel {
                        label: gather_label,
                        level: gather_level,
                        indent: gather_indent,
                    } = &nodes[gather_index]
                    else {
                        unreachable!();
                    };
                    let body_start = gather_index + 1;
                    let body_end = nodes[body_start..]
                        .iter()
                        .position(|node| {
                            matches!(
                                node,
                                Node::GatherLabel {
                                    level,
                                    indent,
                                    ..
                                } if indent == gather_indent && level <= gather_level
                            )
                        })
                        .map_or(nodes.len(), |offset| body_start + offset);
                    let next_gather_path = nodes.get(body_end).and_then(|node| {
                        let Node::GatherLabel {
                            label: next_label,
                            level: next_level,
                            indent: next_indent,
                        } = node
                        else {
                            return None;
                        };
                        (next_level == gather_level && next_indent == gather_indent)
                            .then(|| format!("{}.{}", scope.path, next_label))
                    });
                    let gather_fallback = next_gather_path
                        .as_deref()
                        .or(fallback_continuation);
                    let sub_scope = scope.choice_branch(gather_label);
                    let gather_body = &nodes[body_start..body_end];
                    let mut sub_container = emit_nodes_with_continuation(
                        gather_body,
                        &sub_scope,
                        context,
                        gather_fallback,
                    )?;
                    if let Some(token) = loose_end_append_for_nodes(
                        gather_body,
                        nodes_contain_choice(gather_body),
                        gather_fallback,
                        None,
                        false,
                        LooseEndNoFallback::None,
                    ) {
                        sub_container.push(token);
                    }

                    let g_keys: Vec<String> = sub_container
                        .named
                        .keys()
                        .filter(|key| key.starts_with("g-"))
                        .cloned()
                        .collect();
                    for key in &g_keys {
                        let old_path = format!("{}.{}", sub_scope.path, key);
                        let new_path = format!("{}.{}", scope.path, key);
                        for value in sub_container.content.iter_mut() {
                            fix_divert_paths(value, &old_path, &new_path);
                        }
                        for value in sub_container.named.values_mut() {
                            fix_divert_paths(value, &old_path, &new_path);
                        }
                    }
                    for key in g_keys {
                        let mut value = sub_container.named.remove(&key).unwrap();
                        let hoisted_path = format!("{}.{}", scope.path, key);
                        replace_self_divert_with_done(&mut value, &hoisted_path);
                        out.insert_named(key, value);
                    }

                    let count_flags = context.count_all_visits.then_some(5);
                    if is_first {
                        out.push(
                            sub_container
                                .into_json_array(Some(gather_label), count_flags)?,
                        );
                    } else {
                        out.insert_named(
                            gather_label.clone(),
                            sub_container.into_json_array(None, count_flags)?,
                        );
                    }

                    if body_end == nodes.len() {
                        break;
                    }
                    if !matches!(
                        &nodes[body_end],
                        Node::GatherLabel {
                            level,
                            indent,
                            ..
                        } if level == gather_level && indent == gather_indent
                    ) {
                        break;
                    }
                    gather_index = body_end;
                    is_first = false;
                }

                break;
            }
        }

        index += 1;
    }

    Ok(out)
}
