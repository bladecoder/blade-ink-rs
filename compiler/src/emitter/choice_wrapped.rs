struct WrappedLoopChoiceBlockConfig<'a> {
    loop_label: &'a str,
    group_index: usize,
    fallback_continuation: Option<&'a str>,
}

fn build_wrapped_loop_choice_block(
    choices: &[Node],
    continuation: &[Node],
    scope: &EmitScope,
    next_choice_index: &mut usize,
    context: &EmitContext,
    config: WrappedLoopChoiceBlockConfig<'_>,
) -> Result<ThreadedChoiceOutput, CompilerError> {
    let WrappedLoopChoiceBlockConfig {
        loop_label,
        group_index,
        fallback_continuation,
    } = config;

    let outer_path = joined_path(&scope.path, group_index);
    let group_path = joined_path(&outer_path, loop_label);
    let mut choice_labels = BTreeMap::new();
    for (offset, node) in choices.iter().enumerate() {
        let Node::Choice(choice) = node else {
            continue;
        };
        if let Some(label) = &choice.label {
            let label_target =
                joined_path(&group_path, format!("c-{}", *next_choice_index + offset));
            choice_labels.insert(label.clone(), label_target);
        }
    }
    choice_labels.insert(loop_label.to_owned(), group_path.clone());
    let block_scope = scope
        .at_path(group_path.clone())
        .with_choice_labels(choice_labels);
    let choices_prefix = group_path;

    let mut choices_group = EmittedContainer::default();
    let mut local_choice_index = *next_choice_index;

    // Build continuation g-N.
    let g_name = format!("g-{}", *next_choice_index);
    let continuation_path_abs = joined_path(&outer_path, &g_name);
    let continuation_scope = scope.at_path(continuation_path_abs.clone());
    let continuation_body = match continuation.first() {
        Some(Node::GatherPoint) => &continuation[1..],
        _ => continuation,
    };
    let continuation_has_nested_choices = continuation_body
        .iter()
        .any(|node| matches!(node, Node::Choice(_)));
    let simple_terminal_fallback = wrapped_loop_simple_terminal_fallback(continuation_body);
    let fallback_is_self = fallback_continuation == Some(continuation_path_abs.as_str());
    let inner_fallback = if fallback_is_self {
        None
    } else {
        fallback_continuation
    };

    let continuation_value = if simple_terminal_fallback.is_none() {
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
        Some(continuation_container.into_json_array(None, None)?)
    } else {
        None
    };

    for node in choices {
        let Node::Choice(choice) = node else {
            continue;
        };

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
                    continuation_path: if simple_terminal_fallback.is_some() {
                        None
                    } else {
                        Some(&continuation_path_abs)
                    },
                    continuation_terminal: simple_terminal_fallback,
                },
                context,
            )?,
        );

        local_choice_index += 1;
        *next_choice_index += 1;
    }

    Ok(ThreadedChoiceOutput {
        group: choices_group,
        group_name: Some(loop_label.to_owned()),
        continuation: continuation_value.map(|value| (g_name, value)),
        continuation_placement: ThreadedContinuationPlacement::OutsideGroup,
    })
}

fn emit_wrapped_loop_choice_header(
    choice: &Choice,
    scope: &EmitScope,
    choice_index: usize,
    header_idx: usize,
    choices_prefix: &str,
    context: &EmitContext,
) -> Result<Value, CompilerError> {
    let mut arr = vec![
        json!("ev"),
        json!({"^->": format!("{choices_prefix}.{header_idx}.$r1")}),
        json!({"temp=": "$r"}),
        json!("str"),
        json!({"->": joined_path(&scope.path, "s")}),
        Value::Array(vec![json!({"#n": "$r1"})]),
        json!("/str"),
    ];

    for (index, condition) in choice.conditions.iter().enumerate() {
        emit_condition(condition, &mut arr, scope, context)?;
        if index > 0 {
            arr.push(json!("&&"));
        }
    }

    arr.push(json!("/ev"));
    arr.push(json!({
        "*": joined_path(choices_prefix, format!("c-{choice_index}")),
        "flg": choice_flags(choice)
    }));

    let mut s = Vec::new();
    emit_choice_text_content(
        &choice.start_text,
        &choice.start_tags,
        &mut s,
        scope,
        context,
    )?;
    emit_choice_text_content(
        &choice.choice_only_text,
        &choice.choice_only_tags,
        &mut s,
        scope,
        context,
    )?;
    s.push(json!({"->": "$r", "var": true}));
    s.push(Value::Null);
    arr.push(json!({"s": s}));

    Ok(Value::Array(arr))
}

