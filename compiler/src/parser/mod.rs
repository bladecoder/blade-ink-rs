pub mod choice;
pub mod conditional;
pub mod expression;
pub mod inline;
pub mod sequence;

use crate::{
    ast::{AssignMode, Flow, GlobalVariable, Node, ParsedStory},
    error::CompilerError,
};

use self::{
    choice::parse_choice,
    conditional::{looks_like_conditional, parse_conditional},
    expression::{parse_bool, parse_expression, parse_path_identifier},
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
    parameters: Vec<String>,
    nodes: Vec<Node>,
    children: Vec<Flow>,
}

pub struct Parser<'a> {
    source: &'a str,
}

pub enum ParsedStatement {
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

pub fn split_lines(source: &str) -> Vec<Line<'_>> {
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

pub fn parse_statement(
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
        return parse_conditional(
            lines,
            line_index,
            strip_leading_whitespace,
            &parse_statement,
        )
        .map(ParsedStatement::Nodes);
    }

    if looks_like_sequence(trimmed) {
        return parse_sequence(
            lines,
            line_index,
            strip_leading_whitespace,
            &parse_statement,
        )
        .map(ParsedStatement::Nodes);
    }

    if looks_like_conditional(trimmed) {
        return parse_conditional(
            lines,
            line_index,
            strip_leading_whitespace,
            &parse_statement,
        )
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
        return parse_choice(lines, line_index, &parse_statement);
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
                .map(|part| part.trim().to_owned())
                .collect();
            Some((name, parameters))
        }
        _ => Some((parse_path_identifier(text)?.to_owned(), Vec::new())),
    }
}
