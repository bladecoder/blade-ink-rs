use super::InkParser;
use crate::{
    parsed_hierarchy::{ChoiceNodeSpec, ParsedNode, ParsedNodeKind},
    string_parser::ParseSuccess,
};

impl<'fh> InkParser<'fh> {
    pub(super) fn try_parse_choice(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();

        let Some(marker) = self.choice_marker() else {
            return self.parser.fail_rule(rule_id);
        };
        let once_only = marker.once_only;
        let depth = marker.indentation_depth;

        let _ = self.parser.parse_characters_from_string(" \t", true, -1);
        let label = self.try_parse_inline_label();
        let _ = self.whitespace();

        if label.is_some() {
            let _ = self.newline();
        }

        let choice_condition = self.parse_choice_condition();

        let _ = self.parser.parse_characters_from_string(" \t", true, -1);
        let _ = self.newline();
        let _ = self.parser.parse_characters_from_string(" \t", true, -1);

        let was_parsing_choice = self.parsing_choice;
        self.parsing_choice = true;

        let start_nodes = self.parse_choice_start_content();
        let mut choice_only_nodes = self.parse_choice_only_content();
        let inner_nodes = self.parse_choice_inner_content();

        if should_close_quoted_choice(&start_nodes, &choice_only_nodes, &inner_nodes) {
            append_text_suffix(&mut choice_only_nodes, "'");
        }

        let is_invisible_default = start_nodes.is_empty() && choice_only_nodes.is_empty();

        self.parsing_choice = was_parsing_choice;
        let _ = self.end_of_line();

        self.parser.succeed_rule(rule_id, Some(()));

        Some(
            ChoiceNodeSpec {
                indentation_depth: depth,
                once_only,
                identifier: label,
                condition: choice_condition,
                start_content: if choice_only_nodes.is_empty() {
                    trim_end_whitespace_nodes(start_nodes)
                } else {
                    start_nodes
                },
                choice_only_content: choice_only_nodes,
                inner_content: inner_nodes,
                is_invisible_default,
            }
            .build(),
        )
    }

    fn parse_choice_start_content(&mut self) -> Vec<ParsedNode> {
        self.parse_inline_content_until("[\n\r")
    }

    fn parse_choice_only_content(&mut self) -> Vec<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("[").is_none() {
            self.parser.cancel_rule(rule_id);
            return Vec::new();
        }
        let nodes = self.parse_inline_content_until("]");
        let _ = self.parser.parse_string("]");
        self.parser.succeed_rule(rule_id, Some(()));
        nodes
    }

    fn parse_choice_inner_content(&mut self) -> Vec<ParsedNode> {
        let mut nodes = trim_end_whitespace_nodes(self.parse_inline_content_until("\n\r"));
        if let Some(divert) = self.try_parse_inline_divert() {
            if !nodes.is_empty() {
                Self::append_trailing_space(&mut nodes);
            }
            nodes.push(divert);
        }
        nodes
    }

    fn parse_choice_condition(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let first = self.choice_single_condition()?;
        let mut conditions = vec![first];

        loop {
            let checkpoint = self.parser.begin_rule();
            if self.choice_conditions_space().is_none()
                || self.parser.peek(|parser| parser.parse_string("{")).is_none()
            {
                let _ = self.parser.fail_rule::<ParseSuccess>(checkpoint);
                break;
            }

            let Some(condition) = self.choice_single_condition() else {
                let _ = self.parser.fail_rule::<crate::parsed_hierarchy::ParsedExpression>(checkpoint);
                break;
            };
            self.parser.succeed_rule(checkpoint, Some(ParseSuccess));
            conditions.push(condition);
        }

        let mut iter = conditions.into_iter();
        let first = iter.next()?;
        Some(iter.fold(first, |left, right| crate::parsed_hierarchy::ParsedExpression::Binary {
            left: Box::new(left),
            operator: "And".to_owned(),
            right: Box::new(right),
        }))
    }

    fn choice_conditions_space(&mut self) -> Option<ParseSuccess> {
        let rule_id = self.parser.begin_rule();
        let start_index = self.parser.index();

        let _ = self.parser.parse_characters_from_string(" \t", true, -1);
        if self.parser.parse_newline().is_some() {
            let _ = self.parser.parse_characters_from_string(" \t", true, -1);
        }

        if self.parser.index() > start_index {
            self.parser.succeed_rule(rule_id, Some(ParseSuccess))
        } else {
            self.parser.fail_rule(rule_id)
        }
    }

    fn choice_single_condition(&mut self) -> Option<crate::parsed_hierarchy::ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("{").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        let Some(condition_text) = self.parser.parse_until_characters_from_string("}", -1) else {
            return self.parser.fail_rule(rule_id);
        };
        if self.parser.parse_string("}").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        let mut parser = InkParser::new(condition_text.trim(), None);
        let Some(condition) = parser.expression() else {
            return self.parser.fail_rule(rule_id);
        };
        let _ = parser.whitespace();
        if !parser.parser.end_of_input() {
            return self.parser.fail_rule(rule_id);
        }

        self.parser.succeed_rule(rule_id, Some(condition))
    }
}

fn trim_end_whitespace_nodes(mut nodes: Vec<ParsedNode>) -> Vec<ParsedNode> {
    if let Some(last) = nodes.last_mut()
        && last.kind() == ParsedNodeKind::Text
    {
        let trimmed = last
            .text()
            .unwrap_or("")
            .trim_end_matches([' ', '\t'])
            .to_owned();
        if trimmed.is_empty() {
            nodes.pop();
        } else {
            *last = ParsedNode::new(ParsedNodeKind::Text).with_text(trimmed);
        }
    }
    nodes
}

fn should_close_quoted_choice(
    start_nodes: &[ParsedNode],
    choice_only_nodes: &[ParsedNode],
    inner_nodes: &[ParsedNode],
) -> bool {
    if choice_only_nodes.is_empty() {
        return false;
    }

    let starts_with_quote = start_nodes
        .first()
        .and_then(|node| node.text())
        .is_some_and(|text| text.starts_with('\''));
    let inner_starts_with_comma_quote = inner_nodes
        .first()
        .and_then(|node| node.text())
        .is_some_and(|text| text.starts_with(",'"));

    starts_with_quote && inner_starts_with_comma_quote
}

fn append_text_suffix(nodes: &mut [ParsedNode], suffix: &str) {
    if let Some(last) = nodes.last_mut()
        && last.kind() == ParsedNodeKind::Text
    {
        let mut text = last.text().unwrap_or("").to_owned();
        text.push_str(suffix);
        *last = ParsedNode::new(ParsedNodeKind::Text).with_text(text);
    }
}
