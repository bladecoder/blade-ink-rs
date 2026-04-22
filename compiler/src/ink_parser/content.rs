use super::{InkParser, parse_inner_logic_string};
use crate::parsed_hierarchy::{ParsedNode, ParsedNodeKind};

impl<'fh> InkParser<'fh> {
    pub(super) fn parse_inline_content_until(&mut self, terminators: &str) -> Vec<ParsedNode> {
        let mut nodes = Vec::new();

        loop {
            if self
                .parser
                .peek(|parser| {
                    parser
                        .current_character()
                        .filter(|ch| terminators.contains(*ch))
                        .map(|_| ())
                })
                .is_some()
            {
                break;
            }

            if self.parser.peek(|parser| parser.parse_string("{")).is_some() {
                let Some(mut expression_nodes) = self.parse_braced_inline_content(terminators) else {
                    break;
                };
                nodes.append(&mut expression_nodes);
                continue;
            }

            if self.parser.peek(|parser| parser.parse_string("#")).is_some() {
                if let Some(tag) = self.parse_tag_content(terminators) {
                    nodes.push(tag);
                    continue;
                }
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

    pub(super) fn parse_braced_inline_content(&mut self, terminators: &str) -> Option<Vec<ParsedNode>> {
        let rule_id = self.parser.begin_rule();
        self.parser.parse_string("{")?;
        let content = self.parse_balanced_brace_body()?;
        self.parser.succeed_rule(rule_id, Some(()));

        parse_inner_logic_string(
            &content,
            terminators,
            self.parsing_choice,
            self.parsing_string_expression,
        )
    }

    pub(super) fn parse_tag_content(&mut self, terminators: &str) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        if self.parser.parse_string("#").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        let _ = self.whitespace();
        let content = self.parse_inline_content_until(terminators);
        self.parser.succeed_rule(
            rule_id,
            Some(ParsedNode::new(ParsedNodeKind::Tag).with_children(content)),
        )
    }

    pub(super) fn parse_inline_content_until_with_brace_break(&mut self, terminators: &str) -> Vec<ParsedNode> {
        let mut nodes = self.parse_inline_content_until(terminators);
        if !self.parsing_choice && let Some(divert) = self.try_parse_inline_divert() {
            Self::append_trailing_space(&mut nodes);
            nodes.push(divert);
        }
        nodes
    }

    pub(super) fn parse_balanced_brace_body(&mut self) -> Option<String> {
        let mut depth = 1usize;
        let mut string_open = false;
        let mut content = String::new();

        while let Some(ch) = self.parser.parse_single_character() {
            match ch {
                '"' => {
                    string_open = !string_open;
                    content.push(ch);
                }
                '{' if !string_open => {
                    depth += 1;
                    content.push(ch);
                }
                '}' if !string_open => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(content);
                    }
                    content.push(ch);
                }
                _ => content.push(ch),
            }
        }

        None
    }
}
