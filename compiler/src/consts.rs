use std::collections::HashMap;

use crate::ast::{Choice, Condition, Expression, Flow, Node, ParsedStory};

pub(crate) fn resolve(mut story: ParsedStory) -> ParsedStory {
    if story.consts.is_empty() {
        return story;
    }

    for global in &mut story.globals {
        resolve_expression(&mut global.initial_value, &story.consts);
    }

    let consts = story.consts.clone();
    resolve_nodes(&mut story.root, &consts);
    for flow in &mut story.flows {
        resolve_flow(flow, &consts);
    }
    story
}

fn resolve_flow(flow: &mut Flow, consts: &HashMap<String, Expression>) {
    resolve_nodes(&mut flow.nodes, consts);
    for child in &mut flow.children {
        resolve_flow(child, consts);
    }
}

fn resolve_nodes(nodes: &mut [Node], consts: &HashMap<String, Expression>) {
    for node in nodes {
        match node {
            Node::OutputExpression(expression) | Node::ReturnExpr(expression) => {
                resolve_expression(expression, consts);
            }
            Node::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                resolve_condition(condition, consts);
                resolve_nodes(when_true, consts);
                if let Some(nodes) = when_false {
                    resolve_nodes(nodes, consts);
                }
            }
            Node::SwitchConditional { value, branches } => {
                resolve_expression(value, consts);
                for (case, nodes) in branches {
                    if let Some(expression) = case {
                        resolve_expression(expression, consts);
                    }
                    resolve_nodes(nodes, consts);
                }
            }
            Node::Assignment { expression, .. } => resolve_expression(expression, consts),
            Node::Choice(choice) => resolve_choice(choice, consts),
            Node::VoidCall { args, .. }
            | Node::TunnelDivert { args, .. }
            | Node::TunnelOnwardsWithTarget { args, .. } => {
                for argument in args {
                    resolve_expression(argument, consts);
                }
            }
            Node::Sequence(sequence) => {
                for branch in &mut sequence.branches {
                    resolve_nodes(branch, consts);
                }
            }
            _ => {}
        }
    }
}

fn resolve_condition(condition: &mut Condition, consts: &HashMap<String, Expression>) {
    if let Condition::Expression(expression) = condition {
        resolve_expression(expression, consts);
    }
}

fn resolve_choice(choice: &mut Choice, consts: &HashMap<String, Expression>) {
    for condition in &mut choice.conditions {
        resolve_condition(condition, consts);
    }
    resolve_nodes(&mut choice.body, consts);
}

fn resolve_expression(expression: &mut Expression, consts: &HashMap<String, Expression>) {
    match expression {
        Expression::Variable(name) => {
            if let Some(value) = consts.get(name) {
                *expression = value.clone();
            }
        }
        Expression::Binary { left, right, .. } => {
            resolve_expression(left, consts);
            resolve_expression(right, consts);
        }
        Expression::Negate(inner) | Expression::Not(inner) => {
            resolve_expression(inner, consts);
        }
        Expression::FunctionCall { args, .. } => {
            for argument in args {
                resolve_expression(argument, consts);
            }
        }
        _ => {}
    }
}
