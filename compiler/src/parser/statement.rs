pub fn parse_statement(
    lines: &[Line<'_>],
    line_index: &mut usize,
    strip_leading_whitespace: bool,
) -> Result<ParsedStatement, CompilerError> {
    let line = &lines[*line_index];
    let trimmed = line.content.trim();
    // 1-based line number for error messages; captured before any sub-parser advances the index.
    let ln = *line_index + 1;

    if trimmed.is_empty() || trimmed.starts_with("//") {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(Vec::new()));
    }

    if trimmed == "{" {
        return parse_conditional(
            lines,
            line_index,
            strip_leading_whitespace,
            &parse_statement,
        )
        .map(ParsedStatement::Nodes)
        .map_err(|e| e.with_line(ln));
    }

    if looks_like_sequence(trimmed) {
        return parse_sequence(
            lines,
            line_index,
            strip_leading_whitespace,
            &parse_statement,
        )
        .map(ParsedStatement::Nodes)
        .map_err(|e| e.with_line(ln));
    }

    if looks_like_conditional(trimmed) && brace_spans_multiple_lines(trimmed) {
        return parse_conditional(
            lines,
            line_index,
            strip_leading_whitespace,
            &parse_statement,
        )
        .map(ParsedStatement::Nodes)
        .map_err(|e| e.with_line(ln));
    }

    if let Some(rest) = trimmed.strip_prefix("EXTERNAL ") {
        *line_index += 1;
        // Extract the function name from "funcName(params)"
        let name = rest.split('(').next().unwrap_or(rest).trim().to_owned();
        return Ok(ParsedStatement::ExternalFunction(name));
    }

    if let Some(rest) = trimmed.strip_prefix("VAR ") {
        *line_index += 1;
        return Ok(ParsedStatement::Global(
            parse_global_assignment(rest).map_err(|e| e.with_line(ln))?,
        ));
    }

    if let Some(rest) = trimmed.strip_prefix("CONST ") {
        *line_index += 1;
        return Ok(ParsedStatement::Const(
            parse_global_assignment(rest).map_err(|e| e.with_line(ln))?,
        ));
    }

    if let Some(rest) = trimmed.strip_prefix("LIST ") {
        *line_index += 1;
        // Collect continuation lines (indented) that are part of the same LIST declaration.
        let mut full = rest.to_owned();
        while *line_index < lines.len() {
            let next = lines[*line_index].content.trim();
            // Stop if line is non-empty and doesn't start with whitespace (new statement)
            if !lines[*line_index].content.starts_with([' ', '\t']) && !next.is_empty() {
                break;
            }
            // Skip blank/whitespace-only continuation lines only if we're still expecting more items
            // (accumulated text ends with a comma). This handles remnants of stripped block comments
            // that appeared between list items on separate lines.
            if next.is_empty() {
                let trimmed_full = full.trim_end();
                if trimmed_full.ends_with(',') {
                    *line_index += 1;
                    continue;
                } else {
                    break;
                }
            }
            // Stop at line-comments
            if next.starts_with("//") {
                break;
            }
            full.push(' ');
            full.push_str(next);
            *line_index += 1;
        }
        // Strip block comments /* ... */ from the collected text
        let full = strip_block_comments(&full);
        return Ok(ParsedStatement::List(
            parse_list_declaration(&full).map_err(|e| e.with_line(ln))?,
        ));
    }

    if trimmed == "~ return" || trimmed == "~return" {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(vec![Node::ReturnVoid]));
    }

    if let Some(rest) = trimmed
        .strip_prefix("~ return ")
        .or_else(|| trimmed.strip_prefix("~return "))
    {
        *line_index += 1;
        // Try as bool first (legacy), otherwise parse as expression
        let expr = match parse_bool(rest.trim()) {
            Ok(b) => Expression::Bool(b),
            Err(_) => parse_expression(rest.trim()).map_err(|e| e.with_line(ln))?,
        };
        return Ok(ParsedStatement::Nodes(vec![Node::ReturnExpr(expr)]));
    }

    if let Some(rest) = trimmed
        .strip_prefix("~ ")
        .or_else(|| trimmed.strip_prefix('~'))
    {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(vec![
            parse_assignment(rest).map_err(|e| e.with_line(ln))?,
        ]));
    }

    if let Some(rest) = trimmed.strip_prefix('#') {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(vec![Node::Tag(
            parse_dynamic_string(rest.trim_start()).map_err(|e| e.with_line(ln))?,
        )]));
    }

    if trimmed.starts_with('*') || trimmed.starts_with('+') {
        return parse_choice(lines, line_index, &parse_statement).map_err(|e| e.with_line(ln));
    }

    if !trimmed.starts_with("->") && trimmed.starts_with('-') {
        *line_index += 1;
        // Strip all leading '-' markers (nested gathers like "- -" or "- - -") — nesting
        // is handled by indentation, treated as a single gather point.
        let mut gather_content = trimmed;
        while let Some(rest) = gather_content.strip_prefix('-') {
            let next = rest.trim_start();
            if next.is_empty() || !next.starts_with('-') || next.starts_with("->") {
                gather_content = next;
                break;
            }
            gather_content = next;
        }
        if gather_content.is_empty() {
            return Ok(ParsedStatement::Nodes(vec![Node::GatherPoint]));
        }

        // Check for gather label: (label_name) at the start
        let (gather_label, gather_content) = if gather_content.starts_with('(') {
            if let Some(end) = gather_content.find(')') {
                let label = gather_content[1..end].trim().to_owned();
                let rest = gather_content[end + 1..].trim_start();
                (Some(label), rest)
            } else {
                (None, gather_content)
            }
        } else {
            (None, gather_content)
        };

        // Handle "- * choice" or "- + choice": a gather point immediately followed by a
        // choice on the same line.  *line_index has already been advanced past this line,
        // so we build a synthetic slice with the choice content as line 0, followed by
        // the remaining (not-yet-consumed) lines, and call parse_choice on it.
        if gather_content.starts_with('*') || gather_content.starts_with('+') {
            let synthetic = Line {
                content: gather_content,
                indent: line.indent,
                had_newline: line.had_newline,
            };
            let tail: Vec<Line<'_>> = lines[*line_index..]
                .iter()
                .map(|l| Line {
                    content: l.content,
                    indent: l.indent,
                    had_newline: l.had_newline,
                })
                .collect();
            let combined: Vec<Line<'_>> = std::iter::once(synthetic).chain(tail).collect();
            let mut local_idx: usize = 0;
            let choice_stmt = parse_choice(&combined, &mut local_idx, &parse_statement)
                .map_err(|e| e.with_line(ln))?;
            // local_idx lines were consumed from `combined`; combined[0] mapped to the
            // already-consumed original line, combined[1..] maps to lines[*line_index..].
            *line_index += local_idx.saturating_sub(1);
            let choice_nodes = match choice_stmt {
                ParsedStatement::Nodes(ns) => ns,
                other => return Ok(other),
            };
            let mut result = vec![Node::GatherPoint];
            if let Some(label) = gather_label {
                result.push(Node::GatherLabel(label));
            }
            result.extend(choice_nodes);
            return Ok(ParsedStatement::Nodes(result));
        }

        // If gather_content starts a multi-line conditional or sequence, parse it as such.
        // Build a synthetic slice: the first entry uses gather_content as its content, then
        // the remaining (not-yet-consumed) lines follow.  We track how many lines the
        // sub-parser consumed and advance the outer line_index accordingly.
        if looks_like_conditional(gather_content) || looks_like_sequence(gather_content) {
            // Synthetic opening line using gather_content.
            let synthetic = Line {
                content: gather_content,
                indent: line.indent,
                had_newline: line.had_newline,
            };
            // Build the combined slice: [synthetic] ++ lines[*line_index..]
            let combined: Vec<Line<'_>> = std::iter::once(synthetic)
                .chain(lines[*line_index..].iter().map(|l| Line {
                    content: l.content,
                    indent: l.indent,
                    had_newline: l.had_newline,
                }))
                .collect();
            let mut local_idx: usize = 0;
            let nodes = if looks_like_sequence(gather_content) {
                parse_sequence(&combined, &mut local_idx, true, &parse_statement)
                    .map_err(|e| e.with_line(ln))?
            } else {
                parse_conditional(&combined, &mut local_idx, true, &parse_statement)
                    .map_err(|e| e.with_line(ln))?
            };
            // local_idx now points past whatever the sub-parser consumed in `combined`.
            // combined[0] was the synthetic gather line (already consumed above via += 1).
            // combined[1..] maps to lines[*line_index..], so advance by local_idx - 1.
            if local_idx > 0 {
                *line_index += local_idx - 1;
            }
            let mut result = Vec::new();
            if let Some(label) = gather_label {
                result.push(Node::GatherLabel(label));
            }
            result.extend(nodes);
            return Ok(ParsedStatement::Nodes(result));
        }

        let gather_line = Line {
            content: gather_content,
            indent: line.indent,
            // Don't emit a newline for a label-only gather line (no content after the label)
            had_newline: line.had_newline && !gather_content.is_empty(),
        };
        let mut nodes = parse_content_line(&gather_line, true).map_err(|e| e.with_line(ln))?;
        if let Some(label) = gather_label {
            nodes.insert(0, Node::GatherLabel(label));
        }
        return Ok(ParsedStatement::Nodes(nodes));
    }

    if trimmed == "->->" {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(vec![Node::TunnelReturn]));
    }

    let thread_rest = if let Some(r) = trimmed.strip_prefix("<- ") {
        Some(r.trim())
    } else {
        // Also handle "<-target(args)" without a space after "<-"
        trimmed
            .strip_prefix("<-")
            .filter(|&r| r.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
    };
    if let Some(rest) = thread_rest {
        *line_index += 1;
        let divert = parse_divert(rest).map_err(|e| e.with_line(ln))?;
        return Ok(ParsedStatement::Nodes(vec![Node::ThreadDivert(divert)]));
    }

    if trimmed.starts_with("->") {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(
            parse_divert_line(trimmed).map_err(|e| e.with_line(ln))?,
        ));
    }

    *line_index += 1;
    parse_content_line(line, strip_leading_whitespace)
        .map(ParsedStatement::Nodes)
        .map_err(|e| e.with_line(ln))
}

