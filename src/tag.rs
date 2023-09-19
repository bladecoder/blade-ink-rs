use std::fmt;

use crate::object::{Object, RTObject};

pub struct Tag {
    obj: Object,
    text: String,
}

impl Tag {
    pub fn new(text: &str) -> Self {
        Tag {obj: Object::new(), text: text.to_string()}
    }

    pub fn get_text(&self) -> String {
        self.text.clone()
    }
}

impl RTObject for Tag {
    fn get_object(&self) -> &Object {
        &self.obj
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let t = &self.text;
        write!(f, "# {t}")
    }
}