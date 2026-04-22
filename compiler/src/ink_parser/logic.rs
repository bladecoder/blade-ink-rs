use super::InkParser;
use crate::parsed_hierarchy::{ParsedNode, ParsedNodeKind};

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
}
