fn parse_content_line(
    line: &Line<'_>,
    strip_leading_whitespace: bool,
) -> Result<Vec<Node>, CompilerError> {
    let content = if strip_leading_whitespace || line.indent > 0 {
        line.content.trim_start()
    } else {
        line.content
    };

    let mut nodes = Vec::new();
    if let Some((text_part, divert_part)) = split_inline_divert(content) {
        nodes.extend(tokenize_inline_content(text_part)?);
        nodes.push(Node::Divert(parse_divert(divert_part)?));
    } else {
        nodes.extend(tokenize_inline_content(content)?);
    }

    if line.had_newline {
        // Trim trailing whitespace from the last text node (inklecate behavior)
        if let Some(Node::Text(t)) = nodes.last_mut() {
            let trimmed = t.trim_end().to_owned();
            if trimmed.is_empty() {
                nodes.pop();
            } else {
                *t = trimmed;
            }
        }
        nodes.push(Node::Newline);
    }

    Ok(nodes)
}

fn parse_global_assignment(input: &str) -> Result<GlobalVariable, CompilerError> {
    let (name, expression) = split_assignment(input, "=")?;
    Ok(GlobalVariable {
        name,
        initial_value: parse_expression(&expression)?,
    })
}

/// Parse `name = item1, (item2), item3, ...` into a ListDeclaration.
fn parse_list_declaration(input: &str) -> Result<ListDeclaration, CompilerError> {
    let eq_pos = input.find('=').ok_or_else(|| {
        CompilerError::invalid_source(format!("LIST declaration missing '=': {input}"))
    })?;
    let name = input[..eq_pos].trim().to_owned();
    let rhs = input[eq_pos + 1..].trim();

    let mut items = Vec::new();
    let mut value: u32 = 1;
    for raw in rhs.split(',') {
        let item = raw.trim();
        if item.is_empty() {
            continue;
        }
        // Strip optional parens (marks the item as initially selected)
        let (inner, selected) =
            if let Some(inner) = item.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
                (inner.trim(), true)
            } else {
                (item, false)
            };
        // Check for explicit value assignment: `name = number`
        if let Some((item_name, item_value)) = inner.split_once('=') {
            let item_name = item_name.trim().to_owned();
            let explicit_value: u32 = item_value.trim().parse().map_err(|_| {
                CompilerError::invalid_source(format!(
                    "invalid LIST item value: '{}'",
                    item_value.trim()
                ))
            })?;
            value = explicit_value;
            items.push((item_name, value, selected));
        } else {
            items.push((inner.to_owned(), value, selected));
        }
        value += 1;
    }

    Ok(ListDeclaration { name, items })
}

fn parse_assignment(input: &str) -> Result<Node, CompilerError> {
    // Strip optional `temp` keyword (marks the assignment as a local/temporary variable)
    let (input, is_temp) = if let Some(rest) = input.strip_prefix("temp ") {
        (rest.trim_start(), true)
    } else {
        (input, false)
    };

    // x++ sugar for x += 1
    if let Some(name) = input.strip_suffix("++") {
        let name = name.trim().to_owned();
        return Ok(Node::Assignment {
            variable_name: name,
            expression: Expression::Int(1),
            mode: AssignMode::AddAssign,
        });
    }

    // x-- sugar for x -= 1
    if let Some(name) = input.strip_suffix("--") {
        let name = name.trim().to_owned();
        return Ok(Node::Assignment {
            variable_name: name,
            expression: Expression::Int(1),
            mode: AssignMode::SubtractAssign,
        });
    }

    // Check for a standalone function call (no '=' in the statement, but has '()')
    // e.g. `~ derp(2, 3, 4)` or `~ merchant_init()`
    if !input.contains('=')
        && let Ok(Some((name, args))) = parse_call_like(input)
    {
        return Ok(Node::VoidCall { name, args });
    }

    if input.contains("+=") {
        let (name, expression) = split_assignment(input, "+=")?;
        return Ok(Node::Assignment {
            variable_name: name,
            expression: parse_expression(&expression)?,
            mode: AssignMode::AddAssign,
        });
    }

    if input.contains("-=") {
        let (name, expression) = split_assignment(input, "-=")?;
        return Ok(Node::Assignment {
            variable_name: name,
            expression: parse_expression(&expression)?,
            mode: AssignMode::SubtractAssign,
        });
    }

    let (name, expression) = split_assignment(input, "=")?;
    Ok(Node::Assignment {
        variable_name: name,
        expression: parse_expression(&expression)?,
        mode: if is_temp {
            AssignMode::TempSet
        } else {
            AssignMode::Set
        },
    })
}

/// Returns true if the leading `{` in `content` is NOT closed on the same line —
/// meaning the block spans multiple lines and should be parsed by the conditional/sequence
/// parser rather than the inline tokenizer.
fn brace_spans_multiple_lines(content: &str) -> bool {
    let mut depth: i32 = 0;
    for ch in content.chars() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return false; // first '{' was closed on this line
                }
            }
            _ => {}
        }
    }
    true // '{' was never closed — spans multiple lines
}

fn split_assignment(input: &str, separator: &str) -> Result<(String, String), CompilerError> {
    let (name, expression) = input.split_once(separator).ok_or_else(|| {
        CompilerError::invalid_source(format!("expected assignment using '{separator}'"))
    })?;

    Ok((name.trim().to_owned(), expression.trim().to_owned()))
}

