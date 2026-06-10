fn emit_choice_block(
    out: &mut EmittedContainer,
    choices: &[Node],
    continuation: &[Node],
    scope: &EmitScope,
    next_choice_index: &mut usize,
    context: &EmitContext,
    fallback_continuation: Option<&str>,
) -> Result<(), CompilerError> {
    let section = analyze_weave_choice_section(choices, continuation, scope);

    if !section.prefix_nodes.is_empty() {
        for value in emit_nodes(section.prefix_nodes, scope, context)?.content {
            out.push(value);
        }
    }

    match &section.mode {
        ChoiceEmissionMode::ThreadedAnonGather => {
            let threaded = build_threaded_choice_block_no_label(
                section.choices,
                section.continuation_nodes,
                scope,
                out.content.len(),
                next_choice_index,
                context,
                fallback_continuation,
            )?;
            pack_threaded_choice_output(out, threaded)?;
            return Ok(());
        }
        ChoiceEmissionMode::ThreadedLoopLabel { loop_label } => {
            let threaded = build_wrapped_loop_choice_block(
                section.choices,
                section.continuation_nodes,
                scope,
                next_choice_index,
                context,
                WrappedLoopChoiceBlockConfig {
                    loop_label,
                    group_index: out.content.len(),
                    fallback_continuation,
                },
            )?;
            pack_threaded_choice_output(out, threaded)?;
            return Ok(());
        }
        ChoiceEmissionMode::Flat => {}
    }

    let mut choice_labels = BTreeMap::new();
    for (offset, choice) in section.choices.iter().enumerate() {
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
    let gather_label: Option<String> =
        if let Some(Node::GatherLabel { label: lbl, .. }) = section.continuation_nodes.first() {
            let path = format!("{}.{}", scope.path, lbl);
            choice_labels.insert(lbl.clone(), path);
            Some(lbl.clone())
        } else {
            None
        };

    let block_scope = scope.with_choice_labels(choice_labels);

    // Compute gather container and continuation path, but defer insert_named so that
    // choice containers (c-N) are inserted into out.named first (matching inklecate order).
    let (continuation_path, deferred_gather): (Option<String>, Option<(String, Value)>) =
        if section.continuation_nodes.is_empty() {
            // No explicit continuation nodes — use fallback if available.
            // If the fallback path points to the g-N for this scope, we'll create the minimal
            // gather container after emitting choices.
            let name = format!("g-{}", *next_choice_index);
            let expected_path = format!("{}.{}", scope.path, name);
            let deferred = if fallback_continuation.is_some_and(|fb| fb == expected_path) {
                let gather_value = Value::Array(vec![json!("done"), Value::Null]);
                Some((name, gather_value))
            } else {
                None
            };
            (fallback_continuation.map(|s| s.to_owned()), deferred)
        } else {
            let generated_name = format!("g-{}", *next_choice_index);
            let name = gather_label.as_deref().unwrap_or(&generated_name);
            let continuation_scope = block_scope.continuation(name);
            // The gather label is represented by the named-content key.
            let continuation_body = if gather_label.is_some() {
                &section.continuation_nodes[1..]
            } else {
                section.continuation_nodes
            };
            let has_nested_choices_in_continuation = continuation_body
                .iter()
                .any(|n| matches!(n, Node::Choice(_)));
            let g_n_path = format!("{}.{}", scope.path, name);
            let fallback_is_self = fallback_continuation == Some(g_n_path.as_str());
            let inner_fallback = if fallback_is_self {
                None
            } else {
                fallback_continuation
            };
            let mut gather_container = emit_nodes_with_continuation(
                continuation_body,
                &continuation_scope,
                context,
                inner_fallback,
            )?;
            if let Some(token) = loose_end_append_for_nodes(
                continuation_body,
                has_nested_choices_in_continuation,
                fallback_continuation,
                Some(g_n_path.as_str()),
                true,
                LooseEndNoFallback::Done,
            ) {
                gather_container.push(token);
            }
            let count_flags = if gather_label.is_some() && context.count_all_visits {
                Some(5)
            } else {
                None
            };
            let continuation_value = gather_container.into_json_array(None, count_flags)?;
            let path = format!("{}.{}", scope.path, name);
            (Some(path), Some((name.to_owned(), continuation_value)))
        };

    for choice in section.choices {
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

    // Insert the gather container AFTER the choice containers so it appears last in the
    // named map, matching inklecate's insertion order (c-0, c-1, ..., g-0).
    if let Some((name, value)) = deferred_gather {
        out.insert_named(name, value);
    }

    Ok(())
}

enum ThreadedContinuationPlacement {
    InsideGroup,
    OutsideGroup,
}

struct ThreadedChoiceOutput {
    group: EmittedContainer,
    group_name: Option<String>,
    continuation: Option<(String, Value)>,
    continuation_placement: ThreadedContinuationPlacement,
}

enum LooseEndNoFallback<'a> {
    None,
    Done,
    Token(&'a str),
}

fn loose_end_append_for_nodes<'a>(
    nodes: &[Node],
    has_nested_choices: bool,
    fallback_path: Option<&'a str>,
    self_path: Option<&str>,
    suppress_on_explicit_transfer: bool,
    no_fallback: LooseEndNoFallback<'a>,
) -> Option<Value> {
    if has_nested_choices || branch_has_terminal_content(nodes) {
        return None;
    }

    if suppress_on_explicit_transfer && branch_has_explicit_flow_transfer(nodes) {
        return None;
    }

    if let Some(path) = fallback_path
        && self_path != Some(path)
    {
        return Some(json!({"->": path}));
    }

    match no_fallback {
        LooseEndNoFallback::None => None,
        LooseEndNoFallback::Done => Some(json!("done")),
        LooseEndNoFallback::Token(token) => Some(json!(token)),
    }
}

fn pack_threaded_choice_output(
    out: &mut EmittedContainer,
    mut threaded: ThreadedChoiceOutput,
) -> Result<(), CompilerError> {
    if let Some((name, value)) = threaded
        .continuation
        .as_ref()
        .filter(|_| {
            matches!(
                threaded.continuation_placement,
                ThreadedContinuationPlacement::InsideGroup
            )
        })
        .cloned()
    {
        threaded.group.insert_named(name, value);
    }

    let group_value = threaded
        .group
        .into_json_array(threaded.group_name.as_deref(), None)?;

    if let Some((name, value)) = threaded.continuation.filter(|_| {
        matches!(
            threaded.continuation_placement,
            ThreadedContinuationPlacement::OutsideGroup
        )
    }) {
        let mut outer = EmittedContainer::default();
        outer.push(group_value);
        outer.insert_named(name, value);
        out.push(outer.into_json_array(None, None)?);
    } else {
        out.push(group_value);
    }

    Ok(())
}

fn build_threaded_choice_block_no_label(
    choices: &[Node],
    continuation: &[Node],
    scope: &EmitScope,
    group_index: usize,
    next_choice_index: &mut usize,
    context: &EmitContext,
    fallback_continuation: Option<&str>,
) -> Result<ThreadedChoiceOutput, CompilerError> {
    let loop_choices = choices
        .iter()
        .filter_map(|node| match node {
            Node::Choice(choice) => Some(choice),
            _ => None,
        })
        .collect::<Vec<_>>();

    let fallback_choice = loop_choices
        .iter()
        .find(|choice| {
            choice.start_text.trim().is_empty() && choice.choice_only_text.trim().is_empty()
        })
        .copied();

    let emit_choices = if fallback_choice.is_some() {
        loop_choices
            .iter()
            .copied()
            .filter(|choice| {
                !(choice.start_text.trim().is_empty() && choice.choice_only_text.trim().is_empty())
            })
            .collect::<Vec<_>>()
    } else {
        loop_choices
    };

    let mut choice_labels = BTreeMap::new();
    let group_path = joined_path(&scope.path, group_index);
    for (offset, choice) in emit_choices.iter().enumerate() {
        if let Some(label) = &choice.label {
            let label_target =
                joined_path(&group_path, format!("c-{}", *next_choice_index + offset));
            choice_labels.insert(label.clone(), label_target);
        }
    }
    let block_scope = scope
        .at_path(group_path.clone())
        .with_choice_labels(choice_labels);
    let choices_prefix = group_path;

    let mut choices_group = EmittedContainer::default();
    let mut local_choice_index = *next_choice_index;

    let g_name = format!("g-{}", *next_choice_index);
    let continuation_path_abs = joined_path(&block_scope.path, &g_name);

    let continuation_scope = block_scope.continuation(&g_name);
    let continuation_body = match continuation.first() {
        Some(Node::GatherPoint) => {
            let mut idx = 1;
            while idx < continuation.len() && matches!(continuation[idx], Node::Newline) {
                idx += 1;
            }
            &continuation[idx..]
        }
        _ => continuation,
    };
    let continuation_has_nested_choices = continuation_body
        .iter()
        .any(|node| matches!(node, Node::Choice(_)));
    let fallback_is_self = fallback_continuation == Some(continuation_path_abs.as_str());
    let inner_fallback = if fallback_is_self {
        None
    } else {
        fallback_continuation
    };
    let mut continuation_container = emit_nodes_with_continuation(
        continuation_body,
        &continuation_scope,
        context,
        inner_fallback,
    )?;
    if let Some(token) = loose_end_append_for_nodes(
        continuation_body,
        continuation_has_nested_choices,
        fallback_continuation,
        Some(continuation_path_abs.as_str()),
        true,
        LooseEndNoFallback::Done,
    ) {
        continuation_container.push(token);
    }
    let continuation_value = continuation_container.into_json_array(None, None)?;

    for choice in emit_choices {
        let header_idx = choices_group.content.len();
        let header_scope =
            block_scope.at_path(joined_path(&block_scope.path, header_idx));
        choices_group.push(emit_wrapped_loop_choice_header(
            choice,
            &header_scope,
            local_choice_index,
            header_idx,
            &choices_prefix,
            context,
        )?);

        let branch_name = format!("c-{local_choice_index}");
        let branch_scope = block_scope.choice_branch(&branch_name);
        choices_group.insert_named(
            branch_name,
            emit_wrapped_loop_choice_body(
                choice,
                &branch_scope,
                WrappedLoopChoiceBodyConfig {
                    choice_index: local_choice_index,
                    header_idx,
                    choices_prefix: &choices_prefix,
                    continuation_path: Some(&continuation_path_abs),
                    continuation_terminal: None,
                },
                context,
            )?,
        );

        local_choice_index += 1;
        *next_choice_index += 1;
    }

    if let Some(fallback_choice) = fallback_choice {
        let branch_name = format!("c-{local_choice_index}");
        let branch_scope = block_scope.choice_branch(&branch_name);
        choices_group.push(json!({"*": branch_scope.path, "flg": 8}));
        choices_group.insert_named(
            branch_name,
            emit_wrapped_loop_choice_body(
                fallback_choice,
                &branch_scope,
                WrappedLoopChoiceBodyConfig {
                    choice_index: local_choice_index,
                    header_idx: 0,
                    choices_prefix: &choices_prefix,
                    continuation_path: Some(&continuation_path_abs),
                    continuation_terminal: None,
                },
                context,
            )?,
        );
        *next_choice_index += 1;
    }

    Ok(ThreadedChoiceOutput {
        group: choices_group,
        group_name: None,
        continuation: Some((g_name, continuation_value)),
        continuation_placement: ThreadedContinuationPlacement::InsideGroup,
    })
}
