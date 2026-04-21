use std::{collections::HashMap, rc::Rc};

use bladeink::{
    CommandType, Container, ControlCommand, Divert, Glue, InkList, InkListItem, ListDefinition,
    NativeFunctionCall, NativeOp, Path, PushPopType, RTObject, Value, VariableAssignment,
    VariableReference, story::Story as RuntimeStory,
};

use crate::{
    error::CompilerError,
    parsed_hierarchy::{
        ListDefinition as ParsedListDefinition, ParsedExpression, ParsedFlow, ParsedNode,
        ParsedNodeKind, Story,
    },
};

pub(crate) fn export_story(story: &Story) -> Result<RuntimeStory, CompilerError> {
    let list_defs = export_list_defs(story.list_definitions());
    let mut named_content = HashMap::new();

    for flow in story.parsed_flows() {
        let name = flow.flow().identifier().unwrap_or_default().to_owned();
        named_content.insert(name, export_flow(flow, story)?);
    }

    if let Some(global_decl) = export_global_decl(story)? {
        named_content.insert("global decl".to_owned(), global_decl);
    }

    let mut root_content = export_nodes(story.root_nodes(), Scope::Root, story)?;
    if !has_terminal(story.root_nodes()) {
        root_content.push(named_container(
            "g-0",
            vec![command(CommandType::Done)],
            HashMap::new(),
            0,
        ));
    }

    let inner_root = container(None, root_content, HashMap::new(), 0);
    let root = Container::new(
        None,
        0,
        vec![inner_root, command(CommandType::Done)],
        named_content,
    );

    RuntimeStory::from_compiled(root, list_defs)
        .map_err(|error| CompilerError::invalid_source(error.to_string()))
}

fn export_list_defs(list_definitions: &[ParsedListDefinition]) -> Vec<ListDefinition> {
    list_definitions
        .iter()
        .filter_map(|definition| {
            let name = definition.identifier()?.to_owned();
            let items = definition
                .item_definitions()
                .iter()
                .map(|item| (item.name().to_owned(), item.series_value()))
                .collect();
            Some(ListDefinition::new(name, items))
        })
        .collect()
}

fn export_global_decl(story: &Story) -> Result<Option<Rc<Container>>, CompilerError> {
    if story.global_initializers().is_empty() && story.list_definitions().is_empty() {
        return Ok(None);
    }

    let mut content = vec![command(CommandType::EvalStart)];

    for list in story.list_definitions() {
        content.push(list_value_from_definition(list));
        if let Some(name) = list.identifier() {
            content.push(variable_assignment(name, true, true));
        }
    }

    for (name, expression) in story.global_initializers() {
        export_expression(expression, story, &mut content)?;
        content.push(variable_assignment(name, true, true));
    }

    content.push(command(CommandType::EvalEnd));
    content.push(command(CommandType::End));
    Ok(Some(Container::new(
        Some("global decl".to_owned()),
        0,
        content,
        HashMap::new(),
    )))
}

fn list_value_from_definition(definition: &ParsedListDefinition) -> Rc<dyn RTObject> {
    let mut list = InkList::new();
    if let Some(name) = definition.identifier() {
        list.set_initial_origin_names(vec![name.to_owned()]);
        for item in definition.item_definitions() {
            if item.in_initial_list() {
                list.items.insert(
                    InkListItem::new(Some(name.to_owned()), item.name().to_owned()),
                    item.series_value(),
                );
            }
        }
    }
    rt_value(list)
}

fn export_flow(flow: &ParsedFlow, story: &Story) -> Result<Rc<Container>, CompilerError> {
    if flow.flow().has_parameters() || flow.flow().is_function() {
        return Err(CompilerError::unsupported_feature(format!(
            "runtime export does not support parameterised/function flow '{}'",
            flow.flow().identifier().unwrap_or_default()
        )));
    }

    let mut content = if flow.content().is_empty() && !flow.children().is_empty() {
        vec![divert_object(&format!(
            "{}.{}",
            flow.flow().identifier().unwrap_or_default(),
            flow.children()[0].flow().identifier().unwrap_or_default()
        ))]
    } else {
        export_nodes(flow.content(), Scope::Flow(flow), story)?
    };

    if !flow.content().is_empty() && !has_terminal(flow.content()) {
        content.push(command(CommandType::Done));
    }

    let mut named = HashMap::new();
    for child in flow.children() {
        named.insert(
            child.flow().identifier().unwrap_or_default().to_owned(),
            export_flow(child, story)?,
        );
    }

    Ok(Container::new(
        Some(flow.flow().identifier().unwrap_or_default().to_owned()),
        visit_count_flags(story.count_all_visits),
        content,
        named,
    ))
}

#[derive(Clone, Copy)]
enum Scope<'a> {
    Root,
    Flow(&'a ParsedFlow),
}

