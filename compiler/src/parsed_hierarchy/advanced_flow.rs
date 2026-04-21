use super::{Expression, ExpressionNode, FlowArgument, ObjectKind, ParsedObject, Story};

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
}

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
}

#[derive(Debug, Clone)]
pub struct Return {
    object: ParsedObject,
    returned_expression: Option<ExpressionNode>,
}

impl Return {
    pub fn new(mut returned_expression: Option<ExpressionNode>) -> Self {
        let mut object = ParsedObject::new(ObjectKind::Return);
        if let Some(returned_expression) = returned_expression.as_mut() {
            returned_expression.object_mut().set_parent(&object);
            object.add_content_ref(returned_expression.object().reference());
        }
        Self {
            object,
            returned_expression,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn returned_expression(&self) -> Option<&ExpressionNode> {
        self.returned_expression.as_ref()
    }
}

#[derive(Debug, Clone)]
pub struct IncludedFile {
    object: ParsedObject,
    included_story: Option<Box<Story>>,
    filename: Option<String>,
}

impl IncludedFile {
    pub fn new(included_story: Option<Story>, filename: Option<String>) -> Self {
        let mut object = ParsedObject::new(ObjectKind::IncludedFile);
        let mut included_story = included_story.map(Box::new);
        if let Some(included_story) = included_story.as_mut() {
            included_story.object_mut().set_parent(&object);
            object.add_content_ref(included_story.object().reference());
        }
        Self {
            object,
            included_story,
            filename,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn included_story(&self) -> Option<&Story> {
        self.included_story.as_deref()
    }

    pub fn filename(&self) -> Option<&str> {
        self.filename.as_deref()
    }
}

#[derive(Debug, Clone)]
pub struct TunnelOnwards {
    object: ParsedObject,
    divert_after: Option<String>,
}

impl TunnelOnwards {
    pub fn new(divert_after: Option<String>) -> Self {
        Self {
            object: ParsedObject::new(ObjectKind::TunnelOnwards),
            divert_after,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn divert_after(&self) -> Option<&str> {
        self.divert_after.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowDecl {
    pub name: String,
    pub arguments: Vec<FlowArgument>,
    pub is_function: bool,
}

#[cfg(test)]
mod tests {
    use super::{DivertTarget, FunctionCall, IncludedFile, Return, TunnelOnwards};
    use crate::parsed_hierarchy::{ExpressionNode, Number, NumberValue, Story};

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

    #[test]
    fn divert_target_keeps_target_path() {
        let target = DivertTarget::new("knot.stitch");
        assert_eq!("knot.stitch", target.target_path());
    }

    #[test]
    fn return_can_be_void() {
        let ret = Return::new(None);
        assert!(ret.returned_expression().is_none());
    }

    #[test]
    fn included_file_tracks_story_and_filename() {
        let story = Story::new("content", Some("included.ink".to_owned()), true);
        let included = IncludedFile::new(Some(story), Some("included.ink".to_owned()));
        assert_eq!(Some("included.ink"), included.filename());
        assert!(included.included_story().is_some());
    }

    #[test]
    fn tunnel_onwards_can_hold_override_target() {
        let tunnel = TunnelOnwards::new(Some("next".to_owned()));
        assert_eq!(Some("next"), tunnel.divert_after());
    }
}
