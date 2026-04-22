use std::rc::Rc;

use bladeink::{RTObject, VariableReference as RuntimeVariableReference};

use crate::error::CompilerError;

use super::{Expression, ObjectKind, ParsedPath, Story, ValidationScope};

#[derive(Debug, Clone)]
pub struct VariableReference {
    pub(crate) expression: Expression,
    pub(crate) path: ParsedPath,
}

impl VariableReference {
    pub fn new(path: impl Into<ParsedPath>) -> Self {
        Self {
            expression: Expression::new(ObjectKind::VariableReference),
            path: path.into(),
        }
    }

    pub fn expression(&self) -> &Expression {
        &self.expression
    }

    pub fn expression_mut(&mut self) -> &mut Expression {
        &mut self.expression
    }

    pub fn path(&self) -> &ParsedPath {
        &self.path
    }

    pub fn name(&self) -> String {
        self.path.as_str().to_owned()
    }

    pub fn runtime_object(&self) -> Rc<dyn RTObject> {
        if let Some(runtime_object) = self.expression.object().runtime_object() {
            return runtime_object;
        }

        let runtime_object: Rc<dyn RTObject> = Rc::new(RuntimeVariableReference::new(&self.name()));
        self.expression.object().set_runtime_object(runtime_object.clone());
        runtime_object
    }

    pub(super) fn validate_name(
        name: &str,
        scope: &ValidationScope,
        story: &Story,
    ) -> Result<(), CompilerError> {
        if name.contains('.') {
            return Ok(());
        }

        if story.has_named_label(name) {
            return Ok(());
        }

        if !scope.visible_vars.contains(name)
            && !scope.local_labels.contains(name)
            && !scope.sibling_flow_names.contains(name)
            && !scope.top_level_flow_names.contains(name)
            && !story
                .list_definitions()
                .iter()
                .any(|list| list.identifier() == Some(name))
            && story.resolve_list_item(name).is_none()
        {
            return Err(CompilerError::invalid_source(format!(
                "Variable or read count '{}' not found in this scope",
                name
            )));
        }

        Ok(())
    }
}