fn export_nodes(
    nodes: &[ParsedNode],
    scope: Scope<'_>,
    story: &Story,
) -> Result<Vec<Rc<dyn RTObject>>, CompilerError> {
    let mut content = Vec::new();

    for node in nodes {
        match node.kind() {
            ParsedNodeKind::Text => {
                if let Some(text) = node.text()
                    && !text.is_empty()
                {
                    content.push(rt_value(text));
                }
            }
            ParsedNodeKind::Newline => content.push(rt_value("\n")),
            ParsedNodeKind::Glue => content.push(Rc::new(Glue::new())),
            ParsedNodeKind::Divert => {
                let target = node.target().ok_or_else(|| {
                    CompilerError::unsupported_feature("runtime export divert missing target")
                })?;
                content.push(export_divert(target, scope, story));
            }
            ParsedNodeKind::OutputExpression => {
                let expression = node.expression().ok_or_else(|| {
                    CompilerError::unsupported_feature(
                        "runtime export output expression missing expression",
                    )
                })?;
                content.push(command(CommandType::EvalStart));
                export_expression(expression, story, &mut content)?;
                content.push(command(CommandType::EvalOutput));
                content.push(command(CommandType::EvalEnd));
            }
            ParsedNodeKind::Assignment => export_assignment(node, story, &mut content)?,
            ParsedNodeKind::Tag => {
                return Err(CompilerError::unsupported_feature(
                    "runtime export does not support tags yet",
                ));
            }
            ParsedNodeKind::Choice
            | ParsedNodeKind::GatherPoint
            | ParsedNodeKind::GatherLabel
            | ParsedNodeKind::Sequence
            | ParsedNodeKind::TunnelDivert
            | ParsedNodeKind::TunnelReturn
            | ParsedNodeKind::TunnelOnwardsWithTarget
            | ParsedNodeKind::Conditional
            | ParsedNodeKind::SwitchConditional
            | ParsedNodeKind::ThreadDivert
            | ParsedNodeKind::ReturnBool
            | ParsedNodeKind::ReturnExpression
            | ParsedNodeKind::ReturnVoid
            | ParsedNodeKind::VoidCall => {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support {:?} yet",
                    node.kind()
                )));
            }
        }
    }

    Ok(content)
}

fn export_divert(target: &str, scope: Scope<'_>, story: &Story) -> Rc<dyn RTObject> {
    match target {
        "END" => command(CommandType::End),
        "DONE" => command(CommandType::Done),
        _ => {
            let resolved = resolve_target(target, scope, story);
            if is_global_variable_divert(&resolved, story) {
                Rc::new(Divert::new(
                    false,
                    PushPopType::Tunnel,
                    false,
                    0,
                    false,
                    Some(resolved),
                    None,
                ))
            } else {
                divert_object(&resolved)
            }
        }
    }
}

fn is_global_variable_divert(target: &str, story: &Story) -> bool {
    story
        .global_initializers()
        .iter()
        .any(|(name, _)| name == target)
        && !story
            .parsed_flows()
            .iter()
            .any(|flow| flow.flow().identifier() == Some(target))
}

fn resolve_target(target: &str, scope: Scope<'_>, story: &Story) -> String {
    if target.contains('.') {
        return target.to_owned();
    }

    let Scope::Flow(flow) = scope else {
        return target.to_owned();
    };

    if flow
        .children()
        .iter()
        .any(|child| child.flow().identifier() == Some(target))
    {
        return format!(
            "{}.{target}",
            flow.flow().identifier().unwrap_or_default()
        );
    }

    if story
        .parsed_flows()
        .iter()
        .any(|candidate| candidate.flow().identifier() == Some(target))
    {
        return target.to_owned();
    }

    target.to_owned()
}

fn export_assignment(
    node: &ParsedNode,
    story: &Story,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    let encoded = node.name().ok_or_else(|| {
        CompilerError::unsupported_feature("runtime export assignment missing target")
    })?;
    let (mode, name) = encoded.split_once(':').ok_or_else(|| {
        CompilerError::unsupported_feature(format!(
            "runtime export assignment target has invalid shape '{encoded}'"
        ))
    })?;
    let expression = node.expression().ok_or_else(|| {
        CompilerError::unsupported_feature("runtime export assignment missing expression")
    })?;

    match mode {
        "Set" => {
            content.push(command(CommandType::EvalStart));
            export_expression(expression, story, content)?;
            content.push(command(CommandType::EvalEnd));
            content.push(variable_assignment(name, true, false));
        }
        "TempSet" => {
            content.push(command(CommandType::EvalStart));
            export_expression(expression, story, content)?;
            content.push(command(CommandType::EvalEnd));
            content.push(variable_assignment(name, false, true));
        }
        "AddAssign" => {
            content.push(command(CommandType::EvalStart));
            content.push(Rc::new(VariableReference::new(name)));
            export_expression(expression, story, content)?;
            content.push(native(NativeOp::Add));
            content.push(variable_assignment(name, true, false));
            content.push(command(CommandType::EvalEnd));
        }
        "SubtractAssign" => {
            content.push(command(CommandType::EvalStart));
            content.push(Rc::new(VariableReference::new(name)));
            export_expression(expression, story, content)?;
            content.push(native(NativeOp::Subtract));
            content.push(variable_assignment(name, true, false));
            content.push(command(CommandType::EvalEnd));
        }
        other => {
            return Err(CompilerError::unsupported_feature(format!(
                "runtime export does not support assignment mode '{other}'"
            )));
        }
    }

    Ok(())
}

