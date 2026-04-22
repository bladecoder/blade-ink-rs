use super::{InkParser, sequences::build_sequence_node};
use crate::{parsed_hierarchy::{ParsedExpression, ParsedNode, ParsedNodeKind, SequenceType}, string_parser::ParseSuccess};

impl<'fh> InkParser<'fh> {
    pub(super) fn inner_logic_nodes(&mut self, terminators: &str) -> Option<Vec<ParsedNode>> {
        let _ = self.whitespace();

        if let Some(explicit_seq_type) = self.try_parse_sequence_type_annotation() {
            let elements = self.inner_sequence_objects(terminators)?;
            return Some(vec![build_sequence_node(explicit_seq_type, elements)]);
        }

        if let Some(initial_query_expression) = self.condition_expression_text() {
            return self.inner_conditional_nodes(Some(initial_query_expression), terminators);
        }

        for rule in [
            InnerLogicRule::Conditional,
            InnerLogicRule::Sequence,
            InnerLogicRule::Expression,
        ] {
            let rule_id = self.parser.begin_rule();
            let result = match rule {
                InnerLogicRule::Conditional => self.inner_conditional_nodes(None, terminators),
                InnerLogicRule::Sequence => {
                    self.inner_sequence_nodes(terminators, SequenceType::Stopping as u8)
                }
                InnerLogicRule::Expression => self.inner_expression_nodes(),
            };

            if let Some(nodes) = result {
                let _ = self.any_whitespace();
                if self.parser.end_of_input() {
                    return self.parser.succeed_rule(rule_id, Some(nodes));
                }
            }

            let _ = self.parser.fail_rule::<Vec<ParsedNode>>(rule_id);
        }

        None
    }

    fn inner_expression_nodes(&mut self) -> Option<Vec<ParsedNode>> {
        let expr = self.expression_until_top_level_terminators("")?;
        Some(vec![ParsedNode::new(ParsedNodeKind::OutputExpression).with_expression(expr)])
    }

    fn inner_conditional_nodes(
        &mut self,
        initial_query_expression: Option<ParsedExpression>,
        terminators: &str,
    ) -> Option<Vec<ParsedNode>> {
        let can_be_inline = initial_query_expression.is_some();
        let is_inline = self.newline().is_none();
        if is_inline && !can_be_inline {
            return None;
        }

        let mut alternatives = if is_inline {
            self.inline_conditional_branches(terminators)?
        } else {
            match self.multiline_conditional_branches() {
                Some(branches) => branches,
                None if initial_query_expression.is_some() => {
                    let sole_content = self.statements_at_level(super::StatementLevel::InnerBlock).nodes;
                    if sole_content.is_empty() {
                        return None;
                    }

                    let mut branches = vec![ConditionalBranchSpec {
                        condition: None,
                        content: sole_content,
                        is_else: false,
                        is_inline: false,
                        is_true_branch: false,
                        matching_equality: false,
                    }];

                    if let Some(mut else_branch) = self.single_multiline_condition() {
                        if !else_branch.is_else {
                            else_branch.is_else = true;
                        }
                        branches.push(else_branch);
                    }

                    branches
                }
                None => return None,
            }
        };

        if !is_inline
            && initial_query_expression.is_some()
            && alternatives.len() == 1
            && alternatives[0].is_else
        {
            alternatives.insert(
                0,
                ConditionalBranchSpec {
                    condition: None,
                    content: Vec::new(),
                    is_else: false,
                    is_inline: false,
                    is_true_branch: true,
                    matching_equality: false,
                },
            );
        }

        if initial_query_expression.is_none() {
            let last_index = alternatives.len().saturating_sub(1);
            for (idx, branch) in alternatives.iter_mut().enumerate() {
                if branch.condition.is_none() && idx == last_index {
                    branch.is_else = true;
                }
            }
        }

        for branch in &mut alternatives {
            branch.is_inline = is_inline;
        }

        Some(vec![build_conditional_node(initial_query_expression, alternatives)])
    }

    fn inline_conditional_branches(&mut self, terminators: &str) -> Option<Vec<ConditionalBranchSpec>> {
        let branches = self.inner_inline_sequence_objects(terminators)?;
        if branches.is_empty() || branches.len() > 2 {
            return None;
        }

        let mut result = Vec::new();
        let mut true_branch = ConditionalBranchSpec::from_nodes(branches[0].clone());
        true_branch.is_true_branch = true;
        result.push(true_branch);

        if branches.len() > 1 {
            let mut else_branch = ConditionalBranchSpec::from_nodes(branches[1].clone());
            else_branch.is_else = true;
            result.push(else_branch);
        }

        Some(result)
    }

