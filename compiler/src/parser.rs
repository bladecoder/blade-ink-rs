use crate::{
    error::CompilerError,
    parsed_hierarchy::{
        AssignMode, BinaryOperator, Choice, ChoiceStyle, Condition, Expression, Flow,
        GlobalVariable, Node, ParsedStory,
    },
};

struct Line<'a> {
    content: &'a str,
    indent: usize,
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

        let mut globals = Vec::new();
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

            let statement = parse_statement(&lines, &mut line_index, false)?;
            match statement {
                ParsedStatement::Global(global) => globals.push(global),
                ParsedStatement::Nodes(mut nodes) => {
                    let target = if let Some(flow) = current_flow.as_mut() {
                        &mut flow.nodes
                    } else {
                        &mut root
                    };

                    target.append(&mut nodes);
                }
            }
        }

        if let Some(flow) = current_flow {
            flows.push(flow);
        }

        Ok(ParsedStory::new(globals, root, flows))
    }
}

enum ParsedStatement {
    Global(GlobalVariable),
    Nodes(Vec<Node>),
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

    if let Some(target) = trimmed.strip_prefix("->") {
        *line_index += 1;
        return Ok(ParsedStatement::Nodes(vec![Node::Divert(
            target.trim().to_owned(),
        )]));
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

    let content = content.trim_end();
    let mut nodes = tokenize_inline_content(content)?;

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

    if let Some(choice_only) = remainder.strip_prefix('[') {
        let end = choice_only.find(']').ok_or_else(|| {
            CompilerError::InvalidSource("choice label is missing closing ']'".to_owned())
        })?;

        let label = choice_only[..end].trim().to_owned();
        let after_label = choice_only[end + 1..].trim_start();
        let target = after_label
            .strip_prefix("->")
            .ok_or_else(|| {
                CompilerError::UnsupportedFeature(
                    "only single-line bracketed choices with diverts are supported".to_owned(),
                )
            })?
            .trim()
            .to_owned();

        *line_index += 1;
        return Ok(ParsedStatement::Nodes(vec![Node::Choice(Choice {
            label,
            body: vec![Node::Divert(target)],
            style: ChoiceStyle::ChoiceOnly,
        })]));
    }

    let label = remainder.trim().to_owned();
    let choice_indent = line.indent;
    *line_index += 1;

    let mut body = Vec::new();
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
        label,
        body,
        style: ChoiceStyle::EchoLabelOnSelect,
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

        let statement = parse_statement(lines, line_index, true)?;
        match statement {
            ParsedStatement::Global(_) => {
                return Err(CompilerError::UnsupportedFeature(
                    "global declarations are not supported inside conditionals".to_owned(),
                ))
            }
            ParsedStatement::Nodes(mut nodes) => branch.append(&mut nodes),
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
            let end = content[index + 1..]
                .find('}')
                .map(|offset| index + 1 + offset)
                .ok_or_else(|| {
                    CompilerError::InvalidSource("unterminated inline brace expression".to_owned())
                })?;

            if !text.is_empty() {
                nodes.push(Node::Text(std::mem::take(&mut text)));
            }

            let inline = &content[index..=end];
            if let Some((condition, branch_text)) = parse_inline_conditional(inline)? {
                nodes.push(Node::Conditional {
                    condition,
                    branch: tokenize_inline_content(branch_text.trim_end())?,
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Bool(bool),
    Int(i32),
    Str(String),
    Ident(String),
    DivertTarget(String),
    Plus,
    Minus,
    Star,
    LeftParen,
    RightParen,
}

fn tokenize_expression(input: &str) -> Result<Vec<Token>, CompilerError> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();

    while let Some((index, ch)) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }

        if ch == '-' && input[index..].starts_with("->") {
            let rest = input[index + 2..].trim_start();
            let name = parse_identifier(rest).ok_or_else(|| {
                CompilerError::InvalidSource("expected divert target after '->'".to_owned())
            })?;
            tokens.push(Token::DivertTarget(name.to_owned()));

            while let Some((peek_index, _)) = chars.peek() {
                if *peek_index < input.len() && input[*peek_index..].starts_with(rest) {
                    chars.next();
                } else {
                    break;
                }
            }

            break;
        }

        match ch {
            '+' => tokens.push(Token::Plus),
            '-' => tokens.push(Token::Minus),
            '*' => tokens.push(Token::Star),
            '(' => tokens.push(Token::LeftParen),
            ')' => tokens.push(Token::RightParen),
            '"' => {
                let string_start = index + 1;
                let string_end = input[string_start..]
                    .find('"')
                    .map(|offset| string_start + offset)
                    .ok_or_else(|| {
                        CompilerError::InvalidSource("unterminated string literal".to_owned())
                    })?;
                tokens.push(Token::Str(input[string_start..string_end].to_owned()));

                while let Some((peek_index, _)) = chars.peek() {
                    if *peek_index <= string_end {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            '0'..='9' => {
                let end = input[index..]
                    .char_indices()
                    .find(|(_, c)| !c.is_ascii_digit())
                    .map(|(offset, _)| index + offset)
                    .unwrap_or(input.len());
                let value = input[index..end].parse::<i32>().map_err(|error| {
                    CompilerError::InvalidSource(format!("invalid integer literal: {error}"))
                })?;
                tokens.push(Token::Int(value));

                while let Some((peek_index, _)) = chars.peek() {
                    if *peek_index < end {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let end = input[index..]
                    .char_indices()
                    .find(|(_, c)| !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_'))
                    .map(|(offset, _)| index + offset)
                    .unwrap_or(input.len());
                let ident = &input[index..end];

                match ident {
                    "true" => tokens.push(Token::Bool(true)),
                    "false" => tokens.push(Token::Bool(false)),
                    _ => tokens.push(Token::Ident(ident.to_owned())),
                }

                while let Some((peek_index, _)) = chars.peek() {
                    if *peek_index < end {
                        chars.next();
                    } else {
                        break;
                    }
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
        self.parse_addition()
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.tokens.len()
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
