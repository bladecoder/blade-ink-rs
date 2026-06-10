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
    let branch_scope = scope
        .choice_branch(&branch_name)
        .with_relative_depth(scope.relative_depth + 1);
    // In root (scope.path == "0") inklecate emits absolute paths ("0.c-N"),
    // in knots/stitches it emits relative paths (".^.c-N").
    let choice_ptr = if scope.path == "0" {
        format!("0.{}", branch_name)
    } else {
        format!(".^.{}", branch_name)
    };
    out.push(json!({"*": choice_ptr, "flg": choice_flags(choice)}));

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
            // Skip the auto-newline for terminal diverts, and also for inline diverts that are
            // authored after inline selected text on the same source line (the selected text keeps
            // the trailing whitespace needed to join the diverted content).
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
        // choice-only with single divert body:
        // - inline divert (same line): "^ " then divert then "\n" (inklecate behavior)
        // - body divert (indented line): "\n" then divert
        if choice.body_divert_is_inline {
            branch_nodes.push(Node::Text(" ".to_owned()));
            branch_nodes.extend(choice.body.clone());
            branch_nodes.push(Node::Newline);
        } else {
            branch_nodes.push(Node::Newline);
            branch_nodes.extend(choice.body.clone());
        }
        body_already_emitted = true;
    } else if choice.has_choice_only_content
        && !choice.has_start_content
        && branch_nodes.is_empty()
        && !choice.body.is_empty()
    {
        // choice-only with multi-node body: "\n" then body
        branch_nodes.push(Node::Newline);
        branch_nodes.extend(choice.body.clone());
        body_already_emitted = true;
    } else if choice.has_choice_only_content && !choice.has_start_content && choice.body.is_empty()
    {
        // choice-only with completely empty body: inklecate always opens the c-N
        // container with a "\n" (representing the line break after the user selects).
        branch_nodes.push(Node::Newline);
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
    if let Some(token) = loose_end_append_for_nodes(
        &branch_nodes,
        has_nested_choices,
        continuation_path,
        None,
        false,
        LooseEndNoFallback::None,
    ) {
        branch_container.push(token);
    }
    out.insert_named(
        branch_name,
        branch_container.into_json_array(
            None,
            Some(
                if choice
                    .label
                    .as_deref()
                    .is_some_and(|l| context.turns_since_targets.contains(l))
                {
                    7 // VISITS | TURNS | COUNT_START_ONLY
                } else {
                    5 // VISITS | COUNT_START_ONLY
                },
            ),
        )?,
    );

    Ok(())
}

