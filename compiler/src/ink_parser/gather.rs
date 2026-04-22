use super::InkParser;
use crate::parsed_hierarchy::{ParsedNode, ParsedNodeKind};

impl<'fh> InkParser<'fh> {
    pub(super) fn try_parse_gather(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        let marker = self.gather_marker();
        let Some(marker) = marker else {
            return self.parser.fail_rule(rule_id);
        };
        let depth = marker.indentation_depth;
        let label = marker.identifier;

        let _ = self.parser.parse_characters_from_string(" \t", true, -1);

        let mut content_nodes = Vec::new();

        if let Some(choice) = self.try_parse_choice() {
            content_nodes.push(choice);
        } else {
            content_nodes = self.parse_inline_content_until_with_brace_break("\n\r");
            if !content_nodes.is_empty() {
                content_nodes.push(ParsedNode::new(ParsedNodeKind::Newline));
            }
            let _ = self.end_of_line();
        }

        self.parser.succeed_rule(rule_id, Some(()));

        let mut node = ParsedNode::new(if label.is_some() {
            ParsedNodeKind::GatherLabel
        } else {
            ParsedNodeKind::GatherPoint
        });
        node.indentation_depth = depth;
        if let Some(label_name) = label {
            node = node.with_name(label_name);
        }
        if !content_nodes.is_empty() {
            node = node.with_children(content_nodes);
        }
        Some(node)
    }

    pub(super) fn try_parse_inline_label(&mut self) -> Option<String> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("(").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let Some(ident) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        if self.parser.parse_string(")").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        self.parser.succeed_rule(rule_id, Some(ident))
    }

    pub(super) fn skip_line(&mut self) {
        let _ = self
            .parser
            .parse_until_characters_from_string("\n\r", -1);
        let _ = self.parser.parse_newline();
    }
}
