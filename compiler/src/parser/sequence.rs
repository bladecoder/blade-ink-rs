use crate::{
    ast::{Node, Sequence, SequenceMode},
    error::CompilerError,
};

use super::{Line, ParsedStatement, inline::tokenize_inline_content};

pub fn looks_like_sequence(content: &str) -> bool {
    if !content.starts_with('{') || !content.contains(':') {
        return false;
    }

    let Some(inner) = content.strip_prefix('{') else {
        return false;
    };
    let Some((head, _)) = inner.split_once(':') else {
        return false;
    };

    parse_sequence_mode(head.trim()).is_ok()
}

pub fn parse_sequence_mode(input: &str) -> Result<SequenceMode, CompilerError> {
    match input {
        "stopping" => Ok(SequenceMode::Stopping),
        "once" => Ok(SequenceMode::Once),
        "cycle" => Ok(SequenceMode::Cycle),
        "shuffle" => Ok(SequenceMode::Shuffle),
        "shuffle once" => Ok(SequenceMode::ShuffleOnce),
        "stopping shuffle" => Ok(SequenceMode::ShuffleStopping),
        _ => Err(CompilerError::unsupported_feature(format!(
            "unsupported sequence mode '{input}'"
        ))),
    }
}

pub fn is_sequence_branch_header(trimmed: &str) -> bool {
    trimmed.starts_with('-') && !trimmed.starts_with("->")
}

pub fn parse_sequence(
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

    if content == "{" {
        return parse_multi_branch_sequence(lines, line_index, parse_stmt);
    }

    let header = content
        .strip_prefix('{')
        .ok_or_else(|| CompilerError::invalid_source("expected sequence block".to_owned()))?;
    let (mode_text, _) = header
        .split_once(':')
        .ok_or_else(|| CompilerError::invalid_source("sequence block is missing ':'".to_owned()))?;
    let mode = parse_sequence_mode(mode_text.trim())?;

    *line_index += 1;
    let mut branches = Vec::new();

    while *line_index < lines.len() {
        let body_line = &lines[*line_index];
        let trimmed = body_line.content.trim();

        if trimmed == "}" {
            *line_index += 1;
            return Ok(vec![Node::Sequence(Sequence { mode, branches })]);
        }

        // Skip blank lines and comments between branches
        if trimmed.is_empty() || trimmed.starts_with("//") {
            *line_index += 1;
            continue;
        }

        let branch_text = trimmed.strip_prefix('-').ok_or_else(|| {
            CompilerError::invalid_source("sequence branch must start with '-'".to_owned())
        })?;
        *line_index += 1;

        let mut branch_nodes = if branch_text.trim().is_empty() {
            Vec::new()
        } else {
            tokenize_inline_content(branch_text.trim_start())?
        };
        if body_line.had_newline {
            branch_nodes.insert(0, Node::Newline);
            branch_nodes.push(Node::Newline);
        }

        while *line_index < lines.len() {
            let next_line = &lines[*line_index];
            let next_trimmed = next_line.content.trim();
            if next_trimmed == "}"
                || is_sequence_branch_header(next_trimmed)
                || super::parse_header(next_line.content).is_some()
            {
                break;
            }

            let statement = parse_stmt(lines, line_index, true)?;
            match statement {
                ParsedStatement::Global(_)
                | ParsedStatement::List(_)
                | ParsedStatement::ExternalFunction(_) => {
                    return Err(CompilerError::unsupported_feature(
                        "global declarations are not supported inside sequences".to_owned(),
                    ));
                }
                ParsedStatement::Nodes(mut nodes) => branch_nodes.append(&mut nodes),
            }
        }

        branches.push(branch_nodes);
    }

    Err(CompilerError::invalid_source(
        "unterminated sequence block".to_owned(),
    ))
}

pub fn parse_multi_branch_sequence(
    lines: &[Line<'_>],
    line_index: &mut usize,
    parse_stmt: &impl Fn(&[Line<'_>], &mut usize, bool) -> Result<ParsedStatement, CompilerError>,
) -> Result<Vec<Node>, CompilerError> {
    *line_index += 1;

    let mut mode = None;
    let mut branches = Vec::new();

    while *line_index < lines.len() {
        let line = &lines[*line_index];
        let trimmed = line.content.trim();
        if trimmed == "}" {
            *line_index += 1;
            return Ok(vec![Node::Sequence(Sequence {
                mode: mode.unwrap_or(SequenceMode::Stopping),
                branches,
            })]);
        }

        // Skip blank lines and comments between branches
        if trimmed.is_empty() || trimmed.starts_with("//") {
            *line_index += 1;
            continue;
        }

        let header = trimmed.strip_prefix('-').ok_or_else(|| {
            CompilerError::invalid_source("sequence branch must start with '-'".to_owned())
        })?;
        let header = header.trim_start();
        let (branch_mode, inline_text) =
            if let Some((candidate_mode, rest)) = header.split_once(':') {
                if let Ok(parsed_mode) = parse_sequence_mode(candidate_mode.trim()) {
                    (Some(parsed_mode), rest.trim_start())
                } else {
                    (None, header)
                }
            } else {
                (None, header)
            };

        if let Some(branch_mode) = branch_mode {
            mode = Some(branch_mode);
        }

        *line_index += 1;
        let mut branch_nodes = if inline_text.is_empty() {
            Vec::new()
        } else {
            tokenize_inline_content(inline_text)?
        };
        if line.had_newline {
            branch_nodes.insert(0, Node::Newline);
            branch_nodes.push(Node::Newline);
        }

        while *line_index < lines.len() {
            let next_line = &lines[*line_index];
            let next_trimmed = next_line.content.trim();
            if next_trimmed == "}"
                || is_sequence_branch_header(next_trimmed)
                || super::parse_header(next_line.content).is_some()
            {
                break;
            }

            let statement = parse_stmt(lines, line_index, true)?;
            match statement {
                ParsedStatement::Global(_)
                | ParsedStatement::List(_)
                | ParsedStatement::ExternalFunction(_) => {
                    return Err(CompilerError::unsupported_feature(
                        "global declarations are not supported inside sequences".to_owned(),
                    ));
                }
                ParsedStatement::Nodes(mut nodes) => branch_nodes.append(&mut nodes),
            }
        }

        branches.push(branch_nodes);
    }

    Err(CompilerError::invalid_source(
        "unterminated sequence block".to_owned(),
    ))
}
