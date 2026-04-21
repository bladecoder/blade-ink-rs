use crate::bootstrap::ast as legacy;

use super::{
    ExternalDeclaration, FlowArgument, FlowLevel, ListDefinition, ListElementDefinition,
    ParsedExpression, ParsedFlow, ParsedNode, ParsedNodeKind, Story, VariableAssignment,
};

impl Story {
    pub(crate) fn populate_from_legacy(&mut self, legacy_story: &legacy::ParsedStory) {
        self.global_declarations = legacy_story
            .globals()
            .iter()
            .map(|global| {
                let mut assignment = VariableAssignment::new(
                    global.name.clone(),
                    Some(convert_expression_node(&global.initial_value)),
                );
                assignment.set_global_declaration(true);
                assignment
            })
            .collect();
        self.global_initializers = legacy_story
            .globals()
            .iter()
            .map(|global| (global.name.clone(), convert_expression(&global.initial_value)))
            .collect();

        self.list_definitions = legacy_story
            .list_declarations()
            .iter()
            .map(convert_list_declaration)
            .collect();

        self.external_declarations = legacy_story
            .external_functions()
            .iter()
            .map(|name| ExternalDeclaration::new(name.clone(), Vec::new()))
            .collect();

        self.const_declarations = legacy_story
            .consts()
            .iter()
            .map(|(name, expression)| {
                super::ConstDeclaration::new(name.clone(), convert_expression_node(expression))
            })
            .collect();

        self.root_nodes = convert_nodes(legacy_story.root());
        self.flows = legacy_story.flows().iter().map(convert_flow).collect();

        self.rebuild_parse_tree_refs();
    }
}

fn convert_flow(flow: &legacy::Flow) -> ParsedFlow {
    let arguments = flow
        .parameters
        .iter()
        .map(|identifier| FlowArgument {
            identifier: identifier.clone(),
            is_by_reference: flow.ref_parameters.contains(identifier),
            is_divert_target: flow.divert_parameters.contains(identifier),
        })
        .collect();

    ParsedFlow::new(
        flow.name.clone(),
        FlowLevel::Knot,
        arguments,
        flow.is_function,
        convert_nodes(&flow.nodes),
        flow.children.iter().map(convert_child_flow).collect(),
    )
}

fn convert_child_flow(flow: &legacy::Flow) -> ParsedFlow {
    let arguments = flow
        .parameters
        .iter()
        .map(|identifier| FlowArgument {
            identifier: identifier.clone(),
            is_by_reference: flow.ref_parameters.contains(identifier),
            is_divert_target: flow.divert_parameters.contains(identifier),
        })
        .collect();

    ParsedFlow::new(
        flow.name.clone(),
        FlowLevel::Stitch,
        arguments,
        flow.is_function,
        convert_nodes(&flow.nodes),
        flow.children.iter().map(convert_child_flow).collect(),
    )
}

fn convert_nodes(nodes: &[legacy::Node]) -> Vec<ParsedNode> {
    nodes.iter().map(convert_node).collect()
}