fn export_expression_node(
    expression: &crate::parsed_hierarchy::ExpressionNode,
    story: &Story,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    use crate::parsed_hierarchy::{ExpressionNode, NumberValue};

    match expression {
        ExpressionNode::Number(number) => match number.value() {
            NumberValue::Int(value) => content.push(rt_value(*value)),
            NumberValue::Float(value) => content.push(rt_value(*value)),
            NumberValue::Bool(value) => content.push(rt_value(*value)),
        },
        ExpressionNode::StringExpression(string) => {
            content.push(command(CommandType::BeginString));
            for item in string.content().content() {
                let crate::parsed_hierarchy::Content::Text(text) = item;
                content.push(rt_value(text.text()));
            }
            content.push(command(CommandType::EndString));
        }
        ExpressionNode::VariableReference(reference) => {
            let name = reference.name();
            if let Some(constant) = story.const_declaration(&name) {
                export_expression_node(constant.expression(), story, content)?;
            } else {
                content.push(Rc::new(VariableReference::new(&name)));
            }
        }
        ExpressionNode::List(list) => {
            let mut runtime_list = InkList::new();
            if let Some(list_items) = list.item_identifier_list() {
                for item in list_items {
                    insert_resolved_list_item(story, item, &mut runtime_list)?;
                }
            }
            content.push(rt_value(runtime_list));
        }
    }
    Ok(())
}

fn export_expression(
    expression: &ParsedExpression,
    story: &Story,
    content: &mut Vec<Rc<dyn RTObject>>,
) -> Result<(), CompilerError> {
    match expression {
        ParsedExpression::Bool(value) => content.push(rt_value(*value)),
        ParsedExpression::Int(value) => content.push(rt_value(*value)),
        ParsedExpression::Float(value) => content.push(rt_value(*value)),
        ParsedExpression::String(value) => {
            content.push(command(CommandType::BeginString));
            content.push(rt_value(value.as_str()));
            content.push(command(CommandType::EndString));
        }
        ParsedExpression::Variable(name) => {
            if let Some(constant) = story.const_declaration(name) {
                export_expression_node(constant.expression(), story, content)?;
            } else {
                content.push(Rc::new(VariableReference::new(name)));
            }
        }
        ParsedExpression::DivertTarget(target) => {
            content.push(rt_value(Path::new_with_components_string(Some(target))));
        }
        ParsedExpression::ListItems(items) => {
            let mut list = InkList::new();
            for item in items {
                insert_resolved_list_item(story, item, &mut list)?;
            }
            content.push(rt_value(list));
        }
        ParsedExpression::EmptyList => content.push(rt_value(InkList::new())),
        ParsedExpression::Unary { operator, expression } => match operator.as_str() {
            "-" => match expression.as_ref() {
                ParsedExpression::Int(value) => content.push(rt_value(-value)),
                ParsedExpression::Float(value) => content.push(rt_value(-value)),
                other => {
                    export_expression(other, story, content)?;
                    content.push(native(NativeOp::Negate));
                }
            },
            "!" => {
                export_expression(expression, story, content)?;
                content.push(native(NativeOp::Not));
            }
            _ => {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support unary operator '{operator}'"
                )));
            }
        },
        ParsedExpression::Binary {
            left,
            operator,
            right,
        } => {
            export_expression(left, story, content)?;
            export_expression(right, story, content)?;
            content.push(native(operator_token(operator)?));
        }
        ParsedExpression::FunctionCall { name, arguments } => {
            if story
                .list_definitions()
                .iter()
                .any(|list| list.identifier() == Some(name.as_str()))
            {
                match arguments.as_slice() {
                    [] => {
                        let list = InkList::new();
                        list.set_initial_origin_names(vec![name.to_owned()]);
                        content.push(rt_value(list));
                    }
                    [argument] => {
                        content.push(rt_value(name.as_str()));
                        export_expression(argument, story, content)?;
                        content.push(command(CommandType::ListFromInt));
                    }
                    _ => {
                        return Err(CompilerError::unsupported_feature(format!(
                            "runtime export does not support list call '{name}' with {} arguments",
                            arguments.len()
                        )));
                    }
                }
            } else if let Some(command_type) = builtin_command(name) {
                for argument in arguments {
                    export_expression(argument, story, content)?;
                }
                content.push(command(command_type));
            } else if let Some(native_op) = native_function(name) {
                for argument in arguments {
                    export_expression(argument, story, content)?;
                }
                content.push(native(native_op));
            } else {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export does not support function call '{name}'"
                )));
            }
        }
    }

    Ok(())
}

