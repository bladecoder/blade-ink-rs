use crate::{ast::Node, error::CompilerError};

use super::{
    inline::{parse_condition, parse_inline_conditional, tokenize_inline_content},
    Line, ParsedStatement,
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
        let mut nodes = vec![Node::Conditional {
            condition,
            when_true: tokenize_inline_content(branch_text)?,
            when_false: None,
        }];

        if line.had_newline {
            nodes.push(Node::Newline);
        }

        return Ok(nodes);
    }

    if content == "{" {
        return parse_multi_branch_conditional(lines, line_index, parse_stmt);
    }

    let header = content
        .strip_prefix('{')
        .ok_or_else(|| CompilerError::InvalidSource("expected conditional block".to_owned()))?;
    let (condition_text, _) = header.split_once(':').ok_or_else(|| {
        CompilerError::InvalidSource("conditional block is missing ':'".to_owned())
    })?;

    let condition = parse_condition(condition_text.trim())?;
    *line_index += 1;

    let mut when_true = Vec::new();
    let mut when_false = Vec::new();
    let mut in_else = false;

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

        let statement = parse_stmt(lines, line_index, true)?;
        let target = if in_else {
            &mut when_false
        } else {
            &mut when_true
        };
        match statement {
            ParsedStatement::Global(_) | ParsedStatement::List(_) => {
                return Err(CompilerError::UnsupportedFeature(
                    "global declarations are not supported inside conditionals".to_owned(),
                ))
            }
            ParsedStatement::Nodes(mut nodes) => target.append(&mut nodes),
        }
    }

    Err(CompilerError::InvalidSource(
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
                    ParsedStatement::Global(_) | ParsedStatement::List(_) => {
                        return Err(CompilerError::UnsupportedFeature(
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

            let (condition, rest) = header.split_once(':').ok_or_else(|| {
                CompilerError::InvalidSource(
                    "conditional branch is missing ':' after condition".to_owned(),
                )
            })?;
            current_condition = Some(parse_condition(condition.trim())?);
            if !rest.trim().is_empty() {
                current_nodes.extend(tokenize_inline_content(rest.trim())?);
                if line.had_newline {
                    current_nodes.push(Node::Newline);
                }
            }
            *line_index += 1;
            continue;
        }

        let statement = parse_stmt(lines, line_index, true)?;
        match statement {
            ParsedStatement::Global(_) | ParsedStatement::List(_) => {
                return Err(CompilerError::UnsupportedFeature(
                    "global declarations are not supported inside conditionals".to_owned(),
                ));
            }
            ParsedStatement::Nodes(mut nodes) => current_nodes.append(&mut nodes),
        }
    }

    Err(CompilerError::InvalidSource(
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
