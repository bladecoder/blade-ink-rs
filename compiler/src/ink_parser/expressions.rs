use super::InkParser;
use crate::{
    parsed_hierarchy::{ParsedExpression, ParsedNode, ParsedNodeKind},
    string_parser::ParseSuccess,
};

impl<'fh> InkParser<'fh> {
    pub(super) fn expression_until_top_level_terminators(
        &mut self,
        terminators: &str,
    ) -> Option<ParsedExpression> {
        let text = self.parse_until_top_level_terminator_set(terminators)?;
        parse_expression_text(text.trim())
    }

    pub(super) fn expression(&mut self) -> Option<ParsedExpression> {
        self.expression_with_min_precedence(0)
    }

    fn expression_with_min_precedence(
        &mut self,
        minimum_precedence: i32,
    ) -> Option<ParsedExpression> {
        self.whitespace();

        let mut expr = self.expression_unary()?;
        self.whitespace();

        loop {
            let rule_id = self.parser.begin_rule();
            let Some(op) = self.parse_infix_operator() else {
                let _ = self.parser.fail_rule::<ParseSuccess>(rule_id);
                break;
            };

            if op.precedence <= minimum_precedence {
                let _ = self.parser.fail_rule::<ParseSuccess>(rule_id);
                break;
            }

            self.whitespace();
            let Some(right) = self.expression_with_min_precedence(op.precedence) else {
                let _ = self.parser.fail_rule::<ParseSuccess>(rule_id);
                return None;
            };

            expr = ParsedExpression::Binary {
                left: Box::new(expr),
                operator: op.kind.to_owned(),
                right: Box::new(right),
            };
            let _ = self.parser.succeed_rule(rule_id, Some(ParseSuccess));
            self.whitespace();
        }

        Some(expr)
    }

    fn expression_unary(&mut self) -> Option<ParsedExpression> {
        let rule_id = self.parser.begin_rule();

        if let Some(divert_target) = self.expression_divert_target() {
            return self.parser.succeed_rule(rule_id, Some(divert_target));
        }

        let prefix_op: Option<String> = if self.parser.parse_string("-").is_some() {
            Some("-".to_owned())
        } else if self.parser.parse_string("!").is_some() {
            Some("!".to_owned())
        } else {
            self.expression_not_keyword()
        };

        self.whitespace();

        let mut expr = self
            .expression_list()
            .or_else(|| self.expression_paren())
            .or_else(|| self.expression_function_call())
            .or_else(|| self.expression_variable_name())
            .or_else(|| self.expression_literal());

        if expr.is_none() && prefix_op.is_some() {
            expr = self.expression_unary();
        }

        let Some(mut expr) = expr else {
            return self.parser.fail_rule(rule_id);
        };

        if let Some(prefix_op) = prefix_op {
            expr = ParsedExpression::Unary {
                operator: prefix_op,
                expression: Box::new(expr),
            };
        }

        self.parser.succeed_rule(rule_id, Some(expr))
    }

    fn parse_infix_operator(&mut self) -> Option<InfixOperator> {
        for op in infix_operators() {
            let rule_id = self.parser.begin_rule();
            if self.parser.parse_string(op.token).is_some() {
                if op.require_whitespace_after && self.whitespace().is_none() {
                    let _ = self.parser.fail_rule::<InfixOperator>(rule_id);
                    continue;
                }
                return self.parser.succeed_rule(rule_id, Some(*op));
            }
            let _ = self.parser.fail_rule::<InfixOperator>(rule_id);
        }

        None
    }

    fn expression_not_keyword(&mut self) -> Option<String> {
        let rule_id = self.parser.begin_rule();
        if self.parse_identifier().as_deref() == Some("not") {
            self.parser.succeed_rule(rule_id, Some("!".to_owned()))
        } else {
            self.parser.fail_rule(rule_id)
        }
    }

