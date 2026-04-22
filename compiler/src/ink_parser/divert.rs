use super::{DivertPrototype, InkParser, parse_divert_argument_expressions};
use crate::parsed_hierarchy::{DivertNodeKind, ParsedNode, ParsedNodeKind};

impl<'fh> InkParser<'fh> {
    pub(super) fn try_parse_divert_line(&mut self) -> Option<Vec<ParsedNode>> {
        let rule_id = self.parser.begin_rule();

        self.whitespace();

        if let Some(thread) = self.start_thread() {
            let _ = self.end_of_line();
            let arguments = parse_divert_argument_expressions(&thread.arguments)?;
            let node = ParsedNode::new_divert(
                DivertNodeKind::Thread,
                thread.target,
                arguments,
            );
            return self.parser.succeed_rule(rule_id, Some(vec![node]));
        }

        let Some(first_arrow) = self.parse_divert_arrow_or_tunnel_onwards() else {
            return self.parser.fail_rule(rule_id);
        };

        let mut arrows = vec![first_arrow];
        let mut diverts = Vec::new();

        loop {
            self.whitespace();
            let Some(divert) = self.divert_identifier_with_arguments() else {
                break;
            };
            diverts.push(divert);
            self.whitespace();
            let Some(next_arrow) = self.parse_divert_arrow_or_tunnel_onwards() else {
                break;
            };
            arrows.push(next_arrow);
        }

        let mut nodes = Vec::new();
        let mut divert_index = 0usize;
        for (arrow_index, arrow) in arrows.iter().enumerate() {
            match arrow.as_str() {
                "->->" => {
                    if let Some(divert) = diverts.get(divert_index) {
                        let arguments = parse_divert_argument_expressions(&divert.arguments)?;
                        nodes.push(
                            ParsedNode::new_divert(
                                DivertNodeKind::TunnelOnwards,
                                divert.target.clone(),
                                arguments,
                            ),
                        );
                    } else {
                        nodes.push(ParsedNode::new(ParsedNodeKind::TunnelReturn));
                    }
                    break;
                }
                "->" => {
                    let Some(divert) = diverts.get(divert_index) else {
                        break;
                    };
                    divert_index += 1;
                    let kind = if arrow_index < arrows.len() - 1 {
                        DivertNodeKind::Tunnel
                    } else {
                        DivertNodeKind::Normal
                    };
                    let arguments = parse_divert_argument_expressions(&divert.arguments)?;
                    nodes.push(ParsedNode::new_divert(kind, divert.target.clone(), arguments));
                }
                _ => return self.parser.fail_rule(rule_id),
            }
        }

        let _ = self.end_of_line();
        self.parser.succeed_rule(rule_id, Some(nodes))
    }

    pub fn divert_identifier_with_arguments(&mut self) -> Option<DivertPrototype> {
        let _ = self.whitespace();
        let target = self.dot_separated_divert_path_components()?;
        let _ = self.whitespace();
        let arguments = self
            .expression_function_call_arguments()
            .unwrap_or_default();
        let _ = self.whitespace();

        Some(DivertPrototype {
            target,
            arguments,
            is_thread: false,
        })
    }

    pub fn start_thread(&mut self) -> Option<DivertPrototype> {
        let _ = self.whitespace();
        self.parse_thread_arrow()?;
        let _ = self.whitespace();
        let mut divert = self.divert_identifier_with_arguments()?;
        divert.is_thread = true;
        Some(divert)
    }
}
