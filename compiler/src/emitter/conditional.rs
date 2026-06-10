fn emit_condition(
    condition: &Condition,
    out: &mut Vec<Value>,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    match condition {
        Condition::Bool(value) => out.push(json!(value)),
        Condition::FunctionCall(name) => {
            out.push(json!({"f()": name}));
        }
        Condition::Expression(Expression::Variable(name))
            if scope.resolve_choice_label(name).is_some() =>
        {
            // Labels are stored as absolute paths now
            out.push(json!({"CNT?": scope.resolve_choice_label(name).unwrap()}));
        }
        Condition::Expression(Expression::Variable(name))
            if context.qualified_choice_labels.contains_key(name) =>
        {
            out.push(json!({"CNT?": context.qualified_choice_labels[name]}));
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
        Condition::Expression(expression) => {
            emit_expression_ctx(expression, out, Some(context), Some(scope))
        }
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

fn branch_has_explicit_flow_transfer(nodes: &[Node]) -> bool {
    nodes
        .iter()
        .rev()
        .find(|node| !matches!(node, Node::Newline))
        .is_some_and(|node| {
            matches!(
                node,
                Node::Divert(Divert { target, .. }) if target != "END" && target != "DONE"
            )
        })
}

fn node_is_terminal(node: &Node) -> bool {
    match node {
        // Only diverts to END/DONE are truly terminal — they mean "stop here, no gather
        // continuation needed".  Diverts to knots/stitches are NOT terminal: inklecate
        // still appends the gather continuation divert after them.
        Node::Divert(d) => d.target == "END" || d.target == "DONE",
        Node::TunnelReturn
        | Node::TunnelOnwardsWithTarget { .. }
        | Node::ReturnBool(_)
        | Node::ReturnExpr(_) => true,
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
    // First pass: emit only condition tokens to determine their lengths,
    // so we can compute the absolute index of each branch array in the parent.
    let mut cond_tokens_list: Vec<Option<Vec<Value>>> = Vec::new();
    for (branch_condition, _) in branches.iter() {
        let cond_tokens = if let Some(cond) = branch_condition {
            let mut tokens = Vec::new();
            emit_condition(cond, &mut tokens, scope, context)?;
            Some(tokens)
        } else {
            None
        };
        cond_tokens_list.push(cond_tokens);
    }
    // Compute the absolute index of each branch's array in the parent container.
    // For branch i: branch_array_index = conditional_index + sum of (cond_overhead+1) for branches 0..i
    let mut cumulative_offset = 0usize;
    let mut branch_array_indices: Vec<usize> = Vec::new();
    for cond_tokens in &cond_tokens_list {
        let cond_overhead = cond_tokens.as_ref().map_or(0, |t| t.len() + 2);
        branch_array_indices.push(conditional_index + cumulative_offset + cond_overhead);
        cumulative_offset += cond_overhead + 1; // +1 for the array element itself
    }
    // Now emit branch content with the correct scope path for each branch.
    for (branch_index, (_, branch_nodes)) in branches.iter().enumerate() {
        let cond_tokens = cond_tokens_list.remove(0);
        let branch_array_idx = branch_array_indices[branch_index];
        // The branch array element is `[selector, {b:[...]}]`.
        // `b` is a named key in the pair, so the path to b's content is:
        // `scope.path + "." + branch_array_idx + ".b"`
        let branch_scope = scope.conditional_branch(&format!("{branch_array_idx}.b"));
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
    let rejoin_target = joined_path(&scope.path, nop_index);

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

        let branch_array_index = conditional_index + out.len();
        let branch_target = joined_path(
            &joined_path(&scope.path, branch_array_index),
            "b",
        );
        let selector = if has_condition {
            json!({"->": branch_target, "c": true})
        } else {
            json!({"->": branch_target})
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
    emit_expression_ctx(value, &mut value_tokens, Some(context), Some(scope));
    let preamble_len = value_tokens.len() + 2; // ev + value_tokens + /ev

    let num_branches = branches.len();
    // Layout: [preamble_len tokens] [N branch arrays] [nop]
    let nop_index = switch_index + preamble_len + num_branches;
    let exit_target = joined_path(&scope.path, nop_index);

    // Emit all branch bodies first (they all reference exit_target)
    let mut branch_bodies: Vec<EmittedContainer> = Vec::new();
    for (branch_index, (_, body_nodes)) in branches.iter().enumerate() {
        let branch_array_index = switch_index + preamble_len + branch_index;
        let branch_scope =
            scope.conditional_branch(&format!("{branch_array_index}.b"));
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
    for (branch_index, ((case_expr, _), body)) in
        branches.iter().zip(branch_bodies).enumerate()
    {
        let branch_array_index = switch_index + preamble_len + branch_index;
        let branch_target = joined_path(
            &joined_path(&scope.path, branch_array_index),
            "b",
        );
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
            emit_expression_ctx(case_expr, &mut case_tokens, Some(context), Some(scope));
            let mut branch_array = vec![json!("du"), json!("ev")];
            branch_array.extend(case_tokens);
            branch_array.push(json!("=="));
            branch_array.push(json!("/ev"));
            branch_array.push(json!({"->": branch_target, "c": true}));
            branch_array.push(Value::Object(named));
            out.push(Value::Array(branch_array));
        } else {
            let branch_array = vec![json!({"->": branch_target}), Value::Object(named)];
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
