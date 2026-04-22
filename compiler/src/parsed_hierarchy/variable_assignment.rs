use std::collections::HashSet;

use bladeink::{CommandType, NativeOp, RTObject, VariableReference as RuntimeVariableReference};

use crate::{
    error::CompilerError,
    runtime_export::{
        Scope, command, export_expression, expression_contains_function_call, native, rt_value,
        variable_assignment as runtime_variable_assignment, variable_is_temporary_in_scope,
    },
};

use super::{
    ExpressionNode, ObjectKind, ParsedAssignmentMode, ParsedNode, ParsedNodeKind, ParsedObject,
    Story,
};

#[derive(Debug, Clone)]
pub struct VariableAssignment {
    pub(crate) object: ParsedObject,
    pub(crate) variable_name: String,
    pub(crate) expression: Option<ExpressionNode>,
    pub(crate) is_global_declaration: bool,
    pub(crate) is_new_temporary_declaration: bool,
}

impl VariableAssignment {
    pub fn new(variable_name: impl Into<String>, mut expression: Option<ExpressionNode>) -> Self {
        let mut object = ParsedObject::new(ObjectKind::VariableAssignment);
        if let Some(expression) = expression.as_mut() {
            expression.object_mut().set_parent(&object);
            object.add_content_ref(expression.object().reference());
        }
        Self {
            object,
            variable_name: variable_name.into(),
            expression,
            is_global_declaration: false,
            is_new_temporary_declaration: false,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn variable_name(&self) -> &str {
        &self.variable_name
    }

    pub fn expression(&self) -> Option<&ExpressionNode> {
        self.expression.as_ref()
    }

    pub fn is_global_declaration(&self) -> bool {
        self.is_global_declaration
    }

    pub fn set_global_declaration(&mut self, value: bool) {
        self.is_global_declaration = value;
    }

    pub fn is_new_temporary_declaration(&self) -> bool {
        self.is_new_temporary_declaration
    }

    pub fn set_new_temporary_declaration(&mut self, value: bool) {
        self.is_new_temporary_declaration = value;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AssignmentNode<'a> {
    node: &'a ParsedNode,
}

impl<'a> AssignmentNode<'a> {
    pub fn from_node(node: &'a ParsedNode) -> Option<Self> {
        (node.kind() == ParsedNodeKind::Assignment).then_some(Self { node })
    }

    pub fn mode(self) -> Option<ParsedAssignmentMode> {
        self.node.assignment_mode()
    }

    pub fn target(self) -> Option<&'a str> {
        self.node.assignment_target()
    }

    pub fn collect_temp_var(self, names: &mut HashSet<String>) {
        if self.mode() == Some(ParsedAssignmentMode::TempSet)
            && let Some(name) = self.target()
        {
            names.insert(name.to_owned());
        }
    }

    pub fn collect_global_declared_var(self, names: &mut HashSet<String>) {
        if self.mode() == Some(ParsedAssignmentMode::GlobalDecl)
            && let Some(name) = self.target()
        {
            names.insert(name.to_owned());
        }
    }

    pub(crate) fn export_runtime(
        self,
        scope: Scope<'_>,
        story: &Story,
        content: &mut Vec<std::rc::Rc<dyn RTObject>>,
    ) -> Result<(), CompilerError> {
        let mode = self.mode().ok_or_else(|| {
            CompilerError::unsupported_feature("runtime export assignment missing mode")
        })?;
        let name = self.target().ok_or_else(|| {
            CompilerError::unsupported_feature("runtime export assignment missing target")
        })?;
        let expression = self.node.expression().ok_or_else(|| {
            CompilerError::unsupported_feature("runtime export assignment missing expression")
        })?;
        let is_temporary = variable_is_temporary_in_scope(name, scope);

        match mode {
            ParsedAssignmentMode::Set => {
                content.push(command(CommandType::EvalStart));
                export_expression(expression, story, content)?;
                content.push(command(CommandType::EvalEnd));
                content.push(runtime_variable_assignment(name, !is_temporary, false));
            }
            ParsedAssignmentMode::GlobalDecl => {
                content.push(command(CommandType::EvalStart));
                export_expression(expression, story, content)?;
                content.push(command(CommandType::EvalEnd));
                content.push(runtime_variable_assignment(name, !is_temporary, true));
            }
            ParsedAssignmentMode::TempSet => {
                content.push(command(CommandType::EvalStart));
                export_expression(expression, story, content)?;
                content.push(command(CommandType::EvalEnd));
                content.push(runtime_variable_assignment(name, false, true));
            }
            ParsedAssignmentMode::AddAssign => {
                content.push(command(CommandType::EvalStart));
                content.push(std::rc::Rc::new(RuntimeVariableReference::new(name)));
                export_expression(expression, story, content)?;
                content.push(native(NativeOp::Add));
                content.push(runtime_variable_assignment(name, !is_temporary, false));
                content.push(command(CommandType::EvalEnd));
            }
            ParsedAssignmentMode::SubtractAssign => {
                content.push(command(CommandType::EvalStart));
                content.push(std::rc::Rc::new(RuntimeVariableReference::new(name)));
                export_expression(expression, story, content)?;
                content.push(native(NativeOp::Subtract));
                content.push(runtime_variable_assignment(name, !is_temporary, false));
                content.push(command(CommandType::EvalEnd));
            }
        }

        if expression_contains_function_call(expression) {
            content.push(rt_value("\n"));
        }

        Ok(())
    }
}

impl ParsedNode {
    pub fn as_assignment(&self) -> Option<AssignmentNode<'_>> {
        AssignmentNode::from_node(self)
    }
}
