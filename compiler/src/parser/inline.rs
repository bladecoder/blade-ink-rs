use crate::{
    ast::{Divert, DynamicString, DynamicStringPart, Node, Sequence, SequenceMode},
    error::CompilerError,
};

use super::expression::{parse_call_like, parse_expression};

pub fn tokenize_inline_content(content: &str) -> Result<Vec<Node>, CompilerError> {
    let mut nodes = Vec::new();
    let mut text = String::new();
    let mut chars = content.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if ch == '#' {
            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(&mut text)));
            }
            nodes.push(Node::Tag(parse_dynamic_string(
                content[index + 1..].trim_start(),
            )?));
            break;
        }

        if ch == '<' && content[index..].starts_with("<>") {
            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(&mut text)));
            }
            nodes.push(Node::Glue);
            chars.next();
            continue;
        }

        if ch == '{' {
            let end = find_matching_brace(content, index).ok_or_else(|| {
                CompilerError::InvalidSource("unterminated inline brace expression".to_owned())
            })?;

            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(&mut text)));
            }

            let inline = &content[index..=end];
            if let Some((condition, branch_text)) = parse_inline_conditional(inline)? {
                nodes.push(Node::Conditional {
                    condition,
                    when_true: tokenize_inline_content(branch_text)?,
                    when_false: None,
                });
            } else if let Some(sequence) = parse_inline_sequence(&content[index + 1..end])? {
                nodes.push(Node::Sequence(sequence));
            } else {
                let expression = parse_expression(&content[index + 1..end])?;
                nodes.push(Node::OutputExpression(expression));
            }

            while let Some((peek_index, _)) = chars.peek() {
                if *peek_index <= end {
                    chars.next();
                } else {
                    break;
                }
            }

            continue;
        }

        text.push(ch);
    }

    if !text.is_empty() {
        nodes.push(Node::Text(text));
    }

    Ok(nodes)
}

pub fn parse_dynamic_string(input: &str) -> Result<DynamicString, CompilerError> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut chars = input.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if ch == '{' {
            let end = find_matching_brace(input, index).ok_or_else(|| {
                CompilerError::InvalidSource("unterminated inline brace expression".to_owned())
            })?;

            if !text.is_empty() {
                parts.push(DynamicStringPart::Text(std::mem::take(&mut text)));
            }

            let inner = &input[index + 1..end];
            if let Some(sequence) = parse_inline_sequence(inner)? {
                parts.push(DynamicStringPart::Sequence(sequence));
            } else {
                parts.push(DynamicStringPart::Expression(parse_expression(inner)?));
            }

            while let Some((peek_index, _)) = chars.peek() {
                if *peek_index <= end {
                    chars.next();
                } else {
                    break;
                }
            }

            continue;
        }

        text.push(ch);
    }

    if !text.is_empty() {
        parts.push(DynamicStringPart::Text(text));
    }

    Ok(DynamicString { parts })
}

pub fn parse_divert(input: &str) -> Result<Divert, CompilerError> {
    if let Some((target, args)) = parse_call_like(input)? {
        return Ok(Divert {
            target,
            arguments: args,
        });
    }

    Ok(Divert {
        target: input.trim().to_owned(),
        arguments: Vec::new(),
    })
}

pub fn parse_divert_line(input: &str) -> Result<Vec<Node>, CompilerError> {
    let trimmed = input.trim();
    if trimmed == "->->" {
        return Ok(vec![Node::TunnelReturn]);
    }

    let segments: Vec<&str> = trimmed
        .split("->")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();

    if segments.is_empty() {
        return Err(CompilerError::InvalidSource(
            "expected divert target after '->'".to_owned(),
        ));
    }

    if trimmed.ends_with("->") && segments.len() > 1 {
        return Ok(segments
            .into_iter()
            .map(|segment| Node::TunnelDivert(segment.to_owned()))
            .collect());
    }

    Ok(vec![Node::Divert(parse_divert(segments[0])?)])
}

pub fn split_inline_divert(content: &str) -> Option<(&str, &str)> {
    let index = content.rfind("->")?;
    let (text, divert) = content.split_at(index);
    let divert = divert.strip_prefix("->")?.trim();
    if divert.is_empty() {
        None
    } else {
        Some((text, divert))
    }
}

pub fn split_inline_choice_divert(input: &str) -> Result<(&str, Option<Divert>), CompilerError> {
    if let Some((text, divert_part)) = split_inline_divert(input) {
        return Ok((text.trim_end(), Some(parse_divert(divert_part)?)));
    }

    Ok((input, None))
}

pub fn split_text_and_tags(input: &str) -> Result<(String, Vec<DynamicString>), CompilerError> {
    if let Some((text, tag_text)) = input.split_once('#') {
        return Ok((
            text.to_owned(),
            vec![parse_dynamic_string(tag_text.trim_start())?],
        ));
    }

    Ok((input.to_owned(), Vec::new()))
}

pub fn split_top_level_pipe(input: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut start = 0;

    for (index, ch) in input.char_indices() {
        match ch {
            '{' | '(' => depth += 1,
            '}' | ')' => depth -= 1,
            '|' if depth == 0 => {
                result.push(&input[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }

    result.push(&input[start..]);
    result
}

pub fn find_matching_brace(content: &str, start: usize) -> Option<usize> {
    let mut depth = 0;
    for (index, ch) in content[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + index);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn parse_inline_conditional(
    content: &str,
) -> Result<Option<(crate::ast::Condition, &str)>, CompilerError> {
    let trimmed = content.trim();
    if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
        return Ok(None);
    }

    let inner = &trimmed[1..trimmed.len() - 1];
    let (condition, branch) = match inner.split_once(':') {
        Some(parts) => parts,
        None => return Ok(None),
    };

    Ok(Some((
        parse_condition(condition.trim())?,
        branch.trim_start(),
    )))
}

pub fn parse_condition(condition: &str) -> Result<crate::ast::Condition, CompilerError> {
    use crate::ast::Condition;
    match condition {
        "true" => Ok(Condition::Bool(true)),
        "false" => Ok(Condition::Bool(false)),
        _ => {
            if let Some(name) = condition.strip_suffix("()") {
                return Ok(Condition::FunctionCall(name.trim().to_owned()));
            }

            Ok(Condition::Expression(parse_expression(condition)?))
        }
    }
}

pub fn parse_inline_sequence(content: &str) -> Result<Option<Sequence>, CompilerError> {
    let mode = SequenceMode::Stopping;
    let parts = split_top_level_pipe(content);
    if parts.len() < 2 {
        return Ok(None);
    }

    let mut branches = Vec::new();
    for part in parts {
        branches.push(tokenize_inline_content(part.trim())?);
    }

    Ok(Some(Sequence { mode, branches }))
}
