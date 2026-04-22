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

#[cfg(test)]
mod tests {
    use super::Text;
    use crate::parsed_hierarchy::DebugMetadata;

    #[test]
    fn text_runtime_object_receives_debug_metadata() {
        let mut text = Text::new("hello");
        text.object_mut().set_debug_metadata(DebugMetadata {
            start_line_number: 1,
            end_line_number: 1,
            start_character_number: 1,
            end_character_number: 5,
            file_name: Some("main.ink".to_owned()),
        });

        let runtime = text.runtime_object();
        let metadata = runtime.get_object().debug_metadata().expect("runtime metadata");
        assert_eq!(1, metadata.start_line_number);
        assert_eq!(Some("main.ink".to_owned()), metadata.file_name);
    }
}
