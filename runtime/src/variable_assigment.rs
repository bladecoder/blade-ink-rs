use std::fmt;

use crate::object::{Object, RTObject};

pub struct VariableAssignment {
    obj: Object,
    pub is_global: bool,
    pub is_new_declaration: bool,
    pub variable_name: String,
}

impl VariableAssignment {
    pub fn new(variable_name: &str, is_new_declaration: bool, is_global: bool) -> Self {
        VariableAssignment {
            obj: Object::new(),
            is_global,
            is_new_declaration,
            variable_name: variable_name.to_string(),
        }
    }
}

impl RTObject for VariableAssignment {
    fn get_object(&self) -> &Object {
        &self.obj
    }
}

impl fmt::Display for VariableAssignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VarAssign to {}", self.variable_name)
    }
}
