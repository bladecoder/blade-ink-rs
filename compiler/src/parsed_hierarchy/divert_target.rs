use crate::error::CompilerError;

use super::{Expression, ObjectKind, Story, ValidationScope};

#[derive(Debug, Clone)]
pub struct DivertTarget {
    expression: Expression,
    target_path: String,
}

impl DivertTarget {
    pub fn new(target_path: impl Into<String>) -> Self {
        Self {
            expression: Expression::new(ObjectKind::DivertTarget),
            target_path: target_path.into(),
        }
    }

    pub fn expression(&self) -> &Expression {
        &self.expression
    }

    pub fn target_path(&self) -> &str {
        &self.target_path
    }

    pub(super) fn validate_explicit_target(
        target: &str,
        scope: &ValidationScope,
        story: &Story,
    ) -> Result<(), CompilerError> {
        if target.contains('.') {
            return Ok(());
        }

        if scope.local_labels.contains(target) || scope.sibling_flow_names.contains(target) {
            return Ok(());
        }

        if story.has_flow_path(target) {
            return Ok(());
        }

        Err(CompilerError::invalid_source(format!(
            "Divert target not found: '{}'",
            target
        )))
    }

    pub(super) fn validate_target_name(
        target: &str,
        scope: &ValidationScope,
        story: &Story,
    ) -> Result<(), CompilerError> {
        if target == "END" || target == "DONE" {
            return Ok(());
        }

        if target.contains('.') {
            return Self::validate_explicit_target(target, scope, story);
        }

        if scope.local_labels.contains(target)
            || scope.sibling_flow_names.contains(target)
            || scope.top_level_flow_names.contains(target)
            || scope.all_flow_names.contains(target)
            || scope.divert_target_vars.contains(target)
        {
            return Ok(());
        }

        if scope.visible_vars.contains(target) {
            return Err(CompilerError::invalid_source(format!(
                "Since '{}' is used as a variable divert target, it should be marked as: -> {}",
                target, target
            )));
        }

        if story.resolve_list_item(target).is_some() {
            return Ok(());
        }

        Err(CompilerError::invalid_source(format!(
            "Divert target not found: '{}'",
            target
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::DivertTarget;

    #[test]
    fn divert_target_keeps_target_path() {
        let target = DivertTarget::new("knot.stitch");
        assert_eq!("knot.stitch", target.target_path());
    }
}