    fn expression_paren(&mut self) -> Option<ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        let Some(expression) = self.expression_until_top_level_terminators(")") else {
            return self.parser.fail_rule(rule_id);
        };
        self.whitespace();
        if self.parser.parse_string(")").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.parser.succeed_rule(rule_id, Some(expression))
    }

    fn expression_list(&mut self) -> Option<ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();

        let mut members = Vec::new();
        if self.parser.parse_string(")").is_some() {
            return self.parser.succeed_rule(rule_id, Some(ParsedExpression::EmptyList));
        }

        loop {
            let Some(member) = self.list_member() else {
                return self.parser.fail_rule(rule_id);
            };
            members.push(member);

            self.whitespace();
            if self.parser.parse_string(")").is_some() {
                break;
            }

            if self.parser.parse_string(",").is_none() {
                return self.parser.fail_rule(rule_id);
            }
            self.whitespace();
        }

        self.parser
            .succeed_rule(rule_id, Some(ParsedExpression::ListItems(members)))
    }

    fn list_member(&mut self) -> Option<String> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        let Some(first) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        if is_number_only_identifier(&first) {
            return self.parser.fail_rule(rule_id);
        }

        let mut name = first;
        let checkpoint = self.parser.begin_rule();
        if self.parser.parse_string(".").is_some() {
            let Some(second) = self.parse_identifier() else {
                return self.parser.fail_rule(rule_id);
            };
            let _ = self.parser.succeed_rule(checkpoint, Some(ParseSuccess));
            name.push('.');
            name.push_str(&second);
        } else {
            let _ = self.parser.fail_rule::<ParseSuccess>(checkpoint);
        }

        self.whitespace();
        self.parser.succeed_rule(rule_id, Some(name))
    }

    fn expression_divert_target(&mut self) -> Option<ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        let Some(node) = self.single_divert_node() else {
            return self.parser.fail_rule(rule_id);
        };
        self.whitespace();
        let Some(target) = node.target() else {
            return self.parser.fail_rule(rule_id);
        };
        self.parser
            .succeed_rule(rule_id, Some(ParsedExpression::DivertTarget(target.to_owned())))
    }

    fn expression_literal(&mut self) -> Option<ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        let expr = self
            .expression_float()
            .or_else(|| self.expression_int())
            .or_else(|| self.expression_bool())
            .or_else(|| self.expression_string());
        let Some(expr) = expr else {
            return self.parser.fail_rule(rule_id);
        };
        self.parser.succeed_rule(rule_id, Some(expr))
    }

    fn expression_int(&mut self) -> Option<ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        let Some(value) = self.parser.parse_int() else {
            return self.parser.fail_rule(rule_id);
        };
        self.parser.succeed_rule(rule_id, Some(ParsedExpression::Int(value)))
    }

    fn expression_float(&mut self) -> Option<ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        let Some(value) = self.parser.parse_float() else {
            return self.parser.fail_rule(rule_id);
        };
        self.parser.succeed_rule(rule_id, Some(ParsedExpression::Float(value)))
    }

    fn expression_bool(&mut self) -> Option<ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        let Some(identifier) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        let expr = match identifier.as_str() {
            "true" => ParsedExpression::Bool(true),
            "false" => ParsedExpression::Bool(false),
            _ => return self.parser.fail_rule(rule_id),
        };
        self.parser.succeed_rule(rule_id, Some(expr))
    }

    fn expression_string(&mut self) -> Option<ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("\"").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        if self.parser.peek(|parser| parser.parse_string("\"")).is_some() {
            self.parser.parse_string("\"")?;
            return self
                .parser
                .succeed_rule(rule_id, Some(ParsedExpression::String(String::new())));
        }

        let was_parsing_string = self.parsing_string_expression;
        self.parsing_string_expression = true;
        let text_and_logic = self.string_text_and_logic();
        self.parsing_string_expression = was_parsing_string;

        if self.parser.parse_string("\"").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        if text_and_logic.iter().any(|node| {
            matches!(
                node.kind(),
                ParsedNodeKind::Divert
                    | ParsedNodeKind::TunnelDivert
                    | ParsedNodeKind::TunnelReturn
                    | ParsedNodeKind::TunnelOnwardsWithTarget
            )
        }) {
            return self.parser.fail_rule(rule_id);
        }

        self.parser
            .succeed_rule(rule_id, Some(ParsedExpression::StringExpression(text_and_logic)))
    }

    fn string_text_and_logic(&mut self) -> Vec<ParsedNode> {
        let mut nodes = Vec::new();

        loop {
            if self.parser.peek(|parser| parser.parse_string("\"")).is_some() {
                break;
            }

            if self.parser.peek(|parser| parser.parse_string("{")).is_some() {
                self.parser.parse_string("{");
                let Some(content) = self.parse_balanced_brace_body() else {
                    break;
                };
                if let Some(mut expression_nodes) = super::parse_inner_logic_string(
                    &content,
                    "\"",
                    self.parsing_choice,
                    false,
                ) {
                    nodes.append(&mut expression_nodes);
                    continue;
                }
                break;
            }

            if self.parser.peek(|parser| parser.parse_string("#")).is_some() {
                if let Some(tag) = self.parse_tag_content("\"") {
                    nodes.push(tag);
                    continue;
                }
                break;
            }

            if self.parser.parse_string("<>").is_some() {
                nodes.push(ParsedNode::new(ParsedNodeKind::Glue));
                continue;
            }

            let Some(text) = self.content_text_allowing_escape_char() else {
                break;
            };
            if !text.is_empty() {
                nodes.push(ParsedNode::new(ParsedNodeKind::Text).with_text(text));
            }
        }

        nodes
    }

    fn expression_function_call(&mut self) -> Option<ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        let Some(name) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        self.whitespace();
        let Some(arguments) = self.expression_function_call_arguments_parsed() else {
            return self.parser.fail_rule(rule_id);
        };
        self.parser.succeed_rule(
            rule_id,
            Some(ParsedExpression::FunctionCall { name, arguments }),
        )
    }

    fn expression_function_call_arguments_parsed(&mut self) -> Option<Vec<ParsedExpression>> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        let mut arguments = Vec::new();
        loop {
            let _ = self.whitespace();
            if self.parser.parse_string(")").is_some() {
                break;
            }

            let Some(argument) = self.expression_until_top_level_terminators(",)") else {
                return self.parser.fail_rule(rule_id);
            };
            arguments.push(argument);
            let _ = self.whitespace();
            if self.parser.parse_string(")").is_some() {
                break;
            }
            if self.parser.parse_string(",").is_none() {
                return self.parser.fail_rule(rule_id);
            }
        }

        self.parser.succeed_rule(rule_id, Some(arguments))
    }

    fn expression_variable_name(&mut self) -> Option<ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        let Some(first) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        if is_reserved_keyword(&first) || is_number_only_identifier(&first) {
            return self.parser.fail_rule(rule_id);
        }

        let mut path = vec![first];
        loop {
            let checkpoint = self.parser.begin_rule();
            let _ = self.whitespace();
            if self.parser.parse_string(".").is_none() {
                let _ = self.parser.fail_rule::<ParseSuccess>(checkpoint);
                break;
            }
            let _ = self.whitespace();
            let Some(next) = self.parse_identifier() else {
                return self.parser.fail_rule(rule_id);
            };
            self.parser.succeed_rule(checkpoint, Some(ParseSuccess));
            path.push(next);
        }

        self.parser
            .succeed_rule(rule_id, Some(ParsedExpression::Variable(path.join("."))))
    }
}

