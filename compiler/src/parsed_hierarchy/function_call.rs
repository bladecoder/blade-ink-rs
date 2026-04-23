use std::rc::Rc;

use bladeink::{CommandType, Divert, InkList, PushPopType, RTObject, Value};

use crate::{
    error::CompilerError,
    runtime_export::{
        builtin_command, command, export_count_argument_expression, export_expression,
        native, native_function, rt_value, simple_function_text,
    },
};

use super::{DivertTarget, Expression, ObjectKind, ParsedExpression, ParsedObjectRef, ParsedPath, Story, ValidationScope};

#[derive(Debug, Clone)]
pub struct FunctionCallProxyDivert {
    path: ParsedPath,
    resolved_target: Option<ParsedObjectRef>,
}

impl FunctionCallProxyDivert {
    pub fn new(path: impl Into<ParsedPath>) -> Self {
        Self {
            path: path.into(),
            resolved_target: None,
        }
    }

    pub fn path(&self) -> &ParsedPath {
        &self.path
    }

    pub fn name(&self) -> &str {
        self.path.as_str()
    }

    pub fn resolved_target(&self) -> Option<ParsedObjectRef> {
        self.resolved_target
    }

    pub fn resolve_targets(&mut self, story: &Story) {
        self.resolved_target = story.find_flow_by_name(self.name()).map(|flow| flow.reference());
    }

    pub fn runtime_target_path(&self, story: &Story) -> Option<String> {
        self.resolved_target
            .and_then(|target_ref| story.runtime_target_cache_for_ref(target_ref))
            .and_then(|cache| cache.runtime_path())
            .map(|path| path.to_string())
    }

    pub fn resolves_variable_target(&self) -> bool {
        self.resolved_target.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct FunctionCall {
    expression: Expression,
    proxy_divert: FunctionCallProxyDivert,
    arguments: Vec<ParsedExpression>,
    should_pop_returned_value: bool,
}

impl FunctionCall {
    pub fn new(path: impl Into<ParsedPath>, arguments: Vec<ParsedExpression>) -> Self {
        Self {
            expression: Expression::new(ObjectKind::FunctionCall),
            proxy_divert: FunctionCallProxyDivert::new(path),
            arguments,
            should_pop_returned_value: false,
        }
    }

    pub fn expression(&self) -> &Expression {
        &self.expression
    }

    pub fn path(&self) -> &ParsedPath {
        self.proxy_divert.path()
    }

    pub fn name(&self) -> &str {
        self.proxy_divert.name()
    }

    pub fn arguments(&self) -> &[ParsedExpression] {
        &self.arguments
    }

    pub fn resolved_target(&self) -> Option<ParsedObjectRef> {
        self.proxy_divert.resolved_target()
    }

    pub fn proxy_divert(&self) -> &FunctionCallProxyDivert {
        &self.proxy_divert
    }

    pub fn resolve_targets(&mut self, story: &Story) {
        self.proxy_divert.resolve_targets(story);
        for argument in &mut self.arguments {
            argument.resolve_targets(story);
        }
    }

    pub fn should_pop_returned_value(&self) -> bool {
        self.should_pop_returned_value
    }

    pub fn set_should_pop_returned_value(&mut self, value: bool) {
        self.should_pop_returned_value = value;
    }

    pub fn is_builtin(&self) -> bool {
        Self::is_builtin_name(self.name())
    }

    pub fn is_turns_since(&self) -> bool {
        self.name() == "TURNS_SINCE"
    }

    pub fn is_read_count(&self) -> bool {
        self.name() == "READ_COUNT"
    }

    pub fn is_count_function(&self) -> bool {
        self.is_turns_since() || self.is_read_count()
    }

    pub(crate) fn validate(&self, scope: &ValidationScope, story: &Story) -> Result<(), CompilerError> {
        Self::validate_call_arguments(self.name(), self.arguments(), scope, story)
    }

    pub fn apply_counting_marks(&self, story: &mut Story) {
        if self.is_count_function()
            && let Some(argument) = self.arguments().first()
            && let Some(target) = argument.resolved_target().or_else(|| argument.resolved_count_target())
        {
            story.mark_count_target(target, self.is_turns_since());
        }
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
                        ParsedExpression::Variable(variable_name) => {
                            if !scope.divert_target_vars.contains(variable_name.path().as_str()) {
                                return Err(CompilerError::invalid_source(format!(
                                    "Since '{}' is used as a variable divert target, it should be marked as: -> {}",
                                    variable_name.path().as_str(), variable_name.path().as_str()
                                )));
                            }
                        }
                        ParsedExpression::DivertTarget(target_path) => {
                            if scope.divert_target_vars.contains(target_path.target_path().as_str()) {
                                return Err(CompilerError::invalid_source(format!(
                                    "Can't pass '-> {}' to a parameter that already expects a divert target variable",
                                    target_path.target_path().as_str()
                                )));
                            }
                            DivertTarget::validate_explicit_target(target_path.target_path().as_str(), scope, story)?;
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

    pub(crate) fn export_runtime(
        &self,
        story: &Story,
        content: &mut Vec<Rc<dyn RTObject>>,
    ) -> Result<(), CompilerError> {
        Self::export_parsed_call(self.proxy_divert(), self.arguments(), story, content)
    }

    pub(crate) fn export_parsed_call(
        proxy_divert: &FunctionCallProxyDivert,
        arguments: &[ParsedExpression],
        story: &Story,
        content: &mut Vec<Rc<dyn RTObject>>,
    ) -> Result<(), CompilerError> {
        let path = proxy_divert.path();

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
                    let ParsedExpression::Variable(var_name) = argument else {
                        return Err(CompilerError::unsupported_feature(format!(
                            "runtime export by-reference function call '{}' requires variable arguments",
                            path.as_str()
                        )));
                    };
                    content.push(Rc::new(Value::new_variable_pointer(var_name.path().as_str(), -1)));
                } else {
                    export_expression(argument, story, content)?;
                }
            }

            let target_path = proxy_divert
                .runtime_target_path(story)
                .or_else(|| function_flow.runtime_path().map(|path| path.to_string()))
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

    pub fn is_builtin_name(name: &str) -> bool {
        builtin_command(name).is_some() || native_function(name).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::FunctionCall;
    use crate::parsed_hierarchy::ParsedExpression;

    #[test]
    fn function_call_tracks_arguments_and_pop_flag() {
        let mut call = FunctionCall::new("my_func", vec![ParsedExpression::Int(1)]);
        call.set_should_pop_returned_value(true);
        assert_eq!("my_func", call.name());
        assert_eq!(1, call.arguments().len());
        assert!(call.should_pop_returned_value());
    }
}
