use std::{
    fmt,
    rc::Rc,
};

use crate::object::{Object, RTObject};


pub struct Glue {
    obj: Object,
}

impl Glue {
    pub fn new() -> Self {
        Glue {obj: Object::new()}
    }
}

impl RTObject for Glue {
    fn get_object(&self) -> &Object {
        &self.obj
    }
}

impl fmt::Display for Glue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Glue")
    }
}