#[derive(Clone, Copy)]
struct InfixOperator {
    token: &'static str,
    kind: &'static str,
    precedence: i32,
    require_whitespace_after: bool,
}

fn parse_expression_text(text: &str) -> Option<ParsedExpression> {
    if let Some(expression) = fast_parse_simple_expression(text) {
        return Some(expression);
    }

    let mut parser = InkParser::new(text, None);
    let expression = parser.expression()?;
    let _ = parser.whitespace();
    if !parser.parser.end_of_input() {
        return None;
    }
    Some(expression)
}

fn fast_parse_simple_expression(text: &str) -> Option<ParsedExpression> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    match text {
        "true" => return Some(ParsedExpression::Bool(true)),
        "false" => return Some(ParsedExpression::Bool(false)),
        _ => {}
    }

    if let Ok(value) = text.parse::<i32>() {
        return Some(ParsedExpression::Int(value));
    }

    if text.contains('.') && let Ok(value) = text.parse::<f32>() {
        return Some(ParsedExpression::Float(value));
    }

    if text.starts_with('"')
        && text.ends_with('"')
        && text.len() >= 2
        && !text[1..text.len() - 1].contains(['{', '#'])
    {
        return Some(ParsedExpression::String(text[1..text.len() - 1].to_owned()));
    }

    if is_simple_identifier_or_path(text) && !is_reserved_keyword(text) && !is_number_only_identifier(text) {
        return Some(ParsedExpression::Variable(text.to_owned()));
    }

    None
}