    fn multiline_conditional_branches(&mut self) -> Option<Vec<ConditionalBranchSpec>> {
        self.multiline_whitespace();

        let mut branches = Vec::new();
        while let Some(branch) = self.single_multiline_condition() {
            branches.push(branch);
        }

        self.multiline_whitespace();
        (!branches.is_empty()).then_some(branches)
    }

    fn single_multiline_condition(&mut self) -> Option<ConditionalBranchSpec> {
        let rule_id = self.parser.begin_rule();
        self.whitespace();

        if self.parser.peek(|parser| parser.parse_string("->")).is_some() {
            return self.parser.fail_rule(rule_id);
        }

        if self.parser.parse_string("-").is_none() {
            return self.parser.fail_rule(rule_id);
        }

        self.whitespace();

        let is_else = self.else_expression();
        let condition = if is_else {
            None
        } else {
            self.condition_expression_text()
        };

        let mut content = self.statements_at_level(super::StatementLevel::InnerBlock).nodes;
        if condition.is_none() && content.is_empty() {
            content.push(ParsedNode::new(ParsedNodeKind::Text).with_text(""));
        }

        self.multiline_whitespace();

        self.parser.succeed_rule(
            rule_id,
            Some(ConditionalBranchSpec {
                condition,
                content,
                is_else,
                is_inline: false,
                is_true_branch: false,
                matching_equality: false,
            }),
        )
    }

    fn condition_expression_text(&mut self) -> Option<ParsedExpression> {
        let rule_id = self.parser.begin_rule();
        if self
            .parser
            .peek(|parser| parser.current_character())
            .is_some_and(|ch| ch == '\n' || ch == '\r' || ch == '-')
        {
            return self.parser.fail_rule(rule_id);
        }
        let Some(expr) = self.expression_until_top_level_terminators(":") else {
            return self.parser.fail_rule(rule_id);
        };
        self.whitespace();
        if self.parser.parse_string(":").is_none() {
            return self.parser.fail_rule(rule_id);
        }
        self.parser.succeed_rule(rule_id, Some(expr))
    }

    fn else_expression(&mut self) -> bool {
        let rule_id = self.parser.begin_rule();
        if self.parse_identifier().as_deref() != Some("else") {
            let _ = self.parser.fail_rule::<ParseSuccess>(rule_id);
            return false;
        }

        self.whitespace();
        if self.parser.parse_string(":").is_none() {
            let _ = self.parser.fail_rule::<ParseSuccess>(rule_id);
            return false;
        }

        self.parser.succeed_rule(rule_id, Some(ParseSuccess)).is_some()
    }
}

#[derive(Clone, Copy)]
enum InnerLogicRule {
    Conditional,
    Sequence,
    Expression,
}

#[derive(Clone)]
struct ConditionalBranchSpec {
    condition: Option<ParsedExpression>,
    content: Vec<ParsedNode>,
    is_else: bool,
    is_inline: bool,
    is_true_branch: bool,
    matching_equality: bool,
}

impl ConditionalBranchSpec {
    fn from_nodes(nodes: Vec<ParsedNode>) -> Self {
        Self {
            condition: None,
            content: nodes,
            is_else: false,
            is_inline: true,
            is_true_branch: false,
            matching_equality: false,
        }
    }
}

fn build_conditional_node(
    initial_condition: Option<ParsedExpression>,
    mut branches: Vec<ConditionalBranchSpec>,
) -> ParsedNode {
    let mut saw_branch_condition = false;
    let mut children = Vec::new();

    for (idx, branch) in branches.iter_mut().enumerate() {
        let mut branch_node = ParsedNode::new(ParsedNodeKind::Conditional);
        branch_node.is_inline = branch.is_inline;
        branch_node.is_else = branch.is_else;
        branch_node.is_true_branch = branch.is_true_branch;
        branch_node.matching_equality = branch.matching_equality;
        if let Some(condition) = branch.condition.clone() {
            branch_node = branch_node.with_condition(condition);
        }
        branch_node.set_children(branch.content.clone());

        if initial_condition.is_some() {
            if branch.condition.is_some() {
                branch_node.matching_equality = true;
                saw_branch_condition = true;
            } else if saw_branch_condition && branch.is_else {
                branch_node.matching_equality = true;
            } else if idx == 0 {
                branch_node.is_true_branch = true;
            } else {
                branch_node.is_else = true;
            }
        } else if branch.is_else {
            branch_node.is_else = true;
        }

        children.push(branch_node);
    }

    let mut node = ParsedNode::new(if initial_condition.is_some() {
        ParsedNodeKind::SwitchConditional
    } else {
        ParsedNodeKind::Conditional
    });
    if let Some(condition) = initial_condition {
        node = node.with_condition(condition);
    }
    node.set_children(children);
    node
}
