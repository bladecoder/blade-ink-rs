use super::InkParser;
use crate::parsed_hierarchy::{ParsedAssignmentMode, ParsedNode, ParsedNodeKind};

impl<'fh> InkParser<'fh> {
    pub(super) fn try_parse_logic_line(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();
        if self.parser.parse_string("~").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        self.whitespace();

        if let Some(return_node) = self.try_parse_return_after_tilde() {
            let _ = self.end_of_line();
            return self.parser.succeed_rule(rule_id, Some(return_node));
        }

        if let Some(assignment_node) = self.temp_declaration_or_assignment() {
            let _ = self.end_of_line();
            return self.parser.succeed_rule(rule_id, Some(assignment_node));
        }

        let expression = self.expression_until_top_level_terminators("\n\r")?;
        let _ = self.end_of_line();
        match expression {
            crate::parsed_hierarchy::ParsedExpression::FunctionCall { .. } => self
                .parser
                .succeed_rule(rule_id, Some(ParsedNode::new(ParsedNodeKind::VoidCall).with_expression(expression))),
            _ => self.parser.fail_rule(rule_id),
        }
    }

    fn try_parse_return_after_tilde(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        if let Some(keyword) = self.parse_identifier()
            && keyword == "return"
        {
            self.whitespace();
            let expression = self.expression_until_top_level_terminators("\n\r");
            return if expression.is_none() {
                self.parser
                    .succeed_rule(rule_id, Some(ParsedNode::new(ParsedNodeKind::ReturnVoid)))
            } else {
                self.parser.succeed_rule(
                    rule_id,
                    Some(
                        ParsedNode::new(ParsedNodeKind::ReturnExpression)
                            .with_expression(expression?),
                    ),
                )
            };
        }

        self.parser.fail_rule(rule_id)
    }

    pub(super) fn temp_declaration_or_assignment(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        let default_mode = if self.parse_temp_keyword() {
            self.whitespace();
            "TempSet"
        } else {
            "Set"
        };

        let name = self.parse_identifier()?;
        self.whitespace();

        let (mode, expression_override) = if self.parser.parse_string("+=").is_some() {
            ("AddAssign", None)
        } else if self.parser.parse_string("-=").is_some() {
            ("SubtractAssign", None)
        } else if self.parser.parse_string("++").is_some() {
            ("AddAssign", Some(crate::parsed_hierarchy::ParsedExpression::Int(1)))
        } else if self.parser.parse_string("--").is_some() {
            (
                "SubtractAssign",
                Some(crate::parsed_hierarchy::ParsedExpression::Int(1)),
            )
        } else if self.parser.parse_string("=").is_some() {
            (default_mode, None)
        } else {
            return self.parser.fail_rule(rule_id);
        };

        self.whitespace();

        let expression = if let Some(expression) = expression_override {
            expression
        } else {
            self.whitespace();
            self.expression_until_top_level_terminators("\n\r")?
        };

        self.parser.succeed_rule(
            rule_id,
            Some(
                ParsedNode::new(ParsedNodeKind::Assignment)
                    .with_assignment(
                        match mode {
                            "Set" => ParsedAssignmentMode::Set,
                            "TempSet" => ParsedAssignmentMode::TempSet,
                            "AddAssign" => ParsedAssignmentMode::AddAssign,
                            "SubtractAssign" => ParsedAssignmentMode::SubtractAssign,
                            _ => return self.parser.fail_rule(rule_id),
                        },
                        name,
                    )
                    .with_expression(expression),
            ),
        )
    }

    fn parse_temp_keyword(&mut self) -> bool {
        let rule_id = self.parser.begin_rule();
        if self.parse_identifier().as_deref() == Some("temp") {
            self.parser.succeed_rule(rule_id, Some(crate::string_parser::ParseSuccess)).is_some()
        } else {
            let _ = self.parser.fail_rule::<crate::string_parser::ParseSuccess>(rule_id);
            false
        }
    }
}
