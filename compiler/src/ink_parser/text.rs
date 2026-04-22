use super::{InkParser, parse_divert_argument_expressions};
use crate::{parsed_hierarchy::{ContentList, ParsedNode, ParsedNodeKind}, string_parser::{CharacterSet}};

impl<'fh> InkParser<'fh> {
    pub fn content_text(&mut self) -> Option<String> {
        self.content_text_allowing_escape_char()
    }

    pub fn content_text_allowing_escape_char(&mut self) -> Option<String> {
        let mut out = String::new();

        loop {
            let part = self.content_text_no_escape();
            let got_escape = self.parser.parse_string("\\").is_some();
            let had_part = part.is_some();

            if let Some(part) = part {
                out.push_str(&part);
            }

            if got_escape {
                if let Some(ch) = self.parser.parse_single_character() {
                    out.push(ch);
                }
            }

            if !had_part && !got_escape {
                break;
            }
        }

        if out.is_empty() { None } else { Some(out) }
    }

    pub fn line_of_mixed_text(&mut self) -> Option<ContentList> {
        let _ = self.whitespace();
        let text = self.content_text()?;
        let mut list = ContentList::new();
        list.push_text(text);
        list.trim_trailing_whitespace();
        list.push_text("\n");
        let _ = self.end_of_line();
        Some(list)
    }

    fn content_text_no_escape(&mut self) -> Option<String> {
        let pause_characters = CharacterSet::from("-<");

        let mut end_characters = CharacterSet::from("{}|\n\r\\#");
        if self.parsing_choice {
            end_characters = end_characters.add_characters("[]".chars());
        }
        if self.parsing_string_expression {
            end_characters = end_characters.add_characters("\"".chars());
        }

        self.parser.parse_until(
            |parser| {
                parser
                    .parse_string("->")
                    .or_else(|| parser.parse_string("<-") )
                    .or_else(|| parser.parse_string("<>") )
                    .or_else(|| {
                        if parser.parse_newline().is_some() {
                            Some(String::new())
                        } else {
                            None
                        }
                    })
            },
            Some(&pause_characters),
            Some(&end_characters),
        )
    }

    pub(super) fn single_divert_node(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();
        let Some(node) = self.try_parse_inline_divert() else {
            return self.parser.fail_rule(rule_id);
        };

        if matches!(
            node.kind(),
            ParsedNodeKind::ThreadDivert
                | ParsedNodeKind::TunnelDivert
                | ParsedNodeKind::TunnelReturn
                | ParsedNodeKind::TunnelOnwardsWithTarget
        ) {
            return self.parser.fail_rule(rule_id);
        }

        self.parser.succeed_rule(rule_id, Some(node))
    }

    pub(super) fn try_parse_mixed_line(&mut self) -> Option<Vec<ParsedNode>> {
        self.whitespace();

        let mut nodes = self.mixed_text_and_logic()?;
        let line_is_pure_tag = !nodes.is_empty() && nodes.iter().all(|node| node.kind() == ParsedNodeKind::Tag);
        let line_ends_with_divert = nodes.last().is_some_and(|node| {
            matches!(
                node.kind(),
                ParsedNodeKind::Divert
                    | ParsedNodeKind::TunnelDivert
                    | ParsedNodeKind::TunnelReturn
                    | ParsedNodeKind::TunnelOnwardsWithTarget
            )
        });
        if !line_ends_with_divert {
            Self::trim_trailing_whitespace_nodes(&mut nodes);
        }
        if !line_is_pure_tag {
            nodes.push(ParsedNode::new(ParsedNodeKind::Newline));
        }
        let _ = self.end_of_line();

        Some(nodes)
    }

    fn mixed_text_and_logic(&mut self) -> Option<Vec<ParsedNode>> {
        let mut nodes: Vec<ParsedNode> = Vec::new();

        loop {
            if let Some(text) = self.content_text() {
                if !text.is_empty() {
                    nodes.push(ParsedNode::new(ParsedNodeKind::Text).with_text(text));
                }
            }

            if self.parser.peek(|parser| parser.parse_string("{")).is_some() {
                let mut expr_nodes = self.parse_braced_inline_content("\n\r")?;
                nodes.append(&mut expr_nodes);
                continue;
            }

            if self.parser.peek(|parser| parser.parse_string("#")).is_some() {
                if let Some(tag) = self.parse_tag_content("\n\r") {
                    nodes.push(tag);
                    continue;
                }
            }

            if self.parser.parse_string("<>").is_some() {
                nodes.push(ParsedNode::new(ParsedNodeKind::Glue));
                continue;
            }

            break;
        }

        if !self.parsing_choice && let Some(divert) = self.try_parse_inline_divert() {
            Self::append_trailing_space(&mut nodes);
            nodes.push(divert);
        }

        (!nodes.is_empty()).then_some(nodes)
    }

