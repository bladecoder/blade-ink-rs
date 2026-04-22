use super::{ExpressionNode, ObjectKind, ParsedObject};

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

#[cfg(test)]
mod tests {
    use super::Return;

    #[test]
    fn return_can_be_void() {
        let ret = Return::new(None);
        assert!(ret.returned_expression().is_none());
    }
}
