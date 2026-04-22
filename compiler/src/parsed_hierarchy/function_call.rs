use std::rc::Rc;

use bladeink::{CommandType, Divert, InkList, PushPopType, RTObject, Value};

use crate::{
    error::CompilerError,
    runtime_export::{
        builtin_command, command, export_count_argument_expression, export_expression,
        native, native_function, rt_value, simple_function_text,
    },
};

use super::{
    DivertTarget, Expression, ExpressionNode, ObjectKind, ParsedExpression, ParsedPath, Story,
    ValidationScope,
};

#[derive(Debug, Clone)]
pub struct FunctionCall {
    expression: Expression,
    name: String,
    arguments: Vec<ExpressionNode>,
    should_pop_returned_value: bool,
}

impl FunctionCall {
    pub fn new(name: impl Into<String>, mut arguments: Vec<ExpressionNode>) -> Self {
        let mut expression = Expression::new(ObjectKind::FunctionCall);
        for argument in &mut arguments {
            argument.object_mut().set_parent(expression.object());
            expression
                .object_mut()
                .add_content_ref(argument.object().reference());
        }
        Self {
            expression,
            name: name.into(),
            arguments,
            should_pop_returned_value: false,
        }
    }

    pub fn expression(&self) -> &Expression {
        &self.expression
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn arguments(&self) -> &[ExpressionNode] {
        &self.arguments
    }

    pub fn should_pop_returned_value(&self) -> bool {
        self.should_pop_returned_value
    }

    pub fn set_should_pop_returned_value(&mut self, value: bool) {
        self.should_pop_returned_value = value;
    }

    pub(super) fn validate_call_arguments(
        target_name: &str,
        arguments: &[ParsedExpression],
        scope: &ValidationScope,
        story: &Story,
    ) -> Result<(), CompilerError> {
        if let Some(target_flow) = story.find_flow_by_name(target_name) {
            for (argument, parameter) in arguments.iter().zip(target_flow.flow().arguments().iter()) {
                if parameter.is_divert_target {
                    match argument {
                        ParsedExpression::Variable {
                            path: variable_name,
                            ..
                        } => {
                            if !scope.divert_target_vars.contains(variable_name.as_str()) {
                                return Err(CompilerError::invalid_source(format!(
                                    "Since '{}' is used as a variable divert target, it should be marked as: -> {}",
                                    variable_name, variable_name
                                )));
                            }
                        }
                        ParsedExpression::DivertTarget { target_path, .. } => {
                            if scope.divert_target_vars.contains(target_path.as_str()) {
                                return Err(CompilerError::invalid_source(format!(
                                    "Can't pass '-> {}' to a parameter that already expects a divert target variable",
                                    target_path.as_str()
                                )));
                            }
                            DivertTarget::validate_explicit_target(target_path.as_str(), scope, story)?;
                        }
                        _ => {
                            return Err(CompilerError::invalid_source(format!(
                                "Parameter '{}' expects a divert target",
                                parameter.identifier
                            )));
                        }
                    }
                } else {
                    argument.validate(scope, story)?;
                }
            }
            for argument in arguments.iter().skip(target_flow.flow().arguments().len()) {
                argument.validate(scope, story)?;
            }
        } else {
            for argument in arguments {
                argument.validate(scope, story)?;
            }
        }

        Ok(())
    }

    pub(crate) fn export_parsed_call(
        path: &ParsedPath,
        arguments: &[ParsedExpression],
        resolved_target: Option<super::ParsedObjectRef>,
        story: &Story,
        content: &mut Vec<Rc<dyn RTObject>>,
    ) -> Result<(), CompilerError> {
        if story
            .list_definitions()
            .iter()
            .any(|list| list.identifier() == Some(path.as_str()))
        {
            match arguments {
                [] => {
                    let list = InkList::new();
                    list.set_initial_origin_names(vec![path.as_str().to_owned()]);
                    content.push(rt_value(list));
                }
                [argument] => {
                    content.push(rt_value(path.as_str()));
                    export_expression(argument, story, content)?;
                    content.push(command(CommandType::ListFromInt));
                }
                _ => {
                    return Err(CompilerError::unsupported_feature(format!(
                        "runtime export does not support list call '{}' with {} arguments",
                        path.as_str(),
                        arguments.len()
                    )));
                }
            }
            return Ok(());
        }

        if let Some(command_type) = builtin_command(path.as_str()) {
            for argument in arguments {
                if matches!(path.as_str(), "TURNS_SINCE" | "READ_COUNT") {
                    export_count_argument_expression(argument, story, content)?;
                } else {
                    export_expression(argument, story, content)?;
                }
            }
            content.push(command(command_type));
            return Ok(());
        }

        if let Some(native_op) = native_function(path.as_str()) {
            for argument in arguments {
                export_expression(argument, story, content)?;
            }
            content.push(native(native_op));
            return Ok(());
        }

        if let Some(function_flow) = story.parsed_flows().iter().find(|flow| {
            flow.flow().identifier() == Some(path.as_str()) && flow.flow().is_function()
        }) {
            if arguments.is_empty() && let Some(text) = simple_function_text(function_flow) {
                content.push(rt_value(text.as_str()));
                return Ok(());
            }

            let params = function_flow.flow().arguments();
            if params.len() != arguments.len() {
                return Err(CompilerError::unsupported_feature(format!(
                    "runtime export function call '{}' has {} arguments but expected {}",
                    path.as_str(),
                    arguments.len(),
                    params.len()
                )));
            }

            for (argument, parameter) in arguments.iter().zip(params.iter()) {
                if parameter.is_by_reference {
                    let ParsedExpression::Variable { path: var_name, .. } = argument else {
                        return Err(CompilerError::unsupported_feature(format!(
                            "runtime export by-reference function call '{}' requires variable arguments",
                            path.as_str()
                        )));
                    };
                    content.push(Rc::new(Value::new_variable_pointer(var_name.as_str(), -1)));
                } else {
                    export_expression(argument, story, content)?;
                }
            }

            let target_path = resolved_target
                .and_then(|target_ref| story.runtime_target_cache_for_ref(target_ref))
                .and_then(|cache| cache.runtime_path())
                .or_else(|| function_flow.runtime_path())
                .map(|path| path.to_string())
                .unwrap_or_else(|| path.as_str().to_owned());

            content.push(Rc::new(Divert::new(
                true,
                PushPopType::Function,
                false,
                0,
                false,
                None,
                Some(&target_path),
            )));
            return Ok(());
        }

        Err(CompilerError::unsupported_feature(format!(
            "runtime export does not support function call '{}'",
            path.as_str()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::FunctionCall;
    use crate::parsed_hierarchy::{ExpressionNode, Number, NumberValue};

    #[test]
    fn function_call_tracks_arguments_and_pop_flag() {
        let mut call = FunctionCall::new(
            "my_func",
            vec![ExpressionNode::Number(Number::new(NumberValue::Int(1)))],
        );
        call.set_should_pop_returned_value(true);
        assert_eq!("my_func", call.name());
        assert_eq!(1, call.arguments().len());
        assert!(call.should_pop_returned_value());
    }
}
