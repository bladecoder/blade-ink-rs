use std::fmt;

use crate::object::{Object, RTObject};

pub struct Void {
    obj: Object,
}

impl Void {
    pub fn new() -> Self {
        Void { obj: Object::new() }
    }
}

impl RTObject for Void {
    fn get_object(&self) -> &Object {
        &self.obj
    }
}

impl fmt::Display for Void {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Void")
    }
}
