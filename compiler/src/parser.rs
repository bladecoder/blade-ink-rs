use crate::{
    error::CompilerError,
    parsed_hierarchy::{
        AssignMode, BinaryOperator, Choice, Condition, Divert, DynamicString, DynamicStringPart,
        Expression, Flow, GlobalVariable, Node, ParsedStory, Sequence, SequenceMode,
    },
};

struct Line<'a> {
    content: &'a str,
    indent: usize,
    had_newline: bool,
}

enum Header {
    Knot {
        name: String,
        parameters: Vec<String>,
    },
    Function {
        name: String,
        parameters: Vec<String>,
    },
    Stitch {
        name: String,
    },
}

#[derive(Default)]
struct FlowBuilder {
    name: String,
    parameters: Vec<String>,
    nodes: Vec<Node>,
    children: Vec<Flow>,
}

pub struct Parser<'a> {
    source: &'a str,
}

enum ParsedStatement {
    Global(GlobalVariable),
    Nodes(Vec<Node>),
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source }
    }

    pub fn parse(&self) -> Result<ParsedStory, CompilerError> {
        if self.source.is_empty() {
            return Err(CompilerError::InvalidSource(
                "ink source is empty; expected at least one line of text".to_owned(),
            ));
        }

        let normalized = self.source.replace("\r\n", "\n");
        let lines = split_lines(&normalized);

        if lines.is_empty() {
            return Err(CompilerError::InvalidSource(
                "ink source is empty; expected at least one line of text".to_owned(),
            ));
        }

        let mut globals = Vec::new();
        let mut root = Vec::new();
        let mut flows = Vec::new();
        let mut current_flow: Option<FlowBuilder> = None;
        let mut current_stitch: Option<FlowBuilder> = None;
        let mut line_index = 0;

        while line_index < lines.len() {
            if let Some(header) = parse_header(lines[line_index].content) {
                match header {
                    Header::Knot { name, parameters } | Header::Function { name, parameters } => {
                        finalize_stitch(&mut current_flow, &mut current_stitch)?;
                        if let Some(flow) = current_flow.take() {
                            flows.push(flow.build());
                        }

                        current_flow = Some(FlowBuilder {
                            name,
                            parameters,
                            nodes: Vec::new(),
                            children: Vec::new(),
                        });
                    }
                    Header::Stitch { name } => {
                        finalize_stitch(&mut current_flow, &mut current_stitch)?;
                        if current_flow.is_none() {
                            return Err(CompilerError::InvalidSource(
                                "stitch declared outside of a knot".to_owned(),
                            ));
                        }
                        current_stitch = Some(FlowBuilder {
                            name,
                            parameters: Vec::new(),
                            nodes: Vec::new(),
                            children: Vec::new(),
                        });
                    }
                }

                line_index += 1;
                continue;
            }

            let statement = parse_statement(&lines, &mut line_index, false)?;
            match statement {
                ParsedStatement::Global(global) => globals.push(global),
                ParsedStatement::Nodes(mut nodes) => {
                    target_nodes(&mut root, current_flow.as_mut(), current_stitch.as_mut())
                        .append(&mut nodes)
                }
            }
        }

        finalize_stitch(&mut current_flow, &mut current_stitch)?;
        if let Some(flow) = current_flow.take() {
            flows.push(flow.build());
        }

        Ok(ParsedStory::new(globals, root, flows))
    }
}

impl FlowBuilder {
    fn build(self) -> Flow {
        Flow {
            name: self.name,
            parameters: self.parameters,
            nodes: self.nodes,
            children: self.children,
        }
    }
}

fn finalize_stitch(
    current_flow: &mut Option<FlowBuilder>,
    current_stitch: &mut Option<FlowBuilder>,
) -> Result<(), CompilerError> {
    if let Some(stitch) = current_stitch.take() {
        let flow = current_flow.as_mut().ok_or_else(|| {
            CompilerError::InvalidSource("stitch declared without enclosing knot".to_owned())
        })?;
        flow.children.push(stitch.build());
    }

    Ok(())
}

