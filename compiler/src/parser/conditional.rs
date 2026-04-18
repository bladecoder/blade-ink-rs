use crate::{
    ast::{Expression, Node},
    error::CompilerError,
};

use super::{
    Line, ParsedStatement,
    expression::parse_expression,
    inline::{parse_condition, parse_inline_conditional, tokenize_inline_content},
};

pub fn looks_like_conditional(content: &str) -> bool {
    content.starts_with('{') && content.contains(':')
}

pub fn parse_conditional(
    lines: &[Line<'_>],
    line_index: &mut usize,
    strip_leading_whitespace: bool,
    parse_stmt: &impl Fn(&[Line<'_>], &mut usize, bool) -> Result<ParsedStatement, CompilerError>,
) -> Result<Vec<Node>, CompilerError> {
    let line = &lines[*line_index];
    let content = if strip_leading_whitespace {
        line.content.trim_start()
    } else {
        line.content.trim()
    };

    if let Some((condition, branch_text)) = parse_inline_conditional(content)? {
        *line_index += 1;

        // Split the branch text by top-level `|` to get true and false branches
        use super::inline::split_top_level_pipe;
        let branches: Vec<&str> = split_top_level_pipe(branch_text);
        let when_true = tokenize_inline_content(branches[0].trim())?;
        let when_false = if branches.len() > 1 {
            Some(tokenize_inline_content(branches[1].trim())?)
        } else {
            None
        };

        let mut nodes = vec![Node::Conditional {
            condition,
            when_true,
            when_false,
        }];

        if line.had_newline {
            nodes.push(Node::Newline);
        }

        return Ok(nodes);
    }

    if content.trim() == "{" {
        return parse_multi_branch_conditional(lines, line_index, parse_stmt);
    }

    let header = content
        .trim()
        .strip_prefix('{')
        .ok_or_else(|| CompilerError::invalid_source("expected conditional block".to_owned()))?;
    let (condition_text, rest_after_colon) = header.split_once(':').ok_or_else(|| {
        CompilerError::invalid_source("conditional block is missing ':'".to_owned())
    })?;

    // Detect switch-style: `{ expr: \n - Case1: ... }` — look ahead at next body line
    let first_body_line_index = *line_index + 1;
    let is_switch = rest_after_colon.trim().is_empty() && first_body_line_index < lines.len() && {
        let first_body = lines[first_body_line_index].content.trim();
        if first_body.starts_with('-')
            && !first_body.starts_with("->")
            && !first_body.starts_with("- else:")
        {
            // Check that the branch looks like "- case_expr: body" (has a colon after stripping -)
            let branch_content = first_body.trim_start_matches('-').trim_start();
            branch_content.contains(':') && !branch_content.starts_with("->")
        } else {
            false
        }
    };

    if is_switch {
        let value = parse_expression(condition_text.trim())?;
        *line_index += 1; // skip the `{ expr:` line
        return parse_switch_conditional(lines, line_index, value, parse_stmt);
    }

    let condition = parse_condition(condition_text.trim())?;
    *line_index += 1;

    let mut when_true = Vec::new();
    let mut when_false = Vec::new();
    let mut in_else = false;
    // Track whether we have seen any `- ` branch separator yet.
    // In `{ cond: - true_branch - false_branch }`, the first `-` starts the true branch,
    // and the second `-` switches to the false/else branch.
    let mut seen_branch_dash = false;

    if line.had_newline {
        when_true.push(Node::Newline);
    }

    while *line_index < lines.len() {
        let body_line = &lines[*line_index];
        let trimmed = body_line.content.trim();

        if trimmed == "}" {
            let closing_had_newline = body_line.had_newline && (*line_index + 1) < lines.len();
            *line_index += 1;

            let mut nodes = vec![Node::Conditional {
                condition,
                when_true,
                when_false: if when_false.is_empty() {
                    None
                } else {
                    Some(when_false)
                },
            }];
            if closing_had_newline {
                nodes.push(Node::Newline);
            }

            return Ok(nodes);
        }

        if trimmed == "- else:" {
            in_else = true;
            *line_index += 1;
            if body_line.had_newline {
                when_false.push(Node::Newline);
            }
            continue;
        }

        // `- else: inline_content` on a single line
        if let Some(else_content) = trimmed.strip_prefix("- else:") {
            in_else = true;
            *line_index += 1;
            let rest = else_content.trim();
            if !rest.is_empty() {
                when_false.extend(tokenize_inline_content(rest)?);
            }
            if body_line.had_newline {
                when_false.push(Node::Newline);
            }
            continue;
        }

        // `- content` as branch body separator (true branch marker) — NOT a gather
        if let Some(branch_content) = trimmed.strip_prefix('-')
            && !branch_content.starts_with('>')
        {
            *line_index += 1;
            let rest = branch_content.trim();
            // Second (or later) `- ` switches to the else branch
            if seen_branch_dash {
                in_else = true;
            }
            seen_branch_dash = true;
            let target = if in_else {
                &mut when_false
            } else {
                &mut when_true
            };
            if !rest.is_empty() {
                // If the branch content starts with `~`, treat it as a tilde statement,
                // not as inline text. e.g. `- ~ x = 5` inside `{true: ... }`.
                if rest.starts_with('~') {
                    // Create a synthetic single-line array for parse_stmt
                    let synthetic = [Line {
                        content: rest,
                        had_newline: false,
                        indent: 0,
                    }];
                    let mut idx = 0;
                    if let ParsedStatement::Nodes(mut nodes) =
                        parse_stmt(&synthetic, &mut idx, true)?
                    {
                        target.append(&mut nodes);
                    }
                } else {
                    target.extend(tokenize_inline_content(rest)?);
                }
            }
            if body_line.had_newline {
                target.push(Node::Newline);
            }
            continue;
        }

        let statement = parse_stmt(lines, line_index, true)?;
        let target = if in_else {
            &mut when_false
        } else {
            &mut when_true
        };
        match statement {
            ParsedStatement::Global(_)
            | ParsedStatement::List(_)
            | ParsedStatement::ExternalFunction(_)
            | ParsedStatement::Const(_) => {
                return Err(CompilerError::unsupported_feature(
                    "global declarations are not supported inside conditionals".to_owned(),
                ));
            }
            ParsedStatement::Nodes(mut nodes) => target.append(&mut nodes),
        }
    }

    Err(CompilerError::invalid_source(
        "unterminated conditional block".to_owned(),
    ))
}

pub fn parse_multi_branch_conditional(
    lines: &[Line<'_>],
    line_index: &mut usize,
    parse_stmt: &impl Fn(&[Line<'_>], &mut usize, bool) -> Result<ParsedStatement, CompilerError>,
) -> Result<Vec<Node>, CompilerError> {
    *line_index += 1;

    let mut branches: Vec<(Option<crate::ast::Condition>, Vec<Node>)> = Vec::new();
    let mut current_condition: Option<crate::ast::Condition> = None;
    let mut current_nodes = Vec::new();

    while *line_index < lines.len() {
        let line = &lines[*line_index];
        let trimmed = line.content.trim();

        if trimmed == "}" {
            let closing_had_newline = line.had_newline && (*line_index + 1) < lines.len();
            *line_index += 1;
            if current_condition.is_some() || !current_nodes.is_empty() {
                branches.push((current_condition.take(), current_nodes));
            }
            let mut nodes = fold_conditional_branches(branches)?;
            if closing_had_newline {
                nodes.push(Node::Newline);
            }
            return Ok(nodes);
        }

        if let Some(header) = trimmed.strip_prefix('-') {
            // Ensure this is a branch header (- cond:) not a divert (->)
            if header.starts_with('>') {
                // This is a divert "-> target", not a branch header; parse as content
                let statement = parse_stmt(lines, line_index, true)?;
                match statement {
                    ParsedStatement::Global(_)
                    | ParsedStatement::List(_)
                    | ParsedStatement::ExternalFunction(_)
                    | ParsedStatement::Const(_) => {
                        return Err(CompilerError::unsupported_feature(
                            "global declarations are not supported inside conditionals".to_owned(),
                        ));
                    }
                    ParsedStatement::Nodes(mut nodes) => current_nodes.append(&mut nodes),
                }
                continue;
            }
            if current_condition.is_some() || !current_nodes.is_empty() {
                branches.push((current_condition.take(), current_nodes));
                current_nodes = Vec::new();
            }

            let header = header.trim_start();
            if let Some(rest) = header.strip_prefix("else:") {
                current_condition = None;
                if !rest.trim().is_empty() {
                    current_nodes.extend(tokenize_inline_content(rest.trim())?);
                    if line.had_newline {
                        current_nodes.push(Node::Newline);
                    }
                }
                *line_index += 1;
                continue;
            }

            // `- { ...` — branch body is a nested conditional block, not a condition expression.
            // Push the current branch and parse the nested block as an else branch.
            if header.trim_start().starts_with('{') {
                if current_condition.is_some() || !current_nodes.is_empty() {
                    branches.push((current_condition.take(), current_nodes));
                    current_nodes = Vec::new();
                }
                current_condition = None;
                // Construct a temporary line whose content is the part after `- ` so that
                // parse_conditional can see `{ 2+2==4: ...` directly.
                let nested_content = header.trim_start();
                let nested_line = Line {
                    content: nested_content,
                    indent: line.indent,
                    had_newline: line.had_newline,
                };
                // Build a temporary slice: swap the current line with the stripped version
                // and append the remaining lines.
                let mut tmp_lines: Vec<Line<'_>> = Vec::with_capacity(lines.len() - *line_index);
                tmp_lines.push(nested_line);
                tmp_lines.extend(lines[*line_index + 1..].iter().map(|l| Line {
                    content: l.content,
                    indent: l.indent,
                    had_newline: l.had_newline,
                }));
                let mut tmp_index = 0usize;
                let nested_nodes = parse_conditional(&tmp_lines, &mut tmp_index, true, parse_stmt)?;
                *line_index += tmp_index; // advance by however many lines parse_conditional consumed
                current_nodes.extend(nested_nodes);
                continue;
            }

            let (condition, rest) = match header.split_once(':') {
                Some(pair) => pair,
                None => {
                    // Bare default branch: `- content` with no colon — treat as else
                    current_condition = None;
                    if !header.trim().is_empty() {
                        current_nodes.extend(tokenize_inline_content(header.trim())?);
                        if line.had_newline {
                            current_nodes.push(Node::Newline);
                        }
                    }
                    *line_index += 1;
                    continue;
                }
            };
            current_condition = Some(parse_condition(condition.trim())?);
            let rest_trimmed = rest.trim();
            if !rest_trimmed.is_empty() {
                if rest_trimmed.starts_with('*') || rest_trimmed.starts_with('+') {
                    // The branch body starts with a choice — parse it via a temporary line.
                    // Do NOT advance line_index yet; build tmp_lines = [choice_line, real_next_lines...]
                    let choice_line = Line {
                        content: rest_trimmed,
                        indent: line.indent + 1,
                        had_newline: false,
                    };
                    let next_line_index = *line_index + 1;
                    let remaining_lines: Vec<Line<'_>> = std::iter::once(choice_line)
                        .chain(lines[next_line_index..].iter().map(|l| Line {
                            content: l.content,
                            indent: l.indent,
                            had_newline: l.had_newline,
                        }))
                        .collect();
                    let mut tmp_idx = 0usize;
                    let stmt = parse_stmt(&remaining_lines, &mut tmp_idx, true)?;
                    // Advance: 1 for the branch header line + however many real lines the choice consumed
                    *line_index += 1 + (tmp_idx.saturating_sub(1));
                    if let ParsedStatement::Nodes(mut nodes) = stmt {
                        current_nodes.append(&mut nodes);
                    }
                } else {
                    current_nodes.extend(tokenize_inline_content(rest_trimmed)?);
                    if line.had_newline {
                        current_nodes.push(Node::Newline);
                    }
                    *line_index += 1;
                }
            } else {
                *line_index += 1;
            }
            continue;
        }

        let statement = parse_stmt(lines, line_index, true)?;
        match statement {
            ParsedStatement::Global(_)
            | ParsedStatement::List(_)
            | ParsedStatement::ExternalFunction(_)
            | ParsedStatement::Const(_) => {
                return Err(CompilerError::unsupported_feature(
                    "global declarations are not supported inside conditionals".to_owned(),
                ));
            }
            ParsedStatement::Nodes(mut nodes) => current_nodes.append(&mut nodes),
        }
    }

    Err(CompilerError::invalid_source(
        "unterminated conditional block".to_owned(),
    ))
}

pub fn fold_conditional_branches(
    mut branches: Vec<(Option<crate::ast::Condition>, Vec<Node>)>,
) -> Result<Vec<Node>, CompilerError> {
    if branches.is_empty() {
        return Ok(Vec::new());
    }

    let mut accumulated_else = None;
    while let Some((condition, nodes)) = branches.pop() {
        if let Some(condition) = condition {
            accumulated_else = Some(vec![Node::Conditional {
                condition,
                when_true: nodes,
                when_false: accumulated_else,
            }]);
        } else {
            accumulated_else = Some(nodes);
        }
    }

    Ok(accumulated_else.unwrap_or_default())
}

/// Parse a switch-style conditional `{ expr:\n - Case1: body\n - Case2: body\n - else: body\n }`.
/// `line_index` points to the first body line (after `{ expr:`).
fn parse_switch_conditional(
    lines: &[Line<'_>],
    line_index: &mut usize,
    value: Expression,
    parse_stmt: &impl Fn(&[Line<'_>], &mut usize, bool) -> Result<ParsedStatement, CompilerError>,
) -> Result<Vec<Node>, CompilerError> {
    let mut branches: Vec<(Option<Expression>, Vec<Node>)> = Vec::new();
    let mut current_case: Option<Expression> = None;
    let mut current_nodes: Vec<Node> = Vec::new();
    let mut closing_had_newline = false;

    while *line_index < lines.len() {
        let line = &lines[*line_index];
        let trimmed = line.content.trim();

        if trimmed == "}" {
            closing_had_newline = line.had_newline && (*line_index + 1) < lines.len();
            *line_index += 1;
            if current_case.is_some() || !current_nodes.is_empty() {
                branches.push((current_case.take(), current_nodes));
            }
            break;
        }

        if let Some(header) = trimmed.strip_prefix('-') {
            if header.starts_with('>') {
                // It's a divert `->`, not a branch header
                let statement = parse_stmt(lines, line_index, true)?;
                if let ParsedStatement::Nodes(mut nodes) = statement {
                    current_nodes.append(&mut nodes);
                }
                continue;
            }

            // Save previous branch
            if current_case.is_some() || !current_nodes.is_empty() {
                branches.push((current_case.take(), current_nodes));
                current_nodes = Vec::new();
            }

            let header = header.trim_start();
            if let Some(rest) = header.strip_prefix("else:") {
                current_case = None; // else branch
                let rest = rest.trim();
                if !rest.is_empty() {
                    let inline_line = Line {
                        content: rest,
                        had_newline: line.had_newline,
                        indent: 0,
                    };
                    let inline_lines = std::slice::from_ref(&inline_line);
                    let mut idx = 0;
                    let statement = parse_stmt(inline_lines, &mut idx, true)?;
                    if let ParsedStatement::Nodes(mut nodes) = statement {
                        current_nodes.append(&mut nodes);
                    }
                }
                *line_index += 1;
                continue;
            }

            let (case_text, rest) = match header.split_once(':') {
                Some(pair) => pair,
                None => {
                    // Bare default branch `- content` with no colon
                    current_case = None;
                    if !header.trim().is_empty() {
                        let inline_line = Line {
                            content: header.trim(),
                            had_newline: line.had_newline,
                            indent: 0,
                        };
                        let inline_lines = std::slice::from_ref(&inline_line);
                        let mut idx = 0;
                        let statement = parse_stmt(inline_lines, &mut idx, true)?;
                        if let ParsedStatement::Nodes(mut nodes) = statement {
                            current_nodes.append(&mut nodes);
                        }
                    }
                    *line_index += 1;
                    continue;
                }
            };
            current_case = Some(parse_expression(case_text.trim())?);
            let rest = rest.trim();
            if !rest.is_empty() {
                let inline_line = Line {
                    content: rest,
                    had_newline: line.had_newline,
                    indent: 0,
                };
                let inline_lines = std::slice::from_ref(&inline_line);
                let mut idx = 0;
                let statement = parse_stmt(inline_lines, &mut idx, true)?;
                if let ParsedStatement::Nodes(mut nodes) = statement {
                    current_nodes.append(&mut nodes);
                }
            }
            *line_index += 1;
            continue;
        }

        let statement = parse_stmt(lines, line_index, true)?;
        match statement {
            ParsedStatement::Global(_)
            | ParsedStatement::List(_)
            | ParsedStatement::ExternalFunction(_)
            | ParsedStatement::Const(_) => {
                return Err(CompilerError::unsupported_feature(
                    "global declarations are not supported inside switch conditionals".to_owned(),
                ));
            }
            ParsedStatement::Nodes(mut nodes) => current_nodes.append(&mut nodes),
        }
    }

    let mut result = vec![Node::SwitchConditional { value, branches }];
    if closing_had_newline {
        result.push(Node::Newline);
    }
    Ok(result)
}
