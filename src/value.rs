// enum with integers: https://enodev.fr/posts/rusticity-convert-an-integer-to-an-enum.html

use std::{fmt};

use crate::{object::{RTObject, Object}, path::Path};

#[repr(i8)]
pub enum ValueType {
    Bool(bool) = -1,
    Int(i32),
    Float(f32),
    //List(List),
    String(StringValue),

    // Not used for coersion described above
    DivertTarget(Path),
    //VariablePointer,
}


#[derive(Clone)]
pub struct StringValue {
    pub string: String,
    pub is_inline_whitespace: bool,
    pub is_newline: bool
}

impl StringValue {
    pub fn is_non_whitespace(&self) -> bool {
        return !self.is_newline && !self.is_inline_whitespace;
    }

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
            ValueType::String(v) => write!(f, "{}", v.string),
            ValueType::DivertTarget(p) => write!(f, "DivertTargetValue({})", p.to_string()),
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

        let mut inline_ws = true;

        for c in v.chars() {
            if c != ' ' && c != '\t' {
                inline_ws = false;
                break;
            }
        }
        
        Value { 
            obj: Object::new(), 
            value: ValueType::String(StringValue {
                string: v.to_string(), 
                is_inline_whitespace: inline_ws, 
                is_newline: v.eq("\n")}) 
            }
    }

    pub fn new_divert_target(p:Path) -> Value {
        Value { obj: Object::new(), value: ValueType::DivertTarget(p) }
    }

    pub fn is_truthy(&self) -> bool {
        match &self.value {
            ValueType::Bool(v) => *v,
            ValueType::Int(v) => *v != 0,
            ValueType::Float(v) => *v != 0.0,
            ValueType::String(v) => v.string.len() > 0,
            ValueType::DivertTarget(_) => false, // exception Shouldn't be checking the truthiness of a divert target??
        }      
    }

    pub fn get_string_value(o: &dyn RTObject) -> Option<&StringValue> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::String(v) => Some(&v),
                _ => None,
            },
            None => None,
        }
    }
}