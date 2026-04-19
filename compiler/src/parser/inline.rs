use crate::{
    ast::{Divert, DynamicString, DynamicStringPart, Expression, Node, Sequence, SequenceMode},
    error::CompilerError,
};

use super::expression::{parse_call_like, parse_expression};

pub fn tokenize_inline_content(content: &str) -> Result<Vec<Node>, CompilerError> {
    let mut nodes = Vec::new();
    let mut text = String::new();
    let mut chars = content.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        // \X is an escape sequence — emit the next character literally
        // e.g. \# -> '#', \| -> '|', \- -> '-', \n -> newline, etc.
        if ch == '\\' {
            if let Some((_, next_ch)) = chars.peek().copied() {
                chars.next();
                // \n inside inline content produces a newline
                if next_ch == 'n' {
                    text.push('\n');
                } else {
                    text.push(next_ch);
                }
                continue;
            }
            // trailing backslash — emit nothing (or push it; shouldn't happen)
            continue;
        }

        if ch == '#' {
            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(&mut text)));
            }
            // Multiple tags on one line: "text #tag1 #tag2 #tag3"
            let tag_str = content[index + 1..].trim_start();
            for tag in split_hash_tags(tag_str)? {
                nodes.push(Node::Tag(tag));
            }
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

        // Thread divert inline: <- target  or  <- target(args)
        if ch == '<' && content[index..].starts_with("<-") {
            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(&mut text)));
            }
            let after = &content[index + 2..]; // skip "<-"
            let trimmed_after = after.trim_start();
            let leading = after.len() - trimmed_after.len();
            let call_len = if trimmed_after.contains('(') {
                thread_divert_call_end(trimmed_after)
            } else {
                trimmed_after
                    .find(|c: char| !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '.'))
                    .unwrap_or(trimmed_after.len())
            };
            let divert_str = &trimmed_after[..call_len];
            let divert = parse_divert(divert_str)?;
            nodes.push(Node::ThreadDivert(divert));
            // Advance the iterator past the consumed "<-" + leading space + call text
            let consume_end = index + 2 + leading + call_len;
            while let Some((peek_idx, _)) = chars.peek() {
                if *peek_idx < consume_end {
                    chars.next();
                } else {
                    break;
                }
            }
            continue;
        }

        // Divert or tunnel: -> target  or  -> target ->
        if ch == '-' && content[index..].starts_with("->") {
            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(&mut text)));
            }
            let divert_str = content[index..].trim();
            let mut divert_nodes = parse_divert_line(divert_str)?;
            nodes.append(&mut divert_nodes);
            // consume the rest of the input — divert always ends the content
            break;
        }

        if ch == '{' {
            let end = find_matching_brace(content, index).ok_or_else(|| {
                CompilerError::invalid_source("unterminated inline brace expression".to_owned())
            })?;

            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(&mut text)));
            }

            let inline = &content[index..=end];
            if let Some((condition, branch_text)) = parse_inline_conditional(inline)? {
                // Split on top-level '|' to get optional false branch.
                let branches: Vec<&str> = split_top_level_pipe(branch_text);
                // Use trim_end (not trim) to preserve the leading space that authors
                // write after ':' — inklecate keeps it as part of the text token.
                let when_true = tokenize_inline_content(branches[0].trim_end())?;
                let when_false = if branches.len() > 1 {
                    Some(tokenize_inline_content(branches[1].trim_end())?)
                } else {
                    None
                };
                nodes.push(Node::Conditional {
                    condition,
                    when_true,
                    when_false,
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
                CompilerError::invalid_source("unterminated inline brace expression".to_owned())
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

    // `->-> target (args)`: tunnel return, continuing to `target` with args
    // e.g. `->-> elsewhere (8)` emits args + {"^->": target} then `->->`
    // Only applies when explicit args are provided (parenthesized).
    // `->-> escape` with no parens falls through to the regular path.
    if let Some(rest) = trimmed.strip_prefix("->->") {
        let rest = rest.trim();
        if !rest.is_empty() && !rest.starts_with("->") && rest.contains('(') {
            // Parse `target` or `target(args)` from `rest`
            let (target_name, args) = if let Some(open) = rest.find('(') {
                let close = rest.rfind(')').unwrap_or(rest.len() - 1);
                let tname = rest[..open].trim().to_owned();
                let args_str = &rest[open + 1..close];
                let mut args = Vec::new();
                for arg_str in super::expression::split_top_level_commas(args_str) {
                    if !arg_str.trim().is_empty() {
                        args.push(parse_expression(arg_str.trim())?);
                    }
                }
                (tname, args)
            } else {
                (rest.to_owned(), Vec::new())
            };
            return Ok(vec![Node::TunnelOnwardsWithTarget {
                target: target_name,
                args,
            }]);
        }
    }

    let segments = split_top_level_divert_segments(trimmed);

    if segments.is_empty() {
        return Err(CompilerError::invalid_source(
            "expected divert target after '->'".to_owned(),
        ));
    }

    // Helper to parse "target" or "target(arg1, arg2)" from a segment
    fn parse_segment(segment: &str) -> Result<(String, Vec<Expression>), CompilerError> {
        if let Some(open) = segment.find('(') {
            let close = segment.rfind(')').unwrap_or(segment.len() - 1);
            let target = segment[..open].trim().to_owned();
            let args_str = &segment[open + 1..close];
            let mut args = Vec::new();
            for arg_str in super::expression::split_top_level_commas(args_str) {
                if !arg_str.trim().is_empty() {
                    args.push(parse_expression(arg_str.trim())?);
                }
            }
            Ok((target, args))
        } else {
            Ok((segment.to_string(), Vec::new()))
        }
    }

    // `-> tunnel2 ->->`: tunnel to `tunnel2` then return from current tunnel
    if trimmed.ends_with("->->") {
        let mut tunnel_nodes = Vec::new();
        for segment in &segments {
            let (target_name, args) = parse_segment(segment)?;
            tunnel_nodes.push(Node::TunnelDivert {
                target: target_name,
                is_variable: !args.is_empty(),
                args,
            });
        }
        tunnel_nodes.push(Node::TunnelReturn);
        return Ok(tunnel_nodes);
    }

    if trimmed.ends_with("->") {
        // All segments are tunnel calls: -> a -> b ->
        let mut tunnel_nodes = Vec::new();
        for segment in &segments {
            let (target_name, args) = parse_segment(segment)?;
            tunnel_nodes.push(Node::TunnelDivert {
                target: target_name,
                is_variable: !args.is_empty(),
                args,
            });
        }
        return Ok(tunnel_nodes);
    }

    if segments.len() > 1 {
        // Multiple segments not ending in "->": all but last are tunnel calls, last is a divert
        // e.g. "-> x -> end" means tunnel-call x, then divert to end
        let mut nodes = Vec::new();
        for segment in &segments[..segments.len() - 1] {
            let (target_name, args) = parse_segment(segment)?;
            nodes.push(Node::TunnelDivert {
                target: target_name,
                is_variable: !args.is_empty(),
                args,
            });
        }
        let last = segments[segments.len() - 1];
        let (target_name, args) = parse_segment(last)?;
        if args.is_empty() {
            nodes.push(Node::Divert(parse_divert(last)?));
        } else {
            nodes.push(Node::Divert(Divert {
                target: target_name,
                arguments: args,
            }));
        }
        return Ok(nodes);
    }

    Ok(vec![Node::Divert(parse_divert(segments[0])?)])
}

fn split_top_level_divert_segments(input: &str) -> Vec<&str> {
    let bytes = input.as_bytes();
    let len = input.len();
    let mut depth = 0usize;
    let mut start: Option<usize> = None;
    let mut segments = Vec::new();
    let mut i = 0;

    while i < len {
        match bytes[i] {
            b'(' | b'{' if start.is_some() => depth += 1,
            b')' | b'}' if start.is_some() => depth = depth.saturating_sub(1),
            b'-' if i + 1 < len && bytes[i + 1] == b'>' && depth == 0 => {
                if let Some(seg_start) = start {
                    let segment = input[seg_start..i].trim();
                    if !segment.is_empty() {
                        segments.push(segment);
                    }
                }
                start = Some(i + 2);
                i += 2;
                continue;
            }
            _ => {}
        }
        i += 1;
    }

    if let Some(seg_start) = start {
        let segment = input[seg_start..].trim();
        if !segment.is_empty() {
            segments.push(segment);
        }
    }

    segments
}

pub fn split_inline_divert(content: &str) -> Option<(&str, &str)> {
    // Find the last "->" that is not inside braces/parens
    let bytes = content.as_bytes();
    let len = content.len();
    let mut depth = 0usize;
    let mut arrow_pos: Option<usize> = None;

    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'{' | b'(' => depth += 1,
            b'}' | b')' => depth = depth.saturating_sub(1),
            b'-' if depth == 0 && i + 1 < len && bytes[i + 1] == b'>' => {
                arrow_pos = Some(i);
                i += 2;
                continue;
            }
            _ => {}
        }
        i += 1;
    }

    let index = arrow_pos?;
    let (text, rest) = content.split_at(index);
    let divert = rest.strip_prefix("->")?.trim();
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
        // There may be multiple tags: `tag1 #tag2 #tag3`
        let tags = split_hash_tags(tag_text)?;
        return Ok((text.to_owned(), tags));
    }

    Ok((input.to_owned(), Vec::new()))
}

