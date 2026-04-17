use crate::{
    ast::{BinaryOperator, Expression},
    error::CompilerError,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Bool(bool),
    Int(i32),
    Float(f32),
    Str(String),
    Ident(String),
    DivertTarget(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqualEqual,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    AndAnd,
    OrOr,
    Bang,
    LeftParen,
    RightParen,
    Comma,
    /// `?` list has operator
    Has,
    /// `!?` list hasn't operator
    Hasnt,
    /// `^` list intersect operator
    Caret,
}

pub fn tokenize_expression(input: &str) -> Result<Vec<Token>, CompilerError> {
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
            let rest = input[index + 2..].trim_start();
            let parsed = parse_path_identifier(rest).ok_or_else(|| {
                CompilerError::invalid_source("expected divert target after '->'".to_owned())
            })?;
            tokens.push(Token::DivertTarget(parsed.to_owned()));
            // advance past the arrow and the target identifier
            index += 2; // skip "->"
            while index < chars.len() && chars[index].is_whitespace() {
                index += 1;
            }
            index += parsed.len(); // skip the target name
            continue;
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
            '/' => {
                tokens.push(Token::Slash);
                index += 1;
            }
            '%' => {
                tokens.push(Token::Percent);
                index += 1;
            }
            '!' if chars.get(index + 1) == Some(&'=') => {
                tokens.push(Token::NotEqual);
                index += 2;
            }
            '!' if chars.get(index + 1) == Some(&'?') => {
                tokens.push(Token::Hasnt);
                index += 2;
            }
            '!' => {
                tokens.push(Token::Bang);
                index += 1;
            }
            '&' if chars.get(index + 1) == Some(&'&') => {
                tokens.push(Token::AndAnd);
                index += 2;
            }
            '|' if chars.get(index + 1) == Some(&'|') => {
                tokens.push(Token::OrOr);
                index += 2;
            }
            '=' if chars.get(index + 1) == Some(&'=') => {
                tokens.push(Token::EqualEqual);
                index += 2;
            }
            '>' if chars.get(index + 1) == Some(&'=') => {
                tokens.push(Token::GreaterEqual);
                index += 2;
            }
            '>' => {
                tokens.push(Token::Greater);
                index += 1;
            }
            '<' if chars.get(index + 1) == Some(&'=') => {
                tokens.push(Token::LessEqual);
                index += 2;
            }
            '<' => {
                tokens.push(Token::Less);
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
            ',' => {
                tokens.push(Token::Comma);
                index += 1;
            }
            '?' => {
                tokens.push(Token::Has);
                index += 1;
            }
            '^' => {
                tokens.push(Token::Caret);
                index += 1;
            }
            '"' => {
                let mut end = index + 1;
                while end < chars.len() && chars[end] != '"' {
                    end += 1;
                }
                if end >= chars.len() {
                    return Err(CompilerError::invalid_source(
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
                        CompilerError::invalid_source(format!("invalid float literal: {error}"))
                    })?;
                    tokens.push(Token::Float(value));
                } else {
                    let value = input[start..index].parse::<i32>().map_err(|error| {
                        CompilerError::invalid_source(format!("invalid integer literal: {error}"))
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
                return Err(CompilerError::unsupported_feature(format!(
                    "unsupported token '{}' in expression",
                    ch
                )))
            }
        }
    }

    Ok(tokens)
}

pub fn parse_expression(input: &str) -> Result<Expression, CompilerError> {
    let tokens = tokenize_expression(input)?;
    let mut parser = ExpressionParser::new(tokens);
    let expression = parser.parse_expression()?;

    if !parser.is_at_end() {
        return Err(CompilerError::unsupported_feature(format!(
            "unexpected token in expression '{}'",
            input.trim()
        )));
    }

    Ok(expression)
}

pub fn parse_bool(value: &str) -> Result<bool, CompilerError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(CompilerError::unsupported_feature(format!(
            "unsupported boolean literal '{value}'"
        ))),
    }
}

pub fn parse_path_identifier(text: &str) -> Option<&str> {
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

pub fn parse_call_like(text: &str) -> Result<Option<(String, Vec<Expression>)>, CompilerError> {
    let trimmed = text.trim();
    let open = match trimmed.find('(') {
        Some(index) => index,
        None => return Ok(None),
    };
    let close = trimmed
        .rfind(')')
        .ok_or_else(|| CompilerError::invalid_source("missing ')' in divert target".to_owned()))?;
    if close < open {
        return Err(CompilerError::invalid_source(
            "invalid call-like syntax".to_owned(),
        ));
    }

    let name = parse_path_identifier(trimmed[..open].trim())
        .ok_or_else(|| CompilerError::invalid_source("invalid divert target".to_owned()))?
        .to_owned();
    let mut arguments = Vec::new();
    for argument in split_top_level_commas(&trimmed[open + 1..close]) {
        if !argument.trim().is_empty() {
            arguments.push(parse_expression(argument.trim())?);
        }
    }

    Ok(Some((name, arguments)))
}

pub fn split_top_level_commas(input: &str) -> Vec<&str> {
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

struct ExpressionParser {
    tokens: Vec<Token>,
    current: usize,
}

impl ExpressionParser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, current: 0 }
    }

    fn parse_expression(&mut self) -> Result<Expression, CompilerError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expression, CompilerError> {
        let mut expression = self.parse_and()?;

        while self.match_token(&Token::OrOr) {
            let right = self.parse_and()?;
            expression = Expression::Binary {
                left: Box::new(expression),
                operator: BinaryOperator::Or,
                right: Box::new(right),
            };
        }

        Ok(expression)
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

        loop {
            if self.match_token(&Token::EqualEqual) {
                let right = self.parse_comparison()?;
                expression = Expression::Binary {
                    left: Box::new(expression),
                    operator: BinaryOperator::Equal,
                    right: Box::new(right),
                };
            } else if self.match_token(&Token::NotEqual) {
                let right = self.parse_comparison()?;
                expression = Expression::Binary {
                    left: Box::new(expression),
                    operator: BinaryOperator::NotEqual,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(expression)
    }

    fn parse_comparison(&mut self) -> Result<Expression, CompilerError> {
        let mut expression = self.parse_addition()?;

        loop {
            if self.match_token(&Token::Greater) {
                let right = self.parse_addition()?;
                expression = Expression::Binary {
                    left: Box::new(expression),
                    operator: BinaryOperator::Greater,
                    right: Box::new(right),
                };
            } else if self.match_token(&Token::GreaterEqual) {
                let right = self.parse_addition()?;
                expression = Expression::Binary {
                    left: Box::new(expression),
                    operator: BinaryOperator::GreaterEqual,
                    right: Box::new(right),
                };
            } else if self.match_token(&Token::Less) {
                let right = self.parse_addition()?;
                expression = Expression::Binary {
                    left: Box::new(expression),
                    operator: BinaryOperator::Less,
                    right: Box::new(right),
                };
            } else if self.match_token(&Token::LessEqual) {
                let right = self.parse_addition()?;
                expression = Expression::Binary {
                    left: Box::new(expression),
                    operator: BinaryOperator::LessEqual,
                    right: Box::new(right),
                };
            } else if self.match_token(&Token::Has) {
                let right = self.parse_addition()?;
                expression = Expression::Binary {
                    left: Box::new(expression),
                    operator: BinaryOperator::Has,
                    right: Box::new(right),
                };
            } else if self.match_token(&Token::Hasnt) {
                let right = self.parse_addition()?;
                expression = Expression::Binary {
                    left: Box::new(expression),
                    operator: BinaryOperator::Hasnt,
                    right: Box::new(right),
                };
            } else if self.match_token(&Token::Caret) {
                let right = self.parse_addition()?;
                expression = Expression::Binary {
                    left: Box::new(expression),
                    operator: BinaryOperator::Intersect,
                    right: Box::new(right),
                };
            } else {
                break;
            }
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
        let mut expression = self.parse_unary()?;

        loop {
            if self.match_token(&Token::Star) {
                let right = self.parse_unary()?;
                expression = Expression::Binary {
                    left: Box::new(expression),
                    operator: BinaryOperator::Multiply,
                    right: Box::new(right),
                };
            } else if self.match_token(&Token::Slash) {
                let right = self.parse_unary()?;
                expression = Expression::Binary {
                    left: Box::new(expression),
                    operator: BinaryOperator::Divide,
                    right: Box::new(right),
                };
            } else if self.match_token(&Token::Percent) {
                let right = self.parse_unary()?;
                expression = Expression::Binary {
                    left: Box::new(expression),
                    operator: BinaryOperator::Modulo,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(expression)
    }

    fn parse_unary(&mut self) -> Result<Expression, CompilerError> {
        if self.match_token(&Token::Bang) {
            let expr = self.parse_primary()?;
            // !x in Ink is equivalent to (x == 0)
            return Ok(Expression::Binary {
                left: Box::new(expr),
                operator: BinaryOperator::Equal,
                right: Box::new(Expression::Int(0)),
            });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expression, CompilerError> {
        let token = self.advance().ok_or_else(|| {
            CompilerError::invalid_source("expected expression but found end of input".to_owned())
        })?;

        match token {
            Token::Bool(value) => Ok(Expression::Bool(value)),
            Token::Int(value) => Ok(Expression::Int(value)),
            Token::Float(value) => Ok(Expression::Float(value)),
            Token::Str(value) => Ok(Expression::Str(value)),
            Token::Ident(name) => {
                // Check if it's a function call: Ident followed by '('
                if self.match_token(&Token::LeftParen) {
                    let mut args = Vec::new();
                    // Parse arguments until ')'
                    if self.peek() != Some(&Token::RightParen) {
                        loop {
                            args.push(self.parse_expression()?);
                            if !self.match_token(&Token::Comma) {
                                break;
                            }
                        }
                    }
                    if !self.match_token(&Token::RightParen) {
                        return Err(CompilerError::invalid_source(
                            "missing ')' in function call".to_owned(),
                        ));
                    }
                    Ok(Expression::FunctionCall { name, args })
                } else {
                    Ok(Expression::Variable(name))
                }
            }
            Token::DivertTarget(target) => Ok(Expression::DivertTarget(target)),
            Token::Minus => {
                let expression = self.parse_unary()?;
                Ok(Expression::Negate(Box::new(expression)))
            }
            Token::LeftParen => {
                // Check for empty list: ()
                if self.match_token(&Token::RightParen) {
                    return Ok(Expression::EmptyList);
                }
                let expression = self.parse_expression()?;
                // If followed by a comma, this is a list literal: (a, b, c)
                if self.match_token(&Token::Comma) {
                    let mut items = vec![expr_to_list_item_name(&expression)?];
                    loop {
                        if self.peek() == Some(&Token::RightParen) {
                            break;
                        }
                        let item_expr = self.parse_expression()?;
                        items.push(expr_to_list_item_name(&item_expr)?);
                        if !self.match_token(&Token::Comma) {
                            break;
                        }
                    }
                    if !self.match_token(&Token::RightParen) {
                        return Err(CompilerError::invalid_source(
                            "missing ')' in list literal".to_owned(),
                        ));
                    }
                    return Ok(Expression::ListItems(items));
                }
                if !self.match_token(&Token::RightParen) {
                    return Err(CompilerError::invalid_source(
                        "missing ')' in expression".to_owned(),
                    ));
                }
                // Single-element list literal: (a) — treat as list with one item
                // but only if the expression is a plain identifier
                if let Expression::Variable(name) = &expression {
                    return Ok(Expression::ListItems(vec![name.clone()]));
                }
                Ok(expression)
            }
            _ => Err(CompilerError::unsupported_feature(
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

/// Extract the name string from an expression for use in a list literal.
fn expr_to_list_item_name(expr: &Expression) -> Result<String, CompilerError> {
    match expr {
        Expression::Variable(name) => Ok(name.clone()),
        _ => Err(CompilerError::invalid_source(
            "list literal items must be identifiers".to_owned(),
        )),
    }
}
