use super::{InkParser, StatementLevel};
use crate::parsed_hierarchy::{ParsedNode, ParsedNodeKind, SequenceType};

impl<'fh> InkParser<'fh> {
    pub(super) fn inner_sequence_nodes(&mut self, terminators: &str, default_flags: u8) -> Option<Vec<ParsedNode>> {
        let elements = self.inner_sequence_objects(terminators)?;
        if elements.len() <= 1 {
            return None;
        }
        Some(vec![build_sequence_node(default_flags, elements)])
    }

    pub(super) fn inner_sequence_objects(&mut self, terminators: &str) -> Option<Vec<Vec<ParsedNode>>> {
        let multiline = self.newline().is_some();
        if multiline {
            self.inner_multiline_sequence_objects()
        } else {
            self.inner_inline_sequence_objects(terminators)
        }
    }

    pub(super) fn inner_inline_sequence_objects(&mut self, terminators: &str) -> Option<Vec<Vec<ParsedNode>>> {
        let mut result = Vec::new();
        let mut just_had_content = false;

        loop {
            let content = self.parse_inline_content_until_with_brace_break(&format!("|}}{terminators}"));
            if !content.is_empty() {
                result.push(content);
                just_had_content = true;
            }

            if self.parser.parse_string("|").is_some() {
                if !just_had_content {
                    result.push(Vec::new());
                }
                just_had_content = false;
                continue;
            }

            break;
        }

        if !just_had_content {
            result.push(Vec::new());
        }

        (!result.is_empty()).then_some(result)
    }

    fn inner_multiline_sequence_objects(&mut self) -> Option<Vec<Vec<ParsedNode>>> {
        self.multiline_whitespace();

        let mut result = Vec::new();
        while let Some(content) = self.single_multiline_sequence_element() {
            result.push(content);
        }

        (!result.is_empty()).then_some(result)
    }

    fn single_multiline_sequence_element(&mut self) -> Option<Vec<ParsedNode>> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        if self.parser.peek(|parser| parser.parse_string("->")).is_some() {
            return self.parser.fail_rule(rule_id);
        }

        if self.parser.parse_string("-").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();
        let mut content = self.statements_at_level(StatementLevel::InnerBlock).nodes;
        if content.is_empty() {
            self.multiline_whitespace();
        } else {
            content.insert(0, ParsedNode::new(ParsedNodeKind::Newline));
        }

        self.parser.succeed_rule(rule_id, Some(content))
    }

    pub(super) fn try_parse_sequence_type_annotation(&mut self) -> Option<u8> {
        let rule_id = self.parser.begin_rule();
        let mut combined = 0u8;
        let mut matched_any = false;
        loop {
            self.whitespace();
            let matched = if self.parser.parse_string("stopping").is_some() {
                combined |= SequenceType::Stopping as u8;
                true
            } else if self.parser.parse_string("cycle").is_some() {
                combined |= SequenceType::Cycle as u8;
                true
            } else if self.parser.parse_string("shuffle").is_some() {
                combined |= SequenceType::Shuffle as u8;
                true
            } else if self.parser.parse_string("once").is_some() {
                combined |= SequenceType::Once as u8;
                true
            } else {
                false
            };

            if !matched {
                break;
            }
            matched_any = true;
        }

        if matched_any {
            self.whitespace();
            if self.parser.parse_string(":").is_some() {
                return self.parser.succeed_rule(rule_id, Some(combined));
            }
        }
        let _ = self.parser.fail_rule::<u8>(rule_id);

        let rule_id = self.parser.begin_rule();

        let mut symbol_combined = 0u8;
        let mut saw_symbol = false;
        loop {
            let matched = if self.parser.parse_string("!").is_some() {
                symbol_combined |= SequenceType::Once as u8;
                true
            } else if self.parser.parse_string("&").is_some() {
                symbol_combined |= SequenceType::Cycle as u8;
                true
            } else if self.parser.parse_string("~").is_some() {
                symbol_combined |= SequenceType::Shuffle as u8;
                true
            } else if self.parser.parse_string("$").is_some() {
                symbol_combined |= SequenceType::Stopping as u8;
                true
            } else {
                false
            };
            if !matched {
                break;
            }
            saw_symbol = true;
        }

        if saw_symbol {
            self.parser.succeed_rule(rule_id, Some(symbol_combined))
        } else {
            self.parser.fail_rule(rule_id)
        }
    }
}

pub(super) fn build_sequence_node(sequence_type: u8, elements: Vec<Vec<ParsedNode>>) -> ParsedNode {
    let element_children = elements
        .into_iter()
        .map(|nodes| ParsedNode::new(ParsedNodeKind::Text).with_text("").with_children(nodes))
        .collect();
    let mut seq_node = ParsedNode::new(ParsedNodeKind::Sequence);
    seq_node.sequence_type = sequence_type;
    seq_node.set_children(element_children);
    seq_node
}