/// Split a string like `"style: robot #target: comm_scn #type:TALK"` into individual tag
/// strings `["style: robot ", "target: comm_scn ", "type:TALK"]`, then parse each one.
fn split_hash_tags(input: &str) -> Result<Vec<DynamicString>, CompilerError> {
    let mut tags = Vec::new();
    let mut rest = input;
    loop {
        // Find the next # that is not inside braces
        let mut depth = 0usize;
        let mut split_at = None;
        for (i, ch) in rest.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                '#' if depth == 0 && i > 0 => {
                    split_at = Some(i);
                    break;
                }
                _ => {}
            }
        }
        if let Some(idx) = split_at {
            tags.push(parse_dynamic_string(rest[..idx].trim_start())?);
            rest = &rest[idx + 1..];
        } else {
            tags.push(parse_dynamic_string(rest.trim_start())?);
            break;
        }
    }
    Ok(tags)
}

pub fn split_top_level_pipe(input: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let ch = input[i..].chars().next().unwrap();
        let ch_len = ch.len_utf8();
        match ch {
            '\\' => {
                // Skip escaped character (e.g. \| should not split)
                i += ch_len;
                if i < bytes.len() {
                    let next = input[i..].chars().next().unwrap();
                    i += next.len_utf8();
                }
                continue;
            }
            '{' | '(' => depth += 1,
            '}' | ')' => depth -= 1,
            '|' if depth == 0 => {
                result.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += ch_len;
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
        // Do NOT trim the leading whitespace from the branch text: inklecate preserves
        // the space that authors write after ':' (e.g. `{cond: text}` → `" text"`).
        branch,
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
    // Detect optional mode prefix: & = cycle, ! = once, ~ = shuffle
    let (mode, explicit_mode, rest) = if let Some(r) = content.strip_prefix('&') {
        (SequenceMode::Cycle, true, r)
    } else if let Some(r) = content.strip_prefix('!') {
        (SequenceMode::Once, true, r)
    } else if let Some(r) = content.strip_prefix('~') {
        (SequenceMode::Shuffle, true, r)
    } else {
        (SequenceMode::Stopping, false, content)
    };

    let parts = split_top_level_pipe(rest);
    // A sequence needs at least 2 branches, UNLESS an explicit mode prefix was
    // given (e.g. {! ->tunnel->}), in which case a single branch is valid.
    if parts.len() < 2 && !explicit_mode {
        return Ok(None);
    }
    if parts.is_empty() {
        return Ok(None);
    }

    let mut branches = Vec::new();
    for part in parts {
        branches.push(tokenize_inline_content(part.trim())?);
    }

    Ok(Some(Sequence { mode, branches }))
}

/// Find the byte length of a function call starting from the opening `(`,
/// handling nested parens and string literals.  Returns the byte offset
/// just past the matching `)`, or the full length of `s` if unmatched.
fn thread_divert_call_end(s: &str) -> usize {
    let mut depth = 0usize;
    let mut in_string = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_string = !in_string,
            '(' if !in_string => depth += 1,
            ')' if !in_string => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + c.len_utf8();
                }
            }
            _ => {}
        }
    }
    s.len()
}
