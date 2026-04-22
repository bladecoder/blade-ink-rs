use crate::error::CompilerError;

use super::{
    DivertTarget, Expression, ExpressionNode, ObjectKind, ParsedExpression, Story,
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
                        ParsedExpression::Variable(variable_name) => {
                            if !scope.divert_target_vars.contains(variable_name) {
                                return Err(CompilerError::invalid_source(format!(
                                    "Since '{}' is used as a variable divert target, it should be marked as: -> {}",
                                    variable_name, variable_name
                                )));
                            }
                        }
                        ParsedExpression::DivertTarget(target) => {
                            if scope.divert_target_vars.contains(target) {
                                return Err(CompilerError::invalid_source(format!(
                                    "Can't pass '-> {}' to a parameter that already expects a divert target variable",
                                    target
                                )));
                            }
                            DivertTarget::validate_explicit_target(target, scope, story)?;
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