fn convert_node(node: &legacy::Node) -> ParsedNode {
    match node {
        legacy::Node::Text(text) => ParsedNode::new(ParsedNodeKind::Text).with_text(text.clone()),
        legacy::Node::OutputExpression(expression) => {
            ParsedNode::new(ParsedNodeKind::OutputExpression)
                .with_expression(convert_expression(expression))
        }
        legacy::Node::Newline => ParsedNode::new(ParsedNodeKind::Newline).with_text("\n"),
        legacy::Node::Tag(tag) => {
            ParsedNode::new(ParsedNodeKind::Tag).with_text(convert_dynamic_string(tag))
        }
        legacy::Node::Glue => ParsedNode::new(ParsedNodeKind::Glue).with_text("<>"),
        legacy::Node::Sequence(sequence) => ParsedNode::new(ParsedNodeKind::Sequence)
            .with_name(format!("{:?}", sequence.mode))
            .with_children(
                sequence
                    .branches
                    .iter()
                    .enumerate()
                    .map(|(index, branch)| {
                        ParsedNode::new(ParsedNodeKind::Sequence)
                            .with_name(index.to_string())
                            .with_children(convert_nodes(branch))
                    })
                    .collect(),
            ),
        legacy::Node::Divert(divert) => ParsedNode::new(ParsedNodeKind::Divert)
            .with_target(divert.target.clone())
            .with_arguments(convert_expressions(&divert.arguments)),
        legacy::Node::TunnelDivert {
            target,
            is_variable,
            args,
        } => ParsedNode::new(ParsedNodeKind::TunnelDivert)
            .with_target(target.clone())
            .with_name(is_variable.to_string())
            .with_arguments(convert_expressions(args)),
        legacy::Node::TunnelReturn => ParsedNode::new(ParsedNodeKind::TunnelReturn),
        legacy::Node::TunnelOnwardsWithTarget { target, args } => {
            ParsedNode::new(ParsedNodeKind::TunnelOnwardsWithTarget)
                .with_target(target.clone())
                .with_arguments(convert_expressions(args))
        }
        legacy::Node::Conditional {
            condition,
            when_true,
            when_false,
        } => {
            let mut children = vec![
                ParsedNode::new(ParsedNodeKind::Conditional)
                    .with_name("true")
                    .with_children(convert_nodes(when_true)),
            ];
            if let Some(when_false) = when_false {
                children.push(
                    ParsedNode::new(ParsedNodeKind::Conditional)
                        .with_name("false")
                        .with_children(convert_nodes(when_false)),
                );
            }
            ParsedNode::new(ParsedNodeKind::Conditional)
                .with_expression(convert_condition(condition))
                .with_children(children)
        }
        legacy::Node::SwitchConditional { value, branches } => {
            ParsedNode::new(ParsedNodeKind::SwitchConditional)
                .with_expression(convert_expression(value))
                .with_children(
                    branches
                        .iter()
                        .map(|(case, body)| {
                            let mut branch = ParsedNode::new(ParsedNodeKind::Conditional)
                                .with_children(convert_nodes(body));
                            if let Some(case) = case {
                                branch = branch.with_expression(convert_expression(case));
                            } else {
                                branch = branch.with_name("else");
                            }
                            branch
                        })
                        .collect(),
                )
        }
        legacy::Node::ThreadDivert(divert) => ParsedNode::new(ParsedNodeKind::ThreadDivert)
            .with_target(divert.target.clone())
            .with_arguments(convert_expressions(&divert.arguments)),
        legacy::Node::ReturnBool(value) => ParsedNode::new(ParsedNodeKind::ReturnBool)
            .with_expression(ParsedExpression::Bool(*value)),
        legacy::Node::ReturnExpr(expression) => ParsedNode::new(ParsedNodeKind::ReturnExpression)
            .with_expression(convert_expression(expression)),
        legacy::Node::ReturnVoid => ParsedNode::new(ParsedNodeKind::ReturnVoid),
        legacy::Node::Assignment {
            variable_name,
            expression,
            mode,
        } => ParsedNode::new(ParsedNodeKind::Assignment)
            .with_name(format!("{mode:?}:{variable_name}"))
            .with_expression(convert_expression(expression)),
        legacy::Node::Choice(choice) => ParsedNode::new(ParsedNodeKind::Choice)
            .with_name(choice.label.clone().unwrap_or_default())
            .with_text(choice.display_text.clone())
            .with_children(convert_nodes(&choice.body)),
        legacy::Node::GatherPoint => ParsedNode::new(ParsedNodeKind::GatherPoint),
        legacy::Node::GatherLabel(label) => {
            ParsedNode::new(ParsedNodeKind::GatherLabel).with_name(label.clone())
        }
        legacy::Node::VoidCall { name, args } => ParsedNode::new(ParsedNodeKind::VoidCall)
            .with_name(name.clone())
            .with_arguments(convert_expressions(args)),
    }
}

fn convert_condition(condition: &legacy::Condition) -> ParsedExpression {
    match condition {
        legacy::Condition::Bool(value) => ParsedExpression::Bool(*value),
        legacy::Condition::FunctionCall(name) => ParsedExpression::FunctionCall {
            name: name.clone(),
            arguments: Vec::new(),
        },
        legacy::Condition::Expression(expression) => convert_expression(expression),
    }
}

