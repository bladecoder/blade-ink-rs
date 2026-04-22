use super::InkParser;
use crate::parsed_hierarchy::{
    ConstDeclaration, ContentList, ExternalDeclaration, List, ListDefinition, Number,
    NumberValue, ParsedFlow, ParsedNode, Story, VariableAssignment,
};

#[derive(Default)]
pub(super) struct ParseSection {
    pub nodes: Vec<ParsedNode>,
    pub flows: Vec<ParsedFlow>,
    pub global_declarations: Vec<VariableAssignment>,
    pub global_initializers: Vec<(String, crate::parsed_hierarchy::ParsedExpression)>,
    pub const_declarations: Vec<ConstDeclaration>,
    pub list_definitions: Vec<ListDefinition>,
    pub external_declarations: Vec<ExternalDeclaration>,
}

pub(super) fn parse_inner_logic_string(
    input: &str,
    terminators: &str,
    parsing_choice: bool,
    parsing_string_expression: bool,
) -> Option<Vec<ParsedNode>> {
    let mut parser = InkParser::new(input, None);
    parser.parsing_choice = parsing_choice;
    parser.parsing_string_expression = parsing_string_expression;
    let nodes = parser.inner_logic_nodes(terminators)?;
    let _ = parser.any_whitespace();
    if !parser.parser.end_of_input() {
        return None;
    }
    Some(nodes)
}

pub(super) fn parse_divert_argument_expressions(
    arguments: &[String],
) -> Option<Vec<crate::parsed_hierarchy::ParsedExpression>> {
    let mut parsed_arguments = Vec::with_capacity(arguments.len());

    for argument in arguments {
        let mut parser = InkParser::new(argument.trim(), None);
        let expression = parser.expression()?;
        let _ = parser.whitespace();
        if !parser.parser.end_of_input() {
            return None;
        }
        parsed_arguments.push(expression);
    }

    Some(parsed_arguments)
}

pub(super) fn parsed_expression_to_expression_node(
    expression: crate::parsed_hierarchy::ParsedExpression,
) -> Option<crate::parsed_hierarchy::ExpressionNode> {
    use crate::parsed_hierarchy::{ExpressionNode, VariableReference};

    match expression {
        crate::parsed_hierarchy::ParsedExpression::Bool(value) => {
            Some(ExpressionNode::Number(Number::new(NumberValue::Bool(value))))
        }
        crate::parsed_hierarchy::ParsedExpression::Int(value) => {
            Some(ExpressionNode::Number(Number::new(NumberValue::Int(value))))
        }
        crate::parsed_hierarchy::ParsedExpression::Float(value) => {
            Some(ExpressionNode::Number(Number::new(NumberValue::Float(value))))
        }
        crate::parsed_hierarchy::ParsedExpression::String(value) => {
            let mut content = ContentList::new();
            content.push_text(value);
            Some(ExpressionNode::StringExpression(
                crate::parsed_hierarchy::StringExpression::new(content),
            ))
        }
        crate::parsed_hierarchy::ParsedExpression::Variable { path, .. } => Some(
            ExpressionNode::VariableReference(VariableReference::new(
                path.clone(),
            )),
        ),
        crate::parsed_hierarchy::ParsedExpression::ListItems(items) => {
            Some(ExpressionNode::List(List::new(Some(items))))
        }
        crate::parsed_hierarchy::ParsedExpression::EmptyList => {
            Some(ExpressionNode::List(List::new(Some(Vec::new()))))
        }
        _ => None,
    }
}

pub(super) fn merge_parse_section(into: &mut ParseSection, mut other: ParseSection) {
    into.nodes.append(&mut other.nodes);
    into.flows.append(&mut other.flows);
    into.global_declarations.append(&mut other.global_declarations);
    into.global_initializers.append(&mut other.global_initializers);
    into.const_declarations.append(&mut other.const_declarations);
    into.list_definitions.append(&mut other.list_definitions);
    into.external_declarations.append(&mut other.external_declarations);
}

pub(super) fn build_story_from_section(
    source: String,
    source_name: Option<String>,
    count_all_visits: bool,
    parsed: ParseSection,
) -> Story {
    let mut story = Story::new(&source, source_name, count_all_visits);
    story.root_nodes = parsed.nodes;
    story.flows = parsed.flows;
    story.global_declarations = parsed.global_declarations;
    story.global_initializers = parsed.global_initializers;
    story.const_declarations = parsed.const_declarations;
    story.list_definitions = parsed.list_definitions;
    story.external_declarations = parsed.external_declarations;
    story.rebuild_parse_tree_refs();
    story
}
