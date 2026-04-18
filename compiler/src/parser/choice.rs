use crate::{
    ast::{Choice, Condition, Divert, DynamicString, Node},
    error::CompilerError,
};

use super::{
    Line, ParsedStatement,
    inline::{
        parse_condition, parse_divert, split_inline_choice_divert, split_inline_divert,
        split_text_and_tags,
    },
};

pub struct ParsedChoiceText {
    pub display_text: String,
    pub selected_text: Option<String>,
    pub start_text: String,
    pub choice_only_text: String,
    pub has_start_content: bool,
    pub has_choice_only_content: bool,
    pub inline_target: Option<Divert>,
    pub start_tags: Vec<DynamicString>,
    pub choice_only_tags: Vec<DynamicString>,
    pub selected_tags: Vec<DynamicString>,
}

pub fn parse_choice(
    lines: &[Line<'_>],
    line_index: &mut usize,
    parse_stmt: &impl Fn(&[Line<'_>], &mut usize, bool) -> Result<ParsedStatement, CompilerError>,
) -> Result<ParsedStatement, CompilerError> {
    let line = &lines[*line_index];
    let trimmed_start = line.content.trim_start();
    let marker = trimmed_start
        .chars()
        .next()
        .ok_or_else(|| CompilerError::invalid_source("expected choice marker".to_owned()))?;
    let once_only = marker == '*';
    // Count nesting level and strip all leading choice markers (e.g. "* * text" for nested
    // choices) — nesting is handled by indentation.
    let after_first_marker = trimmed_start[marker.len_utf8()..].trim_start();
    let mut nesting_level: usize = 1;
    let mut remainder = after_first_marker;
    while let Some(rest) = remainder.strip_prefix(|c| c == '*' || c == '+') {
        remainder = rest.trim_start();
        nesting_level += 1;
    }
    let (label, mut conditions, remainder) = parse_choice_prefixes(remainder)?;
    let mut choice_text = parse_choice_text(remainder)?;
    let mut is_invisible_default =
        choice_text.display_text.is_empty() && choice_text.selected_text.is_none();

    *line_index += 1;

    let choice_indent = line.indent;

    // Look ahead at indented body lines: if they start with `{ condition }` (optionally followed
    // by display text), absorb the condition and, if text is present, set it as the start_text.
    // This implements multi-line choice conditions:
    //   * { cond1 }
    //     { cond2 }  display text   OR   plain display text
    // Only scan when there is no start text yet on the header line.
    // We scan when: the header has conditions OR we absorb body conditions.
    let header_has_conditions = !conditions.is_empty();
    if choice_text.start_text.is_empty() && choice_text.choice_only_text.is_empty() {
        let mut absorbed_body_conditions = false;
        while *line_index < lines.len() {
            let peek = &lines[*line_index];
            let peek_trimmed = peek.content.trim();
            // Must be indented deeper than choice
            if peek.indent <= choice_indent {
                break;
            }
            // If starts with `{`, it only counts as an additional choice condition when the
            // choice was already established as conditional on the header or by an earlier body
            // condition line. Otherwise it's normal body content, such as `{5}` in a default
            // choice body.
            if peek_trimmed.starts_with('{') && (header_has_conditions || absorbed_body_conditions)
            {
                // Find the matching `}`
                if let Some(close) = peek_trimmed.find('}') {
                    let cond_str = &peek_trimmed[1..close];
                    let after_close = peek_trimmed[close + 1..].trim();
                    conditions.push(parse_condition(cond_str.trim())?);
                    absorbed_body_conditions = true;
                    *line_index += 1;
                    if !after_close.is_empty() {
                        // Remaining text after `{ cond }` becomes the display text
                        choice_text.display_text = after_close.to_owned();
                        choice_text.start_text = after_close.to_owned();
                        choice_text.selected_text = Some(after_close.to_owned());
                        choice_text.has_start_content = true;
                        is_invisible_default = false;
                        break;
                    }
                    // Pure condition line — continue looking for more conditions/text
                } else {
                    break;
                }
            } else if absorbed_body_conditions || header_has_conditions {
                // After conditions (header or body), a plain text line becomes start text.
                // Skip if it looks like a gather, divert, or sub-choice.
                if peek_trimmed.starts_with('-')
                    || peek_trimmed.starts_with('*')
                    || peek_trimmed.starts_with('+')
                {
                    break;
                }
                choice_text.display_text = peek_trimmed.to_owned();
                choice_text.start_text = peek_trimmed.to_owned();
                choice_text.selected_text = Some(peek_trimmed.to_owned());
                choice_text.has_start_content = true;
                is_invisible_default = false;
                *line_index += 1;
                break;
            } else {
                break;
            }
        }
    }

    let mut body = Vec::new();
    // Once we absorb a same-level gather, everything that follows at our indent
    // (choices, text, etc.) becomes part of this choice's body continuation.
    let mut absorbed_gather = false;

    if let Some(divert) = choice_text.inline_target.clone() {
        body.push(Node::Divert(divert));
    }

    while *line_index < lines.len() {
        let body_line = &lines[*line_index];
        let body_trimmed = body_line.content.trim();
        // Terminate on knot/stitch headers
        if super::parse_header(body_line.content).is_some() {
            break;
        }

        let gather_level = gather_nesting_level(body_trimmed);

        // A gather whose nesting level matches ours AND which is indented deeper than the
        // choice itself is the "end of sub-choices / start of continuation" boundary for
        // this weave level.  Absorb it so the emitter sees it as a GatherPoint separating
        // the inner choice block from the post-gather continuation.
        if gather_level == nesting_level && body_line.indent > choice_indent {
            let statement = parse_stmt(lines, line_index, true)?;
            if let ParsedStatement::Nodes(mut nodes) = statement {
                body.append(&mut nodes)
            }
            absorbed_gather = true;
            continue;
        }

        // A gather at or shallower than our choice indent terminates the choice body.
        if gather_level > 0 && body_line.indent <= choice_indent {
            break;
        }

        // Non-gather line at or shallower than our indent:
        // - If we haven't absorbed a same-level gather yet, this is a sibling — stop.
        // - If we have absorbed a gather, it's the post-gather continuation — include it.
        if gather_level == 0 && body_line.indent <= choice_indent && !absorbed_gather {
            break;
        }

        let statement = parse_stmt(lines, line_index, true)?;
        match statement {
            ParsedStatement::Global(_)
            | ParsedStatement::List(_)
            | ParsedStatement::ExternalFunction(_)
            | ParsedStatement::Const(_) => {
                return Err(CompilerError::unsupported_feature(
                    "global declarations are not supported inside choice bodies".to_owned(),
                ));
            }
            ParsedStatement::Nodes(mut nodes) => body.append(&mut nodes),
        }
    }

    Ok(ParsedStatement::Nodes(vec![Node::Choice(Choice {
        display_text: choice_text.display_text.clone(),
        selected_text: choice_text.selected_text.clone(),
        body,
        start_text: choice_text.start_text,
        choice_only_text: choice_text.choice_only_text,
        conditions,
        label,
        once_only,
        is_invisible_default,
        has_start_content: choice_text.has_start_content,
        has_choice_only_content: choice_text.has_choice_only_content,
        start_tags: choice_text.start_tags,
        choice_only_tags: choice_text.choice_only_tags,
        selected_tags: choice_text.selected_tags,
        nesting_level,
    })]))
}

/// Count the gather nesting level of a trimmed line (e.g. "- text" = 1, "- - text" = 2).
/// Returns 0 if the line is not a gather (doesn't start with '-' or starts with '->').
fn gather_nesting_level(trimmed: &str) -> usize {
    if trimmed.starts_with("->") {
        return 0;
    }
    let mut s = trimmed;
    let mut level = 0;
    while let Some(rest) = s.strip_prefix('-') {
        level += 1;
        let next = rest.trim_start();
        if next.starts_with("->") || !next.starts_with('-') {
            break;
        }
        s = next;
    }
    level
}

pub fn parse_choice_prefixes(
    input: &str,
) -> Result<(Option<String>, Vec<Condition>, &str), CompilerError> {
    let mut remainder = input.trim_start();
    let mut label = None;
    let mut conditions = Vec::new();

    if let Some(after_open) = remainder.strip_prefix('(') {
        let end = after_open.find(')').ok_or_else(|| {
            CompilerError::invalid_source("choice label is missing ')'".to_owned())
        })?;
        label = Some(after_open[..end].trim().to_owned());
        remainder = after_open[end + 1..].trim_start();
    }

    while let Some(after_open) = remainder.strip_prefix('{') {
        let end = after_open.find('}').ok_or_else(|| {
            CompilerError::invalid_source("choice condition is missing '}'".to_owned())
        })?;
        conditions.push(super::inline::parse_condition(after_open[..end].trim())?);
        remainder = after_open[end + 1..].trim_start();
    }

    Ok((label, conditions, remainder))
}

pub fn parse_choice_text(input: &str) -> Result<ParsedChoiceText, CompilerError> {
    let trimmed = input.trim();
    // `\ ` at the start of choice content is a whitespace-suppression escape —
    // strip the backslash but keep what follows (may start with a space that itself gets trimmed).
    let trimmed = trimmed
        .strip_prefix("\\ ")
        .map(str::trim_start)
        .unwrap_or(trimmed);

    if let Some(target) = trimmed.strip_prefix("->") {
        let inline_target = if target.trim().is_empty() {
            None
        } else {
            Some(parse_divert(target.trim())?)
        };
        return Ok(ParsedChoiceText {
            display_text: String::new(),
            selected_text: None,
            start_text: String::new(),
            choice_only_text: String::new(),
            has_start_content: false,
            has_choice_only_content: false,
            inline_target,
            start_tags: Vec::new(),
            choice_only_tags: Vec::new(),
            selected_tags: Vec::new(),
        });
    }

    if let Some(choice_only) = trimmed.strip_prefix('[') {
        let end = choice_only.find(']').ok_or_else(|| {
            CompilerError::invalid_source("choice label is missing closing ']'".to_owned())
        })?;
        let label = choice_only[..end].trim().to_owned();
        let after_label = choice_only[end + 1..].trim_start();
        let (choice_only_text, choice_only_tags) = split_text_and_tags(&label)?;
        if after_label.is_empty() || after_label.starts_with("->") {
            let inline_target = after_label
                .strip_prefix("->")
                .map(str::trim)
                .map(parse_divert)
                .transpose()?;
            return Ok(ParsedChoiceText {
                display_text: choice_only_text.clone(),
                selected_text: None,
                start_text: String::new(),
                choice_only_text,
                has_start_content: false,
                has_choice_only_content: true,
                inline_target,
                start_tags: Vec::new(),
                choice_only_tags,
                selected_tags: Vec::new(),
            });
        }

        let (selected_text, inline_target) = split_inline_choice_divert(after_label)?;
        let (selected_text, selected_tags) = split_text_and_tags(selected_text)?;
        return Ok(ParsedChoiceText {
            display_text: choice_only_text.clone(),
            selected_text: Some(selected_text),
            start_text: String::new(),
            choice_only_text,
            has_start_content: false,
            has_choice_only_content: true,
            inline_target,
            start_tags: Vec::new(),
            choice_only_tags,
            selected_tags,
        });
    }

    if let Some((before, after)) = trimmed.split_once("[]") {
        let display = before.trim_end().to_owned();
        let raw_suffix = after.trim_start();
        let had_space_before_inline_divert = split_inline_divert(raw_suffix)
            .and_then(|(text, _)| text.chars().last())
            .is_some_and(char::is_whitespace);
        let (suffix, inline_target) = split_inline_choice_divert(raw_suffix)?;
        let suffix = if inline_target.is_some() && had_space_before_inline_divert {
            format!("{suffix} ")
        } else {
            suffix.to_owned()
        };
        let (start_text, start_tags) = split_text_and_tags(&display)?;
        let selected = if suffix.is_empty() {
            Some(display.clone())
        } else if suffix.starts_with(|c: char| c.is_ascii_punctuation() && c != '"' && c != '\'') {
            Some(format!("{display}{suffix}"))
        } else {
            Some(format!("{display} {suffix}"))
        };
        let (selected_text, selected_tags) =
            split_text_and_tags(selected.as_deref().unwrap_or(""))?;
        return Ok(ParsedChoiceText {
            display_text: start_text.clone(),
            selected_text: Some(selected_text),
            start_text,
            choice_only_text: String::new(),
            has_start_content: true,
            has_choice_only_content: false,
            inline_target,
            start_tags,
            choice_only_tags: Vec::new(),
            selected_tags,
        });
    }

    if let Some(open) = trimmed.find('[')
        && let Some(close_rel) = trimmed[open + 1..].find(']')
    {
        let close = open + 1 + close_rel;
        let start = &trimmed[..open];
        let choice_only = trimmed[open + 1..close].trim();
        let end = trimmed[close + 1..].trim_start();
        let (end, inline_target) = split_inline_choice_divert(end)?;
        let (start_text, start_tags) = split_text_and_tags(start)?;
        let (choice_only_text, choice_only_tags) = split_text_and_tags(choice_only)?;
        let (end_text, end_tags) = split_text_and_tags(end)?;
        // Append closing punctuation from `end` to choice_only_text only when the
        // `end` segment is plain text that starts with closing punctuation (like `."` or `,'`).
        // Do NOT pull chars from an expression like `{foo}`.
        let display_suffix: String = if !end.trim_start().starts_with('{') {
            end_text
                .chars()
                .take_while(|c| c.is_ascii_punctuation())
                .filter(|c| matches!(c, '\'' | '"' | ')' | ']'))
                .collect()
        } else {
            String::new()
        };
        let choice_only_text = format!("{choice_only_text}{display_suffix}");
        let display = format!("{start_text}{choice_only_text}");
        let selected_text = if end_text.is_empty() {
            start_text.trim_end().to_owned()
        } else if start_text.trim().is_empty() {
            end_text
        } else if end_text
            .starts_with(|c: char| c.is_ascii_punctuation() && c != '"' && c != '\'' && c != '{')
        {
            format!("{}{}", start_text.trim_end(), end_text)
        } else {
            format!("{} {}", start_text.trim_end(), end_text)
        };
        let mut selected_tags = start_tags.clone();
        selected_tags.extend(end_tags);
        return Ok(ParsedChoiceText {
            display_text: display,
            selected_text: Some(selected_text),
            start_text,
            choice_only_text,
            has_start_content: !start.trim().is_empty(),
            has_choice_only_content: true,
            inline_target,
            start_tags,
            choice_only_tags,
            selected_tags,
        });
    }

    let (trimmed, inline_target) = split_inline_choice_divert(trimmed)?;
    let (start_text, start_tags) = split_text_and_tags(trimmed)?;
    Ok(ParsedChoiceText {
        display_text: start_text.clone(),
        selected_text: if start_text.is_empty() {
            None
        } else {
            Some(start_text.clone())
        },
        start_text: start_text.clone(),
        choice_only_text: String::new(),
        has_start_content: !start_text.is_empty(),
        has_choice_only_content: false,
        inline_target,
        start_tags,
        choice_only_tags: Vec::new(),
        selected_tags: Vec::new(),
    })
}
