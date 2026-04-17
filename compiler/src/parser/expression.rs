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
    EqualEqual,
    Greater,
    AndAnd,
    LeftParen,
    RightParen,
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

pub fn parse_expression(input: &str) -> Result<Expression, CompilerError> {
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

pub fn parse_bool(value: &str) -> Result<bool, CompilerError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(CompilerError::UnsupportedFeature(format!(
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