fn is_simple_identifier_or_path(text: &str) -> bool {
    !text.is_empty()
        && text.split('.').all(|segment| {
            !segment.is_empty()
                && segment.chars().all(|ch| ch == '_' || ch.is_alphanumeric())
        })
}

fn is_reserved_keyword(name: &str) -> bool {
    matches!(
        name,
        "true" | "false" | "not" | "return" | "else" | "VAR" | "CONST" | "temp" | "LIST" | "function"
    )
}

fn is_number_only_identifier(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|ch| ch.is_ascii_digit())
}

fn infix_operators() -> &'static [InfixOperator] {
    &[
        InfixOperator { token: "&&", kind: "And", precedence: 1, require_whitespace_after: false },
        InfixOperator { token: "||", kind: "Or", precedence: 1, require_whitespace_after: false },
        InfixOperator { token: "and", kind: "And", precedence: 1, require_whitespace_after: true },
        InfixOperator { token: "or", kind: "Or", precedence: 1, require_whitespace_after: true },
        InfixOperator { token: "==", kind: "Equal", precedence: 2, require_whitespace_after: false },
        InfixOperator { token: ">=", kind: "GreaterEqual", precedence: 2, require_whitespace_after: false },
        InfixOperator { token: "<=", kind: "LessEqual", precedence: 2, require_whitespace_after: false },
        InfixOperator { token: "<", kind: "Less", precedence: 2, require_whitespace_after: false },
        InfixOperator { token: ">", kind: "Greater", precedence: 2, require_whitespace_after: false },
        InfixOperator { token: "!=", kind: "NotEqual", precedence: 2, require_whitespace_after: false },
        InfixOperator { token: "!?", kind: "Hasnt", precedence: 3, require_whitespace_after: false },
        InfixOperator { token: "?", kind: "Has", precedence: 3, require_whitespace_after: false },
        InfixOperator { token: "has", kind: "Has", precedence: 3, require_whitespace_after: true },
        InfixOperator { token: "hasnt", kind: "Hasnt", precedence: 3, require_whitespace_after: true },
        InfixOperator { token: "^", kind: "Intersect", precedence: 3, require_whitespace_after: false },
        InfixOperator { token: "+", kind: "Add", precedence: 4, require_whitespace_after: false },
        InfixOperator { token: "-", kind: "Subtract", precedence: 5, require_whitespace_after: false },
        InfixOperator { token: "*", kind: "Multiply", precedence: 6, require_whitespace_after: false },
        InfixOperator { token: "/", kind: "Divide", precedence: 7, require_whitespace_after: false },
        InfixOperator { token: "%", kind: "Modulo", precedence: 8, require_whitespace_after: false },
        InfixOperator { token: "mod", kind: "Modulo", precedence: 8, require_whitespace_after: true },
    ]
}