fn target_nodes<'a>(
    root: &'a mut Vec<Node>,
    current_flow: Option<&'a mut FlowBuilder>,
    current_stitch: Option<&'a mut FlowBuilder>,
) -> &'a mut Vec<Node> {
    if let Some(stitch) = current_stitch {
        &mut stitch.nodes
    } else if let Some(flow) = current_flow {
        &mut flow.nodes
    } else {
        root
    }
}

fn split_lines(source: &str) -> Vec<Line<'_>> {
    let mut lines: Vec<Line<'_>> = source
        .split_inclusive('\n')
        .map(|line| {
            let content = line.strip_suffix('\n').unwrap_or(line);
            Line {
                content,
                indent: content
                    .chars()
                    .take_while(|ch| matches!(ch, ' ' | '\t'))
                    .count(),
                had_newline: line.ends_with('\n'),
            }
        })
        .collect();

    if !source.ends_with('\n') {
        if let Some(last_line) = lines.last_mut() {
            if !last_line.content.is_empty() {
                last_line.had_newline = true;
            }
        }
    }

    lines
}

fn parse_statement(
    lines: &[Line<'_>],
    line_index: &mut usize,
    strip_leading_whitespace: bool,
) -> Result<ParsedStatement, CompilerError> {
    let line = &lines[*line_index];
    let trimmed = line.content.trim();

    if trimmed.is_empty() {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(Vec::new()));
    }

    if trimmed == "{" {
        return parse_conditional(lines, line_index, strip_leading_whitespace)
            .map(ParsedStatement::Nodes);
    }

    if looks_like_sequence(trimmed) {
        return parse_sequence(lines, line_index, strip_leading_whitespace)
            .map(ParsedStatement::Nodes);
    }

    if looks_like_conditional(trimmed) {
        return parse_conditional(lines, line_index, strip_leading_whitespace)
            .map(ParsedStatement::Nodes);
    }

    if let Some(rest) = trimmed.strip_prefix("VAR ") {
        *line_index += 1;
        return Ok(ParsedStatement::Global(parse_global_assignment(rest)?));
    }

    if let Some(rest) = trimmed.strip_prefix("~ return ") {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(vec![Node::ReturnBool(parse_bool(
            rest.trim(),
        )?)]));
    }

    if let Some(rest) = trimmed.strip_prefix("~ ") {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(vec![parse_assignment(rest)?]));
    }

    if let Some(rest) = trimmed.strip_prefix('#') {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(vec![Node::Tag(
            parse_dynamic_string(rest.trim_start())?,
        )]));
    }

    if trimmed.starts_with('*') || trimmed.starts_with('+') {
        return parse_choice(lines, line_index);
    }

    if !trimmed.starts_with("->") {
        if let Some(gather_content) = trimmed.strip_prefix('-') {
            *line_index += 1;
            if gather_content.trim().is_empty() {
                return Ok(ParsedStatement::Nodes(Vec::new()));
            }
            let gather_line = Line {
                content: gather_content.trim_start(),
                indent: line.indent,
                had_newline: line.had_newline,
            };
            return parse_content_line(&gather_line, true).map(ParsedStatement::Nodes);
        }
    }

    if trimmed == "->->" {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(vec![Node::TunnelReturn]));
    }

    if trimmed.starts_with("->") {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(parse_divert_line(trimmed)?));
    }

    *line_index += 1;
    parse_content_line(line, strip_leading_whitespace).map(ParsedStatement::Nodes)
}

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
        nodes.push(Node::Newline);
    }

    Ok(nodes)
}

fn parse_choice(
    lines: &[Line<'_>],
    line_index: &mut usize,
) -> Result<ParsedStatement, CompilerError> {
    let line = &lines[*line_index];
    let trimmed_start = line.content.trim_start();
    let marker = trimmed_start
        .chars()
        .next()
        .ok_or_else(|| CompilerError::InvalidSource("expected choice marker".to_owned()))?;
    let once_only = marker == '*';
    let remainder = trimmed_start[marker.len_utf8()..].trim_start();
    let (label, conditions, remainder) = parse_choice_prefixes(remainder)?;
    let choice_text = parse_choice_text(remainder)?;
    let is_invisible_default =
        choice_text.display_text.is_empty() && choice_text.selected_text.is_none();

    *line_index += 1;

    let choice_indent = line.indent;
    let mut body = Vec::new();

    if let Some(divert) = choice_text.inline_target.clone() {
        body.push(Node::Divert(divert));
    }

    while *line_index < lines.len() {
        if parse_header(lines[*line_index].content).is_some()
            || lines[*line_index].indent <= choice_indent
        {
            break;
        }

        let statement = parse_statement(lines, line_index, true)?;
        match statement {
            ParsedStatement::Global(_) => {
                return Err(CompilerError::UnsupportedFeature(
                    "global declarations are not supported inside choice bodies".to_owned(),
                ))
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
    })]))
}