struct WrappedLoopChoiceBodyConfig<'a> {
    choice_index: usize,
    header_idx: usize,
    choices_prefix: &'a str,
    continuation_path: Option<&'a str>,
    continuation_terminal: Option<&'a str>,
}

fn emit_wrapped_loop_choice_body(
    choice: &Choice,
    branch_scope: &EmitScope,
    config: WrappedLoopChoiceBodyConfig<'_>,
    context: &EmitContext,
) -> Result<Value, CompilerError> {
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
        } else if !choice.has_start_content {
            branch_nodes.extend(tokenize_inline_content(selected_text)?);
        }
        branch_nodes.extend(choice.selected_tags.iter().cloned().map(Node::Tag));
        if !body_already_emitted && !choice.has_start_content {
            let body_is_terminal_divert = matches!(
                choice.body.as_slice(),
                [Node::Divert(d)] if d.target == "END" || d.target == "DONE"
            );
            let body_is_inline_divert = matches!(choice.body.as_slice(), [Node::Divert(_)])
                && selected_text.ends_with(char::is_whitespace);
            if !body_is_terminal_divert && !body_is_inline_divert {
                branch_nodes.push(Node::Newline);
            }
        }
    } else if choice.has_choice_only_content
        && !choice.has_start_content
        && matches!(choice.body.as_slice(), [Node::Divert(_)])
    {
        if choice.body_divert_is_inline {
            branch_nodes.push(Node::Text(" ".to_owned()));
            branch_nodes.extend(choice.body.clone());
            branch_nodes.push(Node::Newline);
        } else {
            branch_nodes.push(Node::Newline);
            branch_nodes.extend(choice.body.clone());
        }
        body_already_emitted = true;
    } else if choice.has_choice_only_content && !choice.has_start_content {
        branch_nodes.push(Node::Newline);
    }

    if !body_already_emitted {
        branch_nodes.extend(choice.body.clone());
    }

    let has_nested_choices = branch_nodes.iter().any(|n| matches!(n, Node::Choice(_)));
    let mut branch_container = if has_nested_choices {
        emit_nodes_with_continuation(
            &branch_nodes,
            branch_scope,
            context,
            config.continuation_path,
        )?
    } else {
        emit_nodes(&branch_nodes, branch_scope, context)?
    };
    let no_fallback = if let Some(token) = config.continuation_terminal {
        LooseEndNoFallback::Token(token)
    } else {
        LooseEndNoFallback::None
    };
    if let Some(token) = loose_end_append_for_nodes(
        &branch_nodes,
        has_nested_choices,
        config.continuation_path,
        None,
        false,
        no_fallback,
    ) {
        branch_container.push(token);
    }

    let branch_count_flags = if choice.once_only { Some(5) } else { None };
    if !choice.has_start_content {
        return branch_container.into_json_array(None, branch_count_flags);
    }

    let mut arr = match branch_container.into_json_array(None, branch_count_flags)? {
        Value::Array(arr) => arr,
        _ => unreachable!(),
    };
    let last = arr.pop().unwrap();
    let mut out = vec![
        json!("ev"),
        json!({"^->": format!("{}.c-{}.$r2", config.choices_prefix, config.choice_index)}),
        json!("/ev"),
        json!({"temp=": "$r"}),
        json!({"->": format!("{}.{}.s", config.choices_prefix, config.header_idx)}),
        Value::Array(vec![json!({"#n": "$r2"})]),
        json!("\n"),
    ];
    out.append(&mut arr);
    out.push(last);
    Ok(Value::Array(out))
}

fn wrapped_loop_simple_terminal_fallback(nodes: &[Node]) -> Option<&'static str> {
    let mut iter = nodes
        .iter()
        .filter(|n| !matches!(n, Node::Newline | Node::GatherPoint));
    let first = iter.next()?;
    if iter.next().is_some() {
        return None;
    }

    match first {
        Node::Divert(Divert { target, arguments }) if arguments.is_empty() && target == "END" => {
            Some("end")
        }
        Node::Divert(Divert { target, arguments }) if arguments.is_empty() && target == "DONE" => {
            Some("done")
        }
        _ => None,
    }
}
