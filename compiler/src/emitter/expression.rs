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
        out.push(emit_sequence(
            sequence,
            scope,
            out.content.len() + scope.param_offset,
            context,
        )?);
    }

    Ok(())
}

fn emit_sequence(
    sequence: &Sequence,
    scope: &EmitScope,
    sequence_index: usize,
    context: &EmitContext,
) -> Result<Value, CompilerError> {
    let sequence_path = joined_path(&scope.path, sequence_index);
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
            out.push(json!({"->": joined_path(&sequence_path, 10), "c": true}));
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
        out.push(json!({
            "->": joined_path(&sequence_path, format!("s{index}")),
            "c": true
        }));
    }
    let rejoin_index = out.len();
    out.push(json!("nop"));

    let mut named = Map::new();
    for (index, branch) in sequence.branches.iter().enumerate() {
        let branch_scope =
            scope.at_path(joined_path(&sequence_path, format!("s{index}")));
        let mut branch_container = emit_nodes(branch, &branch_scope, context)?;
        branch_container.content.insert(0, json!("pop"));
        branch_container.push(json!({"->": joined_path(&sequence_path, rejoin_index)}));
        named.insert(
            format!("s{index}"),
            branch_container.into_json_array(None, None)?,
        );
    }
    if has_once_fallthrough {
        let mut branch_container = EmittedContainer::default();
        branch_container.push(json!("pop"));
        branch_container.push(json!({"->": joined_path(&sequence_path, rejoin_index)}));
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
            emit_expression_ctx(argument, &mut out.content, Some(context), Some(scope));
        }
        out.push(json!("/ev"));
    }

    if scope.is_variable_divert(&resolved_target, context) {
        out.push(json!({"->": resolved_target, "var": true}));
    } else {
        out.push(json!({"->": resolved_target}));
    }
}

fn expression_has_function_call(expr: &Expression) -> bool {
    match expr {
        Expression::FunctionCall { .. } => true,
        Expression::Negate(inner) | Expression::Not(inner) => expression_has_function_call(inner),
        Expression::Binary { left, right, .. } => {
            expression_has_function_call(left) || expression_has_function_call(right)
        }
        _ => false,
    }
}

fn emit_assignment(
    variable_name: &str,
    expression: &Expression,
    mode: &AssignMode,
    out: &mut EmittedContainer,
    context: &EmitContext,
    scope: Option<&EmitScope>,
) {
    // When the target variable is a parameter of the enclosing function/knot, it lives
    // in the temp-variable frame, so inklecate uses `{"temp=": name, "re": true}` rather
    // than `{"VAR=": name, "re": true}`.
    let is_param = scope.is_some_and(|s| s.temp_param_names.contains(variable_name));

    match mode {
        AssignMode::Set => {
            out.push(json!("ev"));
            emit_expression_ctx(expression, &mut out.content, Some(context), scope);
            out.push(json!("/ev"));
            if is_param {
                out.push(json!({"temp=": variable_name, "re": true}));
            } else {
                out.push(json!({"VAR=": variable_name, "re": true}));
            }
            if expression_has_function_call(expression) {
                out.push(json!("\n"));
            }
        }
        AssignMode::TempSet => {
            out.push(json!("ev"));
            emit_expression_ctx(expression, &mut out.content, Some(context), scope);
            out.push(json!("/ev"));
            out.push(json!({"temp=": variable_name}));
            if expression_has_function_call(expression) {
                out.push(json!("\n"));
            }
        }
        AssignMode::AddAssign => {
            out.push(json!("ev"));
            emit_expression_ctx(
                &Expression::Variable(variable_name.to_owned()),
                &mut out.content,
                Some(context),
                scope,
            );
            emit_expression_ctx(expression, &mut out.content, Some(context), scope);
            out.push(json!("+"));
            if is_param {
                out.push(json!({"temp=": variable_name, "re": true}));
            } else {
                out.push(json!({"VAR=": variable_name, "re": true}));
            }
            out.push(json!("/ev"));
        }
        AssignMode::SubtractAssign => {
            out.push(json!("ev"));
            emit_expression_ctx(
                &Expression::Variable(variable_name.to_owned()),
                &mut out.content,
                Some(context),
                scope,
            );
            emit_expression_ctx(expression, &mut out.content, Some(context), scope);
            out.push(json!("-"));
            if is_param {
                out.push(json!({"temp=": variable_name, "re": true}));
            } else {
                out.push(json!({"VAR=": variable_name, "re": true}));
            }
            out.push(json!("/ev"));
        }
    }
}