fn parse_choice_prefixes(
    input: &str,
) -> Result<(Option<String>, Vec<Condition>, &str), CompilerError> {
    let mut remainder = input.trim_start();
    let mut label = None;
    let mut conditions = Vec::new();

    if let Some(after_open) = remainder.strip_prefix('(') {
        let end = after_open.find(')').ok_or_else(|| {
            CompilerError::InvalidSource("choice label is missing ')'".to_owned())
        })?;
        label = Some(after_open[..end].trim().to_owned());
        remainder = after_open[end + 1..].trim_start();
    }

    while let Some(after_open) = remainder.strip_prefix('{') {
        let end = after_open.find('}').ok_or_else(|| {
            CompilerError::InvalidSource("choice condition is missing '}'".to_owned())
        })?;
        conditions.push(parse_condition(after_open[..end].trim())?);
        remainder = after_open[end + 1..].trim_start();
    }

    Ok((label, conditions, remainder))
}

struct ParsedChoiceText {
    display_text: String,
    selected_text: Option<String>,
    start_text: String,
    choice_only_text: String,
    has_start_content: bool,
    has_choice_only_content: bool,
    inline_target: Option<Divert>,
    start_tags: Vec<DynamicString>,
    choice_only_tags: Vec<DynamicString>,
    selected_tags: Vec<DynamicString>,
}