    pub(super) fn try_parse_inline_divert(&mut self) -> Option<ParsedNode> {
        let rule_id = self.parser.begin_rule();

        if self.parser.parse_string("->->").is_some() {
            self.whitespace();
            let node = if let Some(divert) = self.divert_identifier_with_arguments() {
                let arguments = parse_divert_argument_expressions(&divert.arguments)?;
                ParsedNode::new(ParsedNodeKind::TunnelOnwardsWithTarget)
                    .with_target(divert.target.join("."))
                    .with_arguments(arguments)
            } else {
                ParsedNode::new(ParsedNodeKind::TunnelReturn)
            };
            return self.parser.succeed_rule(rule_id, Some(node));
        }

        if self.parser.parse_string("->").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();

        let Some(divert) = self.divert_identifier_with_arguments() else {
            return self.parser.succeed_rule(rule_id, None);
        };

        let target = divert.target.join(".");
        let arguments = parse_divert_argument_expressions(&divert.arguments)?;

        let node = if self.parser.parse_string("->").is_some() {
            ParsedNode::new(ParsedNodeKind::TunnelDivert)
                .with_target(target)
                .with_arguments(arguments)
        } else {
            ParsedNode::new(ParsedNodeKind::Divert)
                .with_target(target)
                .with_arguments(arguments)
        };
        self.parser.succeed_rule(rule_id, Some(node))
    }

    pub(super) fn parse_until_top_level_terminator_set(&mut self, terminators: &str) -> Option<String> {
        let mut result = String::new();
        let mut paren_depth = 0usize;
        let mut brace_depth = 0usize;
        let mut bracket_depth = 0usize;
        let mut string_open = false;

        while let Some(ch) = self.parser.peek(|parser| parser.parse_single_character()) {
            if terminators.contains(ch)
                && !string_open
                && paren_depth == 0
                && brace_depth == 0
                && bracket_depth == 0
            {
                return Some(result);
            }

            let ch = self.parser.parse_single_character()?;
            match ch {
                '"' => string_open = !string_open,
                '(' if !string_open => paren_depth += 1,
                ')' if !string_open => paren_depth = paren_depth.saturating_sub(1),
                '{' if !string_open => brace_depth += 1,
                '}' if !string_open => brace_depth = brace_depth.saturating_sub(1),
                '[' if !string_open => bracket_depth += 1,
                ']' if !string_open => bracket_depth = bracket_depth.saturating_sub(1),
                _ => {}
            }
            result.push(ch);
        }

        Some(result)
    }

    fn trim_trailing_whitespace_nodes(nodes: &mut Vec<ParsedNode>) {
        if let Some(last) = nodes.last_mut() {
            if last.kind() == ParsedNodeKind::Text {
                let trimmed = last
                    .text()
                    .unwrap_or("")
                    .trim_end_matches([' ', '\t'])
                    .to_owned();
                if trimmed.is_empty() {
                    nodes.pop();
                    Self::trim_trailing_whitespace_nodes(nodes);
                } else {
                    *last = ParsedNode::new(ParsedNodeKind::Text).with_text(trimmed);
                }
            }
        }
    }

    pub(super) fn append_trailing_space(nodes: &mut Vec<ParsedNode>) {
        if let Some(last) = nodes.last_mut() {
            if last.kind() == ParsedNodeKind::Text {
                let trimmed = last
                    .text()
                    .unwrap_or("")
                    .trim_end_matches([' ', '\t'])
                    .to_owned();
                *last = ParsedNode::new(ParsedNodeKind::Text).with_text(format!("{trimmed} "));
                return;
            }
        }

        if !nodes.is_empty() {
            nodes.push(ParsedNode::new(ParsedNodeKind::Text).with_text(" "));
        }
    }
}
