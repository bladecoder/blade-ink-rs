use crate::{ink_list::InkList, path::Path, story_error::StoryError};

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
            is_newline: str.eq("\n"),
        })
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

    pub fn coerce_to_int(&self) -> Result<i32, StoryError> {
        match self {
            ValueType::Bool(v) => {
                if *v {
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
            ValueType::Int(v) => Ok(*v),
            ValueType::Float(v) => Ok(*v as i32),
            _ => Err(StoryError::BadArgument("Failed to cast to int".to_owned())),
        }
    }

    pub fn coerce_to_float(&self) -> Result<f32, StoryError> {
        match self {
            ValueType::Bool(v) => {
                if *v {
                    Ok(1.0)
                } else {
                    Ok(0.0)
                }
            }
            ValueType::Int(v) => Ok(*v as f32),
            ValueType::Float(v) => Ok(*v),
            _ => Err(StoryError::BadArgument(
                "Failed to cast to float".to_owned(),
            )),
        }
    }

    pub fn coerce_to_bool(&self) -> Result<bool, StoryError> {
        match self {
            ValueType::Bool(v) => Ok(*v),
            ValueType::Int(v) => {
                if *v == 1 {
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            _ => Err(StoryError::BadArgument(
                "Failed to cast to boolean".to_owned(),
            )),
        }
    }

    pub fn coerce_to_string(&self) -> Result<String, StoryError> {
        match self {
            ValueType::Bool(v) => Ok(v.to_string()),
            ValueType::Int(v) => Ok(v.to_string()),
            ValueType::Float(v) => Ok(v.to_string()),
            ValueType::String(v) => Ok(v.string.clone()),
            _ => Err(StoryError::BadArgument(
                "Failed to cast to float".to_owned(),
            )),
        }
    }
}

#[derive(Clone)]
pub struct StringValue {
    pub string: String,
    pub is_inline_whitespace: bool,
    pub is_newline: bool,
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