fn parse_choice_text(input: &str) -> Result<ParsedChoiceText, CompilerError> {
    let trimmed = input.trim();

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
            CompilerError::InvalidSource("choice label is missing closing ']'".to_owned())
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
        let suffix = after.trim_start();
        let (suffix, inline_target) = split_inline_choice_divert(suffix)?;
        let (start_text, start_tags) = split_text_and_tags(&display)?;
        let selected = if suffix.is_empty() {
            Some(display.clone())
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

    if let Some(open) = trimmed.find('[') {
        if let Some(close_rel) = trimmed[open + 1..].find(']') {
            let close = open + 1 + close_rel;
            let start = &trimmed[..open];
            let choice_only = trimmed[open + 1..close].trim();
            let end = trimmed[close + 1..].trim_start();
            let (end, inline_target) = split_inline_choice_divert(end)?;
            let display = format!("{start}{choice_only}");
            let (start_text, start_tags) = split_text_and_tags(start)?;
            let (choice_only_text, choice_only_tags) = split_text_and_tags(choice_only)?;
            let (end_text, end_tags) = split_text_and_tags(end)?;
            let selected_text = if end_text.is_empty() {
                start_text.trim_end().to_owned()
            } else if start_text.trim().is_empty() {
                end_text
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
    }

    let (trimmed, inline_target) = split_inline_choice_divert(trimmed)?;
    let (start_text, start_tags) = split_text_and_tags(trimmed)?;
    Ok(ParsedChoiceText {
        display_text: start_text.clone(),
        selected_text: Some(start_text.clone()),
        start_text,
        choice_only_text: String::new(),
        has_start_content: true,
        has_choice_only_content: false,
        inline_target,
        start_tags,
        choice_only_tags: Vec::new(),
        selected_tags: Vec::new(),
    })
}

fn split_inline_choice_divert(input: &str) -> Result<(&str, Option<Divert>), CompilerError> {
    if let Some((text, divert_part)) = split_inline_divert(input) {
        return Ok((text.trim_end(), Some(parse_divert(divert_part)?)));
    }

    Ok((input, None))
}

fn split_text_and_tags(input: &str) -> Result<(String, Vec<DynamicString>), CompilerError> {
    if let Some((text, tag_text)) = input.split_once('#') {
        return Ok((
            text.to_owned(),
            vec![parse_dynamic_string(tag_text.trim_start())?],
        ));
    }

    Ok((input.to_owned(), Vec::new()))
}

fn parse_dynamic_string(input: &str) -> Result<DynamicString, CompilerError> {
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

fn parse_conditional(
    lines: &[Line<'_>],
    line_index: &mut usize,
    strip_leading_whitespace: bool,
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
        return parse_multi_branch_conditional(lines, line_index);
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

        let statement = parse_statement(lines, line_index, true)?;
        let target = if in_else {
            &mut when_false
        } else {
            &mut when_true
        };
        match statement {
            ParsedStatement::Global(_) => {
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

fn parse_multi_branch_conditional(
    lines: &[Line<'_>],
    line_index: &mut usize,
) -> Result<Vec<Node>, CompilerError> {
    *line_index += 1;

    let mut branches: Vec<(Option<Condition>, Vec<Node>)> = Vec::new();
    let mut current_condition: Option<Condition> = None;
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

        let statement = parse_statement(lines, line_index, true)?;
        match statement {
            ParsedStatement::Global(_) => {
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

fn fold_conditional_branches(
    mut branches: Vec<(Option<Condition>, Vec<Node>)>,
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

fn parse_sequence(
    lines: &[Line<'_>],
    line_index: &mut usize,
    strip_leading_whitespace: bool,
) -> Result<Vec<Node>, CompilerError> {
    let line = &lines[*line_index];
    let content = if strip_leading_whitespace {
        line.content.trim_start()
    } else {
        line.content.trim()
    };

    if content == "{" {
        return parse_multi_branch_sequence(lines, line_index);
    }

    let header = content
        .strip_prefix('{')
        .ok_or_else(|| CompilerError::InvalidSource("expected sequence block".to_owned()))?;
    let (mode_text, _) = header
        .split_once(':')
        .ok_or_else(|| CompilerError::InvalidSource("sequence block is missing ':'".to_owned()))?;
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

        let branch_text = trimmed.strip_prefix('-').ok_or_else(|| {
            CompilerError::InvalidSource("sequence branch must start with '-'".to_owned())
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
                || parse_header(next_line.content).is_some()
            {
                break;
            }

            let statement = parse_statement(lines, line_index, true)?;
            match statement {
                ParsedStatement::Global(_) => {
                    return Err(CompilerError::UnsupportedFeature(
                        "global declarations are not supported inside sequences".to_owned(),
                    ));
                }
                ParsedStatement::Nodes(mut nodes) => branch_nodes.append(&mut nodes),
            }
        }

        branches.push(branch_nodes);
    }

    Err(CompilerError::InvalidSource(
        "unterminated sequence block".to_owned(),
    ))
}

fn parse_multi_branch_sequence(
    lines: &[Line<'_>],
    line_index: &mut usize,
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

        let header = trimmed.strip_prefix('-').ok_or_else(|| {
            CompilerError::InvalidSource("sequence branch must start with '-'".to_owned())
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
                || parse_header(next_line.content).is_some()
            {
                break;
            }

            let statement = parse_statement(lines, line_index, true)?;
            match statement {
                ParsedStatement::Global(_) => {
                    return Err(CompilerError::UnsupportedFeature(
                        "global declarations are not supported inside sequences".to_owned(),
                    ));
                }
                ParsedStatement::Nodes(mut nodes) => branch_nodes.append(&mut nodes),
            }
        }

        branches.push(branch_nodes);
    }

    Err(CompilerError::InvalidSource(
        "unterminated sequence block".to_owned(),
    ))
}

fn is_sequence_branch_header(trimmed: &str) -> bool {
    trimmed.starts_with('-') && !trimmed.starts_with("->")
}

fn parse_inline_sequence(content: &str) -> Result<Option<Sequence>, CompilerError> {
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

fn tokenize_inline_content(content: &str) -> Result<Vec<Node>, CompilerError> {
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

fn split_inline_divert(content: &str) -> Option<(&str, &str)> {
    let index = content.rfind("->")?;
    let (text, divert) = content.split_at(index);
    let divert = divert.strip_prefix("->")?.trim();
    if divert.is_empty() {
        None
    } else {
        Some((text, divert))
    }
}

fn find_matching_brace(content: &str, start: usize) -> Option<usize> {
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

fn looks_like_sequence(content: &str) -> bool {
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

fn looks_like_conditional(content: &str) -> bool {
    content.starts_with('{') && content.contains(':')
}

fn parse_sequence_mode(input: &str) -> Result<SequenceMode, CompilerError> {
    match input {
        "stopping" => Ok(SequenceMode::Stopping),
        "once" => Ok(SequenceMode::Once),
        "cycle" => Ok(SequenceMode::Cycle),
        "shuffle" => Ok(SequenceMode::Shuffle),
        "shuffle once" => Ok(SequenceMode::ShuffleOnce),
        "stopping shuffle" => Ok(SequenceMode::ShuffleStopping),
        _ => Err(CompilerError::UnsupportedFeature(format!(
            "unsupported sequence mode '{input}'"
        ))),
    }
}

fn split_top_level_pipe(input: &str) -> Vec<&str> {
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

fn parse_inline_conditional(content: &str) -> Result<Option<(Condition, &str)>, CompilerError> {
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

fn parse_condition(condition: &str) -> Result<Condition, CompilerError> {
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

fn parse_global_assignment(input: &str) -> Result<GlobalVariable, CompilerError> {
    let (name, expression) = split_assignment(input, "=")?;
    Ok(GlobalVariable {
        name,
        initial_value: parse_expression(&expression)?,
    })
}

fn parse_assignment(input: &str) -> Result<Node, CompilerError> {
    if input.contains("+=") {
        let (name, expression) = split_assignment(input, "+=")?;
        return Ok(Node::Assignment {
            variable_name: name,
            expression: parse_expression(&expression)?,
            mode: AssignMode::AddAssign,
        });
    }

    let (name, expression) = split_assignment(input, "=")?;
    Ok(Node::Assignment {
        variable_name: name,
        expression: parse_expression(&expression)?,
        mode: AssignMode::Set,
    })
}

fn split_assignment(input: &str, separator: &str) -> Result<(String, String), CompilerError> {
    let (name, expression) = input.split_once(separator).ok_or_else(|| {
        CompilerError::InvalidSource(format!("expected assignment using '{separator}'"))
    })?;

    Ok((name.trim().to_owned(), expression.trim().to_owned()))
}

fn parse_divert(input: &str) -> Result<Divert, CompilerError> {
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

fn parse_divert_line(input: &str) -> Result<Vec<Node>, CompilerError> {
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

fn parse_expression(input: &str) -> Result<Expression, CompilerError> {
    let tokens = tokenize_expression(input)?;
    let mut parser = ExpressionParser::new(tokens);
    let expression = parser.parse_expression()?;

    if !parser.is_at_end() {
        return Err(CompilerError::UnsupportedFeature(format!(
            "unexpected token in expression '{}'",
            input.trim()
        )));
    }

    Ok(expression)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Bool(bool),
    Int(i32),
    Float(f32),
    Str(String),
    Ident(String),
    DivertTarget(String),
    Plus,
    Minus,
    Star,
    EqualEqual,
    Greater,
    AndAnd,
    LeftParen,
    RightParen,
}

fn tokenize_expression(input: &str) -> Result<Vec<Token>, CompilerError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }

        if ch == '-' && chars.get(index + 1) == Some(&'>') {
            let target = input[index + 2..].trim();
            let parsed = parse_path_identifier(target).ok_or_else(|| {
                CompilerError::InvalidSource("expected divert target after '->'".to_owned())
            })?;
            tokens.push(Token::DivertTarget(parsed.to_owned()));
            break;
        }

        match ch {
            '+' => {
                tokens.push(Token::Plus);
                index += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                index += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                index += 1;
            }
            '&' if chars.get(index + 1) == Some(&'&') => {
                tokens.push(Token::AndAnd);
                index += 2;
            }
            '=' if chars.get(index + 1) == Some(&'=') => {
                tokens.push(Token::EqualEqual);
                index += 2;
            }
            '>' => {
                tokens.push(Token::Greater);
                index += 1;
            }
            '(' => {
                tokens.push(Token::LeftParen);
                index += 1;
            }
            ')' => {
                tokens.push(Token::RightParen);
                index += 1;
            }
            '"' => {
                let mut end = index + 1;
                while end < chars.len() && chars[end] != '"' {
                    end += 1;
                }
                if end >= chars.len() {
                    return Err(CompilerError::InvalidSource(
                        "unterminated string literal".to_owned(),
                    ));
                }
                tokens.push(Token::Str(chars[index + 1..end].iter().collect()));
                index = end + 1;
            }
            '0'..='9' => {
                let start = index;
                index += 1;
                while index < chars.len() && chars[index].is_ascii_digit() {
                    index += 1;
                }
                if index < chars.len() && chars[index] == '.' {
                    index += 1;
                    while index < chars.len() && chars[index].is_ascii_digit() {
                        index += 1;
                    }
                    let value = input[start..index].parse::<f32>().map_err(|error| {
                        CompilerError::InvalidSource(format!("invalid float literal: {error}"))
                    })?;
                    tokens.push(Token::Float(value));
                } else {
                    let value = input[start..index].parse::<i32>().map_err(|error| {
                        CompilerError::InvalidSource(format!("invalid integer literal: {error}"))
                    })?;
                    tokens.push(Token::Int(value));
                }
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let start = index;
                index += 1;
                while index < chars.len()
                    && matches!(chars[index], 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '.')
                {
                    index += 1;
                }
                let ident = &input[start..index];
                match ident {
                    "true" => tokens.push(Token::Bool(true)),
                    "false" => tokens.push(Token::Bool(false)),
                    "and" => tokens.push(Token::AndAnd),
                    _ => tokens.push(Token::Ident(ident.to_owned())),
                }
            }
            _ => {
                return Err(CompilerError::UnsupportedFeature(format!(
                    "unsupported token '{}' in expression",
                    ch
                )))
            }
        }
    }

    Ok(tokens)
}

struct ExpressionParser {
    tokens: Vec<Token>,
    current: usize,
}

impl ExpressionParser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, current: 0 }
    }

    fn parse_expression(&mut self) -> Result<Expression, CompilerError> {
        self.parse_and()
    }

    fn parse_and(&mut self) -> Result<Expression, CompilerError> {
        let mut expression = self.parse_equality()?;

        while self.match_token(&Token::AndAnd) {
            let right = self.parse_equality()?;
            expression = Expression::Binary {
                left: Box::new(expression),
                operator: BinaryOperator::And,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.tokens.len()
    }

    fn parse_equality(&mut self) -> Result<Expression, CompilerError> {
        let mut expression = self.parse_comparison()?;

        while self.match_token(&Token::EqualEqual) {
            let right = self.parse_comparison()?;
            expression = Expression::Binary {
                left: Box::new(expression),
                operator: BinaryOperator::Equal,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_comparison(&mut self) -> Result<Expression, CompilerError> {
        let mut expression = self.parse_addition()?;

        while self.match_token(&Token::Greater) {
            let right = self.parse_addition()?;
            expression = Expression::Binary {
                left: Box::new(expression),
                operator: BinaryOperator::Greater,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_addition(&mut self) -> Result<Expression, CompilerError> {
        let mut expression = self.parse_multiplication()?;

        while let Some(operator) = self.match_additive_operator() {
            let right = self.parse_multiplication()?;
            expression = Expression::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_multiplication(&mut self) -> Result<Expression, CompilerError> {
        let mut expression = self.parse_primary()?;

        while self.match_token(&Token::Star) {
            let right = self.parse_primary()?;
            expression = Expression::Binary {
                left: Box::new(expression),
                operator: BinaryOperator::Multiply,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<Expression, CompilerError> {
        let token = self.advance().ok_or_else(|| {
            CompilerError::InvalidSource("expected expression but found end of input".to_owned())
        })?;

        match token {
            Token::Bool(value) => Ok(Expression::Bool(value)),
            Token::Int(value) => Ok(Expression::Int(value)),
            Token::Float(value) => Ok(Expression::Float(value)),
            Token::Str(value) => Ok(Expression::Str(value)),
            Token::Ident(name) => Ok(Expression::Variable(name)),
            Token::DivertTarget(target) => Ok(Expression::DivertTarget(target)),
            Token::Minus => {
                let expression = self.parse_primary()?;
                Ok(Expression::Binary {
                    left: Box::new(Expression::Int(0)),
                    operator: BinaryOperator::Subtract,
                    right: Box::new(expression),
                })
            }
            Token::LeftParen => {
                let expression = self.parse_expression()?;
                if !self.match_token(&Token::RightParen) {
                    return Err(CompilerError::InvalidSource(
                        "missing ')' in expression".to_owned(),
                    ));
                }
                Ok(expression)
            }
            _ => Err(CompilerError::UnsupportedFeature(
                "unsupported expression form".to_owned(),
            )),
        }
    }

    fn match_additive_operator(&mut self) -> Option<BinaryOperator> {
        if self.match_token(&Token::Plus) {
            Some(BinaryOperator::Add)
        } else if self.match_token(&Token::Minus) {
            Some(BinaryOperator::Subtract)
        } else {
            None
        }
    }

    fn match_token(&mut self, token: &Token) -> bool {
        if self.peek() == Some(token) {
            self.current += 1;
            true
        } else {
            false
        }
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.current).cloned();
        if token.is_some() {
            self.current += 1;
        }
        token
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.current)
    }
}

fn parse_bool(value: &str) -> Result<bool, CompilerError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(CompilerError::UnsupportedFeature(format!(
            "unsupported boolean literal '{value}'"
        ))),
    }
}

fn parse_header(line: &str) -> Option<Header> {
    let trimmed = line.trim();

    if trimmed.starts_with("===") || trimmed.starts_with("==") {
        let inner = trimmed.trim_matches('=').trim();
        if let Some(rest) = inner.strip_prefix("function") {
            let (name, parameters) = parse_header_signature(rest.trim())?;
            return Some(Header::Function { name, parameters });
        }

        let (name, parameters) = parse_header_signature(inner)?;
        return Some(Header::Knot { name, parameters });
    }

    if trimmed.starts_with('=') {
        let inner = trimmed.trim_start_matches('=').trim();
        let name = parse_path_identifier(inner)?;
        return Some(Header::Stitch {
            name: name.to_owned(),
        });
    }

    None
}

fn parse_header_signature(text: &str) -> Option<(String, Vec<String>)> {
    let open = text.find('(');
    let close = text.rfind(')');

    match (open, close) {
        (Some(open), Some(close)) if close > open => {
            let name = parse_path_identifier(text[..open].trim())?.to_owned();
            let parameters = split_top_level_commas(&text[open + 1..close])
                .into_iter()
                .filter(|part| !part.trim().is_empty())
                .map(|part| part.trim().to_owned())
                .collect();
            Some((name, parameters))
        }
        _ => Some((parse_path_identifier(text)?.to_owned(), Vec::new())),
    }
}

fn parse_call_like(text: &str) -> Result<Option<(String, Vec<Expression>)>, CompilerError> {
    let trimmed = text.trim();
    let open = match trimmed.find('(') {
        Some(index) => index,
        None => return Ok(None),
    };
    let close = trimmed
        .rfind(')')
        .ok_or_else(|| CompilerError::InvalidSource("missing ')' in divert target".to_owned()))?;
    if close < open {
        return Err(CompilerError::InvalidSource(
            "invalid call-like syntax".to_owned(),
        ));
    }

    let name = parse_path_identifier(trimmed[..open].trim())
        .ok_or_else(|| CompilerError::InvalidSource("invalid divert target".to_owned()))?
        .to_owned();
    let mut arguments = Vec::new();
    for argument in split_top_level_commas(&trimmed[open + 1..close]) {
        if !argument.trim().is_empty() {
            arguments.push(parse_expression(argument.trim())?);
        }
    }

    Ok(Some((name, arguments)))
}

fn split_top_level_commas(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    let mut in_string = false;
    let chars: Vec<char> = input.chars().collect();

    for (index, ch) in chars.iter().enumerate() {
        match ch {
            '"' => in_string = !in_string,
            '(' if !in_string => depth += 1,
            ')' if !in_string => depth -= 1,
            ',' if !in_string && depth == 0 => {
                parts.push(input[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }

    parts.push(input[start..].trim());
    parts
}

fn parse_path_identifier(text: &str) -> Option<&str> {
    let end = text
        .char_indices()
        .find(|(_, ch)| !matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '.'))
        .map(|(index, _)| index)
        .unwrap_or(text.len());

    if end == 0 {
        None
    } else {
        Some(&text[..end])
    }
}
