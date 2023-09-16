use std::{
    fmt,
    rc::Rc,
};

use as_any::Downcast;

use crate::{
    object::{Object, RTObject, Null},
    value::{ValueType, Value}, control_command::ControlCommand,
};


pub struct Glue {
    obj: Object,
}

impl Glue {
    pub fn new() -> Rc<Glue> {
        Rc::new(Glue {obj: Object::new()})
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