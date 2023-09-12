// enum with integers: https://enodev.fr/posts/rusticity-convert-an-integer-to-an-enum.html

use std::{fmt};

use crate::{object::{RTObject, Object}};

#[repr(i8)]
pub enum ValueType {
    Bool(bool) = -1,
    Int(i32),
    Float(f32),
    //List(List),
    String(String),

    // Not used for coersion described above
    //DivertTarget,
    //VariablePointer,
}

pub struct Value {
    obj: Object,
    pub value: ValueType,
}

impl RTObject for Value {
    fn get_object(&self) -> &Object {
        &self.obj
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.value {
            ValueType::Bool(v) => write!(f, "{}", v),
            ValueType::Int(v) => write!(f, "{}", v),
            ValueType::Float(v) => write!(f, "{}", v),
            ValueType::String(v) => write!(f, "{}", v),
        }
    }
}

impl Value {
    pub fn new_bool(v:bool) -> Value {
        Value { obj: Object::new(), value: ValueType::Bool(v) }
    }

    pub fn new_int(v:i32) -> Value {
        Value { obj: Object::new(), value: ValueType::Int(v) }
    }

    pub fn new_float(v:f32) -> Value {
        Value { obj: Object::new(), value: ValueType::Float(v) }
    }

    pub fn new_string(v:&str) -> Value {
        Value { obj: Object::new(), value: ValueType::String(v.to_string()) }
    }

    pub fn is_truthy(&self) -> bool {
        match &self.value {
            ValueType::Bool(v) => *v,
            ValueType::Int(v) => *v != 0,
            ValueType::Float(v) => *v != 0.0,
            ValueType::String(v) => v.len() > 0,
        }      
    }
}