fn convert_expression_node(expression: &legacy::Expression) -> super::ExpressionNode {
    match expression {
        legacy::Expression::Bool(value) => {
            super::ExpressionNode::Number(super::Number::new(super::NumberValue::Bool(*value)))
        }
        legacy::Expression::Int(value) => {
            super::ExpressionNode::Number(super::Number::new(super::NumberValue::Int(*value)))
        }
        legacy::Expression::Float(value) => {
            super::ExpressionNode::Number(super::Number::new(super::NumberValue::Float(*value)))
        }
        legacy::Expression::Str(value) => {
            let mut content = super::ContentList::new();
            content.push_text(value.clone());
            super::ExpressionNode::StringExpression(super::StringExpression::new(content))
        }
        legacy::Expression::Variable(name) => super::ExpressionNode::VariableReference(
            super::VariableReference::new(name.split('.').map(str::to_owned).collect()),
        ),
        legacy::Expression::ListItems(items) => {
            super::ExpressionNode::List(super::List::new(Some(items.clone())))
        }
        legacy::Expression::EmptyList => super::ExpressionNode::List(super::List::new(None)),
        other => {
            super::ExpressionNode::VariableReference(super::VariableReference::new(vec![format!(
                "{other:?}"
            )]))
        }
    }
}

fn convert_expression(expression: &legacy::Expression) -> ParsedExpression {
    match expression {
        legacy::Expression::Bool(value) => ParsedExpression::Bool(*value),
        legacy::Expression::Int(value) => ParsedExpression::Int(*value),
        legacy::Expression::Float(value) => ParsedExpression::Float(*value),
        legacy::Expression::Str(value) => ParsedExpression::String(value.clone()),
        legacy::Expression::Variable(name) => ParsedExpression::Variable(name.clone()),
        legacy::Expression::DivertTarget(target) => ParsedExpression::DivertTarget(target.clone()),
        legacy::Expression::Negate(expression) => ParsedExpression::Unary {
            operator: "-".to_owned(),
            expression: Box::new(convert_expression(expression)),
        },
        legacy::Expression::Not(expression) => ParsedExpression::Unary {
            operator: "!".to_owned(),
            expression: Box::new(convert_expression(expression)),
        },
        legacy::Expression::FunctionCall { name, args } => ParsedExpression::FunctionCall {
            name: name.clone(),
            arguments: convert_expressions(args),
        },
        legacy::Expression::ListItems(items) => ParsedExpression::ListItems(items.clone()),
        legacy::Expression::EmptyList => ParsedExpression::EmptyList,
        legacy::Expression::Binary {
            left,
            operator,
            right,
        } => ParsedExpression::Binary {
            left: Box::new(convert_expression(left)),
            operator: format!("{operator:?}"),
            right: Box::new(convert_expression(right)),
        },
    }
}

fn convert_expressions(expressions: &[legacy::Expression]) -> Vec<ParsedExpression> {
    expressions.iter().map(convert_expression).collect()
}

fn convert_dynamic_string(dynamic_string: &legacy::DynamicString) -> String {
    let mut result = String::new();
    for part in &dynamic_string.parts {
        match part {
            legacy::DynamicStringPart::Text(text) => result.push_str(text),
            legacy::DynamicStringPart::Expression(expression) => {
                result.push_str(&format!("{{{}}}", expression_debug_name(expression)));
            }
            legacy::DynamicStringPart::Sequence(sequence) => {
                result.push_str(&format!("{{{:?}}}", sequence.mode));
            }
        }
    }
    result
}

fn expression_debug_name(expression: &legacy::Expression) -> String {
    match expression {
        legacy::Expression::Variable(name) => name.clone(),
        legacy::Expression::FunctionCall { name, .. } => name.clone(),
        other => format!("{other:?}"),
    }
}

fn convert_list_declaration(list: &legacy::ListDeclaration) -> ListDefinition {
    let mut definition = ListDefinition::new(
        list.items
            .iter()
            .map(|(name, value, selected)| {
                ListElementDefinition::new(name.clone(), *selected, Some(*value as i32))
            })
            .collect(),
    );
    definition.set_identifier(list.name.clone());
    definition
}
