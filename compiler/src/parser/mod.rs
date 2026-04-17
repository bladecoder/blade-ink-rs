pub mod choice;
pub mod conditional;
pub mod expression;
pub mod inline;
pub mod sequence;

use crate::{
    ast::{AssignMode, Expression, Flow, GlobalVariable, ListDeclaration, Node, ParsedStory},
    error::CompilerError,
};

use self::{
    choice::parse_choice,
    conditional::{looks_like_conditional, parse_conditional},
    expression::{parse_bool, parse_call_like, parse_expression, parse_path_identifier},
    inline::{
        parse_divert, parse_divert_line, parse_dynamic_string, split_inline_divert,
        tokenize_inline_content,
    },
    sequence::{looks_like_sequence, parse_sequence},
};

pub struct Line<'a> {
    pub content: &'a str,
    pub indent: usize,
    pub had_newline: bool,
}

pub enum Header {
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
    is_function: bool,
    is_root_stitch: bool,
    parameters: Vec<String>,
    nodes: Vec<Node>,
    children: Vec<Flow>,
}

pub struct Parser<'a> {
    source: &'a str,
}

pub enum ParsedStatement {
    Global(GlobalVariable),
    List(ListDeclaration),
    ExternalFunction(String),
    Nodes(Vec<Node>),
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source }
    }

    pub fn parse(&self) -> Result<ParsedStory, CompilerError> {
        if self.source.is_empty() {
            return Err(CompilerError::invalid_source(
                "ink source is empty; expected at least one line of text".to_owned(),
            ));
        }

        let normalized = self.source.replace("\r\n", "\n");
        let lines = split_lines(&normalized);

        if lines.is_empty() {
            return Err(CompilerError::invalid_source(
                "ink source is empty; expected at least one line of text".to_owned(),
            ));
        }

        let mut globals = Vec::new();
        let mut list_declarations = Vec::new();
        let mut external_functions = Vec::new();
        let mut root = Vec::new();
        let mut flows = Vec::new();
        let mut current_flow: Option<FlowBuilder> = None;
        let mut current_stitch: Option<FlowBuilder> = None;
        let mut line_index = 0;

        while line_index < lines.len() {
            let ln = line_index + 1;
            if let Some(header) = parse_header(lines[line_index].content) {
                match header {
                    Header::Knot { name, parameters } => {
                        finalize_stitch(&mut current_flow, &mut current_stitch)
                            .map_err(|e| e.with_line(ln))?;
                        if let Some(flow) = current_flow.take() {
                            flows.push(flow.build());
                        }

                        current_flow = Some(FlowBuilder {
                            name,
                            is_function: false,
                            is_root_stitch: false,
                            parameters,
                            nodes: Vec::new(),
                            children: Vec::new(),
                        });
                    }
                    Header::Function { name, parameters } => {
                        finalize_stitch(&mut current_flow, &mut current_stitch)
                            .map_err(|e| e.with_line(ln))?;
                        if let Some(flow) = current_flow.take() {
                            flows.push(flow.build());
                        }

                        current_flow = Some(FlowBuilder {
                            name,
                            is_function: true,
                            is_root_stitch: false,
                            parameters,
                            nodes: Vec::new(),
                            children: Vec::new(),
                        });
                    }
                    Header::Stitch { name } => {
                        finalize_stitch(&mut current_flow, &mut current_stitch)
                            .map_err(|e| e.with_line(ln))?;
                        let parent_is_root_stitch =
                            current_flow.as_ref().is_some_and(|f| f.is_root_stitch);
                        if current_flow.is_none() || parent_is_root_stitch {
                            // Top-level stitch (no parent knot, or sibling of another root stitch)
                            if let Some(flow) = current_flow.take() {
                                flows.push(flow.build());
                            }
                            current_flow = Some(FlowBuilder {
                                name,
                                is_function: false,
                                is_root_stitch: true,
                                parameters: Vec::new(),
                                nodes: Vec::new(),
                                children: Vec::new(),
                            });
                        } else {
                            current_stitch = Some(FlowBuilder {
                                name,
                                is_function: false,
                                is_root_stitch: false,
                                parameters: Vec::new(),
                                nodes: Vec::new(),
                                children: Vec::new(),
                            });
                        }
                    }
                }

                line_index += 1;
                continue;
            }

            let statement = parse_statement(&lines, &mut line_index, false)?;
            match statement {
                ParsedStatement::Global(global) => globals.push(global),
                ParsedStatement::List(list_decl) => list_declarations.push(list_decl),
                ParsedStatement::ExternalFunction(name) => external_functions.push(name),
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

        Ok({
            let mut story = ParsedStory::new(globals, root, flows);
            story.list_declarations = list_declarations;
            story.external_functions = external_functions;
            story
        })
    }
}

impl FlowBuilder {
    fn build(self) -> Flow {
        Flow {
            name: self.name,
            is_function: self.is_function,
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
            CompilerError::invalid_source("stitch declared without enclosing knot".to_owned())
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

pub fn split_lines(source: &str) -> Vec<Line<'_>> {
    let mut lines: Vec<Line<'_>> = source
        .split_inclusive('\n')
        .map(|line| {
            let content = line.strip_suffix('\n').unwrap_or(line);
            // Strip trailing inline comments (// ...) but not inside strings
            let content = strip_inline_comment(content);
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

    if !source.ends_with('\n')
        && let Some(last_line) = lines.last_mut()
        && !last_line.content.is_empty()
    {
        last_line.had_newline = true;
    }

    lines
}

/// Strip trailing `// comment` from a line, respecting string literals.
fn strip_inline_comment(line: &str) -> &str {
    let mut in_string = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_string = !in_string,
            b'/' if !in_string && i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                return line[..i].trim_end();
            }
            _ => {}
        }
        i += 1;
    }
    line
}

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

    if let Some(rest) = trimmed.strip_prefix("LIST ") {
        *line_index += 1;
        return Ok(ParsedStatement::List(
            parse_list_declaration(rest).map_err(|e| e.with_line(ln))?,
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
            return Ok(ParsedStatement::Nodes(Vec::new()));
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

    if let Some(rest) = trimmed.strip_prefix("<- ") {
        *line_index += 1;
        let divert = parse_divert(rest.trim()).map_err(|e| e.with_line(ln))?;
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

pub fn parse_header(line: &str) -> Option<Header> {
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
    use expression::split_top_level_commas;

    let open = text.find('(');
    let close = text.rfind(')');

    match (open, close) {
        (Some(open), Some(close)) if close > open => {
            let name = parse_path_identifier(text[..open].trim())?.to_owned();
            let parameters = split_top_level_commas(&text[open + 1..close])
                .into_iter()
                .filter(|part| !part.trim().is_empty())
                .map(|part| {
                    // Strip divert-type annotation: "-> paramName" → "paramName"
                    let trimmed = part.trim();
                    if let Some(name) = trimmed.strip_prefix("->").map(str::trim) {
                        name.to_owned()
                    } else if let Some(name) = trimmed.strip_prefix("ref ") {
                        name.trim().to_owned()
                    } else {
                        trimmed.to_owned()
                    }
                })
                .collect();
            Some((name, parameters))
        }
        _ => Some((parse_path_identifier(text)?.to_owned(), Vec::new())),
    }
}