fn insert_resolved_list_item(
    story: &Story,
    item: &str,
    list: &mut InkList,
) -> Result<(), CompilerError> {
    if let Some((qualified, value)) = story.resolve_list_item(item) {
        list.items.insert(InkListItem::from_full_name(&qualified), value);
        Ok(())
    } else {
        Err(CompilerError::unsupported_feature(format!(
            "runtime export cannot resolve list item '{item}'"
        )))
    }
}

fn operator_token(operator: &str) -> Result<NativeOp, CompilerError> {
    match operator {
        "Add" => Ok(NativeOp::Add),
        "Subtract" => Ok(NativeOp::Subtract),
        "Multiply" => Ok(NativeOp::Multiply),
        "Divide" => Ok(NativeOp::Divide),
        "Modulo" => Ok(NativeOp::Mod),
        "Equal" => Ok(NativeOp::Equal),
        "NotEqual" => Ok(NativeOp::NotEquals),
        "Less" => Ok(NativeOp::Less),
        "LessEqual" => Ok(NativeOp::LessThanOrEquals),
        "Greater" => Ok(NativeOp::Greater),
        "GreaterEqual" => Ok(NativeOp::GreaterThanOrEquals),
        "And" => Ok(NativeOp::And),
        "Or" => Ok(NativeOp::Or),
        "Has" => Ok(NativeOp::Has),
        "Hasnt" => Ok(NativeOp::Hasnt),
        "Intersect" => Ok(NativeOp::Intersect),
        other => Err(CompilerError::unsupported_feature(format!(
            "runtime export does not support binary operator '{other}'"
        ))),
    }
}

fn native_function(name: &str) -> Option<NativeOp> {
    match name {
        "LIST_VALUE" => Some(NativeOp::ValueOfList),
        "LIST_COUNT" => Some(NativeOp::Count),
        "LIST_MIN" => Some(NativeOp::ListMin),
        "LIST_MAX" => Some(NativeOp::ListMax),
        "LIST_ALL" => Some(NativeOp::All),
        "LIST_INVERT" => Some(NativeOp::Invert),
        _ => None,
    }
}

fn builtin_command(name: &str) -> Option<CommandType> {
    match name {
        "LIST_RANGE" => Some(CommandType::ListRange),
        "LIST_RANDOM" => Some(CommandType::ListRandom),
        _ => None,
    }
}

fn has_terminal(nodes: &[ParsedNode]) -> bool {
    matches!(
        nodes.last().map(|node| node.kind()),
        Some(ParsedNodeKind::Divert | ParsedNodeKind::TunnelReturn)
    )
}

fn container(
    name: Option<String>,
    content: Vec<Rc<dyn RTObject>>,
    named_content: HashMap<String, Rc<Container>>,
    count_flags: i32,
) -> Rc<dyn RTObject> {
    Container::new(name, count_flags, content, named_content)
}

fn named_container(
    name: &str,
    content: Vec<Rc<dyn RTObject>>,
    named_content: HashMap<String, Rc<Container>>,
    count_flags: i32,
) -> Rc<dyn RTObject> {
    Container::new(Some(name.to_owned()), count_flags, content, named_content)
}

fn command(command_type: CommandType) -> Rc<dyn RTObject> {
    Rc::new(ControlCommand::new(command_type))
}

fn rt_value<T: Into<Value>>(value: T) -> Rc<dyn RTObject> {
    Rc::new(Value::new(value))
}

fn native(op: NativeOp) -> Rc<dyn RTObject> {
    Rc::new(NativeFunctionCall::new(op))
}

fn variable_assignment(name: &str, is_global: bool, is_new_declaration: bool) -> Rc<dyn RTObject> {
    Rc::new(VariableAssignment::new(
        name,
        is_new_declaration,
        is_global,
    ))
}

fn divert_object(target: &str) -> Rc<dyn RTObject> {
    Rc::new(Divert::new(
        false,
        PushPopType::Tunnel,
        false,
        0,
        false,
        None,
        Some(target),
    ))
}

fn visit_count_flags(count_all_visits: bool) -> i32 {
    if count_all_visits { 1 } else { 0 }
}
