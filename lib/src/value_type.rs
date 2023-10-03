use std::fmt;

use crate::{object::{RTObject, Object}, path::Path, ink_list::InkList};

#[repr(u8)]
#[derive(Clone)]
pub enum ValueType {
    Bool(bool),
    Int(i32),
    Float(f32),
    List(InkList),
    String(StringValue),
    DivertTarget(Path),
    VariablePointer(VariablePointerValue),
}

impl ValueType {
    pub fn new_string(str: &str) -> ValueType {
        let mut inline_ws = true;

        for c in str.chars() {
            if c != ' ' && c != '\t' {
                inline_ws = false;
                break;
            }
        }
        
        ValueType::String(StringValue {
                string: str.to_string(), 
                is_inline_whitespace: inline_ws, 
                is_newline: str.eq("\n")})
    }

    pub fn get_bool(&self) -> Option<bool> {
        match self {
            ValueType::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn get_int(&self) -> Option<i32> {
        match self {
            ValueType::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn get_float(&self) -> Option<f32> {
        match self {
            ValueType::Float(v) => Some(*v),
            _ => None,
        }
    }

    pub fn get_str(&self) -> Option<&str> {
        match self {
            ValueType::String(v) => Some(&v.string),
            _ => None,
        }
    }
}


#[derive(Clone)]
pub struct StringValue {
    pub string: String,
    pub is_inline_whitespace: bool,
    pub is_newline: bool
}

impl StringValue {
    pub fn is_non_whitespace(&self) -> bool {
        !self.is_newline && !self.is_inline_whitespace
    }
}

#[derive(Clone, PartialEq)]
pub struct VariablePointerValue {
    pub variable_name: String,

    // Where the variable is located
    // -1 = default, unknown, yet to be determined
    // 0  = in global scope
    // 1+ = callstack element index + 1 (so that the first doesn't conflict with special global scope)
    pub context_index: i32,
}