fn emit_expression(expression: &Expression, out: &mut Vec<Value>) {
    emit_expression_ctx(expression, out, None, None);
}

fn emit_expression_ctx(
    expression: &Expression,
    out: &mut Vec<Value>,
    context: Option<&EmitContext>,
    scope: Option<&EmitScope>,
) {
    match expression {
        Expression::Bool(value) => out.push(json!(value)),
        Expression::Int(value) => out.push(json!(value)),
        Expression::Float(value) => out.push(float_to_json(*value)),
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
                            emit_expression_ctx(expr, out, context, scope);
                            out.push(json!("out"));
                            out.push(json!("/ev"));
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
            if let Some(path) = scope.and_then(|s| s.resolve_choice_label(name)) {
                out.push(json!({"CNT?": path}))
            } else if let Some(path) = context.and_then(|ctx| ctx.qualified_choice_labels.get(name))
            {
                out.push(json!({"CNT?": path}))
            } else if name.contains('.') {
                out.push(json!({"CNT?": name}))
            } else if let (Some(s), Some(ctx)) = (scope, context)
                && (ctx.top_flow_names.contains(name)
                    || s.child_flow_names.contains(name)
                    || s.sibling_flow_names.contains(name))
            {
                out.push(json!({"CNT?": s.resolve_divert_target(name, ctx)}))
            } else if context.is_some_and(|ctx| ctx.top_flow_names.contains(name)) {
                out.push(json!({"CNT?": name}))
            } else {
                out.push(json!({"VAR?": name}))
            }
        }
        Expression::DivertTarget(target) => {
            let resolved = if let (Some(s), Some(ctx)) = (scope, context) {
                s.resolve_divert_target(target, ctx)
            } else {
                target.clone()
            };
            out.push(json!({"^->": resolved}));
        }
        Expression::Negate(expr) => {
            // Constant-fold negation of integer/float literals to match inklecate's
            // output: emit `-3` directly instead of `3, "_"`.
            match expr.as_ref() {
                Expression::Int(n) => out.push(json!(-n)),
                Expression::Float(f) => out.push(float_to_json(-f)),
                other => {
                    emit_expression_ctx(other, out, context, scope);
                    out.push(json!("_"));
                }
            }
        }
        Expression::Not(expr) => {
            emit_expression_ctx(expr, out, context, scope);
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
                    emit_expression_ctx(&args[0], out, context, scope);
                    out.push(json!("listInt"));
                } else {
                    // Fallback: treat as user function call
                    for arg in args {
                        emit_expression_ctx(arg, out, context, scope);
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
            for (index, arg) in args.iter().enumerate() {
                emit_call_argument(name, index, arg, out, context, scope);
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
            emit_expression_ctx(left, out, context, scope);
            emit_expression_ctx(right, out, context, scope);
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

fn emit_call_argument(
    function_name: &str,
    arg_index: usize,
    arg: &Expression,
    out: &mut Vec<Value>,
    context: Option<&EmitContext>,
    scope: Option<&EmitScope>,
) {
    let is_ref_arg = context
        .and_then(|ctx| ctx.function_ref_param_positions.get(function_name))
        .and_then(|positions| positions.get(arg_index))
        .copied()
        .unwrap_or(false);

    if is_ref_arg && let Expression::Variable(name) = arg {
        out.push(json!({"^var": name, "ci": -1}));
    } else {
        emit_expression_ctx(arg, out, context, scope);
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
            emit_expression_ctx(expression, out, Some(context), Some(scope));
            out.push(json!("out"));
            out.push(json!("/ev"));
            emit_dynamic_string_parts(&parts[1..], out, scope, context)
        }
        DynamicStringPart::Sequence(sequence) => {
            out.push(emit_sequence(
                sequence,
                scope,
                out.len() + scope.param_offset,
                context,
            )?);
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

fn emit_choice_text_content(
    text: &str,
    tags: &[DynamicString],
    out: &mut Vec<Value>,
    scope: &EmitScope,
    context: &EmitContext,
) -> Result<(), CompilerError> {
    if text.is_empty() && tags.is_empty() {
        return Ok(());
    }

    if !text.is_empty() {
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

    Ok(())
}
