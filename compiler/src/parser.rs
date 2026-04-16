use crate::{
    error::CompilerError,
    parsed_hierarchy::{Condition, Flow, Node, ParsedStory},
};

struct Line<'a> {
    content: &'a str,
    had_newline: bool,
}

enum FlowHeader {
    Knot(String),
    Function(String),
}

pub struct Parser<'a> {
    source: &'a str,
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

        let mut root = Vec::new();
        let mut flows = Vec::new();
        let mut current_flow: Option<Flow> = None;
        let mut line_index = 0;

        while line_index < lines.len() {
            if let Some(header) = parse_header(lines[line_index].content) {
                if let Some(flow) = current_flow.take() {
                    flows.push(flow);
                }

                current_flow = Some(Flow {
                    name: header.into_name(),
                    nodes: Vec::new(),
                });
                line_index += 1;
                continue;
            }

            let target = if let Some(flow) = current_flow.as_mut() {
                &mut flow.nodes
            } else {
                &mut root
            };

            let mut nodes = parse_statement(&lines, &mut line_index, false)?;
            target.append(&mut nodes);
        }

        if let Some(flow) = current_flow {
            flows.push(flow);
        }

        Ok(ParsedStory::new(root, flows))
    }
}

fn split_lines(source: &str) -> Vec<Line<'_>> {
    source
        .split_inclusive('\n')
        .map(|line| Line {
            content: line.strip_suffix('\n').unwrap_or(line),
            had_newline: line.ends_with('\n'),
        })
        .collect()
}

fn parse_statement(
    lines: &[Line<'_>],
    line_index: &mut usize,
    in_conditional_body: bool,
) -> Result<Vec<Node>, CompilerError> {
    let line = &lines[*line_index];
    let trimmed = line.content.trim();

    if trimmed.is_empty() {
        *line_index += 1;
        return Ok(Vec::new());
    }

    if in_conditional_body && trimmed == "}" {
        return Err(CompilerError::InvalidSource(
            "conditional body parser advanced past its closing brace".to_owned(),
        ));
    }

    if trimmed.starts_with('{') {
        return parse_conditional(lines, line_index, in_conditional_body);
    }

    if let Some(target) = trimmed.strip_prefix("->") {
        *line_index += 1;
        return Ok(vec![Node::Divert(target.trim().to_owned())]);
    }

    if let Some(value) = trimmed.strip_prefix("~ return ") {
        *line_index += 1;
        return Ok(vec![Node::ReturnBool(parse_bool(value.trim())?)]);
    }

    *line_index += 1;
    parse_content_line(line, in_conditional_body)
}

fn parse_content_line(
    line: &Line<'_>,
    strip_leading_whitespace: bool,
) -> Result<Vec<Node>, CompilerError> {
    let content = if strip_leading_whitespace {
        line.content.trim_start()
    } else {
        line.content
    };

    let content = content.trim_end();
    let mut nodes = tokenize_inline_content(content)?;

    if line.had_newline {
        nodes.push(Node::Newline);
    }

    Ok(nodes)
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
    let content = content.trim_end();

    if let Some((condition, branch_text)) = parse_inline_conditional(content)? {
        *line_index += 1;
        let mut nodes = vec![Node::Conditional {
            condition,
            branch: tokenize_inline_content(branch_text.trim_end())?,
        }];

        if line.had_newline {
            nodes.push(Node::Newline);
        }

        return Ok(nodes);
    }

    let header = content
        .strip_prefix('{')
        .ok_or_else(|| CompilerError::InvalidSource("expected conditional block".to_owned()))?;
    let (condition_text, _) = header.split_once(':').ok_or_else(|| {
        CompilerError::InvalidSource("conditional block is missing ':'".to_owned())
    })?;

    let condition = parse_condition(condition_text.trim())?;
    *line_index += 1;

    let mut branch = Vec::new();
    if line.had_newline {
        branch.push(Node::Newline);
    }

    while *line_index < lines.len() {
        let body_line = &lines[*line_index];
        if body_line.content.trim() == "}" {
            let closing_had_newline = body_line.had_newline && (*line_index + 1) < lines.len();
            *line_index += 1;

            let mut nodes = vec![Node::Conditional { condition, branch }];
            if closing_had_newline {
                nodes.push(Node::Newline);
            }

            return Ok(nodes);
        }

        let mut body_nodes = parse_statement(lines, line_index, true)?;
        branch.append(&mut body_nodes);
    }

    Err(CompilerError::InvalidSource(
        "unterminated conditional block".to_owned(),
    ))
}

fn tokenize_inline_content(content: &str) -> Result<Vec<Node>, CompilerError> {
    let mut nodes = Vec::new();
    let mut text = String::new();
    let mut chars = content.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if ch == '<' && content[index..].starts_with("<>") {
            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(&mut text)));
            }
            nodes.push(Node::Glue);
            chars.next();
            continue;
        }

        if ch == '{' {
            let end = content[index + 1..]
                .find('}')
                .map(|offset| index + 1 + offset)
                .ok_or_else(|| {
                    CompilerError::UnsupportedFeature(
                        "multiline conditionals embedded inside text are not supported".to_owned(),
                    )
                })?;

            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(&mut text)));
            }

            let inline = &content[index..=end];
            let (condition, branch_text) = parse_inline_conditional(inline)?.ok_or_else(|| {
                CompilerError::InvalidSource("invalid inline conditional".to_owned())
            })?;
            nodes.push(Node::Conditional {
                condition,
                branch: tokenize_inline_content(branch_text.trim_end())?,
            });

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

fn parse_inline_conditional(content: &str) -> Result<Option<(Condition, &str)>, CompilerError> {
    let trimmed = content.trim();
    if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
        return Ok(None);
    }

    let inner = &trimmed[1..trimmed.len() - 1];
    let (condition, branch) = inner.split_once(':').ok_or_else(|| {
        CompilerError::InvalidSource("inline conditional is missing ':'".to_owned())
    })?;

    Ok(Some((parse_condition(condition.trim())?, branch)))
}

fn parse_condition(condition: &str) -> Result<Condition, CompilerError> {
    match condition {
        "true" => Ok(Condition::Bool(true)),
        "false" => Ok(Condition::Bool(false)),
        _ => {
            if let Some(name) = condition.strip_suffix("()") {
                return Ok(Condition::FunctionCall(name.trim().to_owned()));
            }

            Err(CompilerError::UnsupportedFeature(format!(
                "unsupported condition '{condition}'"
            )))
        }
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

fn parse_header(line: &str) -> Option<FlowHeader> {
    let trimmed = line.trim();
    if !trimmed.starts_with("==") {
        return None;
    }

    let inner = trimmed.trim_matches('=').trim();
    if let Some(rest) = inner.strip_prefix("function") {
        let name = parse_identifier(rest.trim())?;
        return Some(FlowHeader::Function(name.to_owned()));
    }

    let name = parse_identifier(inner)?;
    Some(FlowHeader::Knot(name.to_owned()))
}

fn parse_identifier(text: &str) -> Option<&str> {
    let end = text
        .char_indices()
        .find(|(_, ch)| !matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_'))
        .map(|(index, _)| index)
        .unwrap_or(text.len());

    if end == 0 {
        None
    } else {
        Some(&text[..end])
    }
}

impl FlowHeader {
    fn into_name(self) -> String {
        match self {
            Self::Knot(name) | Self::Function(name) => name,
        }
    }
}
