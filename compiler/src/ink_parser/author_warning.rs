use super::InkParser;
use crate::parsed_hierarchy::{ParsedNode, ParsedNodeKind};

impl<'fh> InkParser<'fh> {
    pub(super) fn try_parse_author_warning_line(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        let Some(identifier) = self.parse_identifier() else {
            return self.parser.fail_rule(rule_id);
        };
        if identifier != "TODO" {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();
        let _ = self.parser.parse_string(":");
        self.whitespace();
        let message = self
            .parser
            .parse_until_characters_from_string("\n\r", -1)
            .unwrap_or_default();
        let _ = self.end_of_line();

        self.parser.succeed_rule(
            rule_id,
            Some(ParsedNode::new(ParsedNodeKind::AuthorWarning).with_text(message)),
        )
    }
}
