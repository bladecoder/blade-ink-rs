use crate::{
    error::CompilerError,
    parsed_hierarchy::{
        AssignMode, BinaryOperator, Choice, Condition, Divert, Expression, Flow, GlobalVariable,
        Node, ParsedStory,
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
    source
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
        .collect()
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

    if trimmed.starts_with('*') {
        return parse_choice(lines, line_index);
    }

    if let Some(divert) = trimmed.strip_prefix("->") {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(vec![Node::Divert(parse_divert(
            divert.trim(),
        )?)]));
    }

    *line_index += 1;
    parse_content_line(line, strip_leading_whitespace).map(ParsedStatement::Nodes)
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
    let remainder = trimmed_start
        .strip_prefix('*')
        .ok_or_else(|| CompilerError::InvalidSource("expected choice marker".to_owned()))?
        .trim_start();

    let (display_text, selected_text, inline_target) =
        if let Some(choice_only) = remainder.strip_prefix('[') {
            let end = choice_only.find(']').ok_or_else(|| {
                CompilerError::InvalidSource("choice label is missing closing ']'".to_owned())
            })?;
            let label = choice_only[..end].trim().to_owned();
            let after_label = choice_only[end + 1..].trim_start();
            let inline_target = after_label
                .strip_prefix("->")
                .map(str::trim)
                .map(parse_divert)
                .transpose()?;
            (label, None, inline_target)
        } else if let Some((before, after)) = remainder.split_once("[]") {
            let label = before.trim_end().to_owned();
            let suffix = after.trim_start();
            let selected = if suffix.is_empty() {
                Some(label.clone())
            } else {
                Some(format!("{label} {suffix}"))
            };
            (label, selected, None)
        } else {
            (
                remainder.trim().to_owned(),
                Some(remainder.trim().to_owned()),
                None,
            )
        };

    *line_index += 1;

    let choice_indent = line.indent;
    let mut body = Vec::new();

    if let Some(divert) = inline_target {
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
        display_text,
        selected_text,
        body,
    })]))
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

fn looks_like_conditional(content: &str) -> bool {
    content.starts_with('{') && content.contains(':')
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
            '=' if chars.get(index + 1) == Some(&'=') => {
                tokens.push(Token::EqualEqual);
                index += 2;
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
        self.parse_equality()
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.tokens.len()
    }

    fn parse_equality(&mut self) -> Result<Expression, CompilerError> {
        let mut expression = self.parse_addition()?;

        while self.match_token(&Token::EqualEqual) {
            let right = self.parse_addition()?;
            expression = Expression::Binary {
                left: Box::new(expression),
                operator: BinaryOperator::Equal,
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
