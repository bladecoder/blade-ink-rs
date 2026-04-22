use std::rc::Rc;

use bladeink::{RTObject, Value};

use super::{ObjectKind, ParsedObject};

#[derive(Debug, Clone)]
pub struct Text {
    object: ParsedObject,
    text: String,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            object: ParsedObject::new(ObjectKind::Text),
            text: text.into(),
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        &mut self.object
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn trim_end_inline_whitespace(&mut self) {
        self.text = self.text.trim_end_matches([' ', '\t']).to_owned();
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn runtime_object(&self) -> Rc<dyn RTObject> {
        if let Some(runtime_object) = self.object.runtime_object() {
            return runtime_object;
        }

        let runtime_object: Rc<dyn RTObject> = Rc::new(Value::new(self.text.as_str()));
        self.object.set_runtime_object(runtime_object.clone());
        runtime_object
    }
}
