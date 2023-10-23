//! A combination of an Ink value with its type.
use crate::{ink_list::InkList, path::Path, story_error::StoryError};

/// An Ink value, tagged with its type.
#[repr(u8)]
#[derive(Clone)]
pub enum ValueType {
    Bool(bool),
    Int(i32),
    Float(f32),
    /// An Ink list value.
    List(InkList),
    /// Ink string, constructed with [`new_string`](ValueType::new_string)
    String(StringValue),
    /// Reference to an Ink divert.
    DivertTarget(Path),
    /// Reference to an Ink variable.
    VariablePointer(VariablePointerValue),
}

impl ValueType {
    /// Creates a new `ValueType` for a `String`.
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

    /// Gets the internal boolean, value or `None` if the `ValueType` is not a [`ValueType::Bool`]
    pub fn get_bool(&self) -> Option<bool> {
        match self {
            ValueType::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Gets the internal `i32` value, or `None` if the `ValueType` is not a [`ValueType::Int`]
    pub fn get_int(&self) -> Option<i32> {
        match self {
            ValueType::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// Gets the internal `f32` value, or `None` if the `ValueType` is not a [`ValueType::Float`]
    pub fn get_float(&self) -> Option<f32> {
        match self {
            ValueType::Float(v) => Some(*v),
            _ => None,
        }
    }

    /// Gets the internal string value, or `None` if the `ValueType` is not a [`ValueType::String`]
    pub fn get_str(&self) -> Option<&str> {
        match self {
            ValueType::String(v) => Some(&v.string),
            _ => None,
        }
    }

    /// Tries to convert the internal value of this `ValueType` to `i32`
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

    /// Tries to convert the internal value of this `ValueType` to `f32`
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

    /// Tries to convert the internal value of this `ValueType` to `bool`
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

    /// Tries to convert the internal value of this `ValueType` to `String`
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

/// Ink runtime representation of a string.
#[derive(Clone)]
pub struct StringValue {
    /// The internal string value.
    pub string: String,
    pub(crate) is_inline_whitespace: bool,
    pub(crate) is_newline: bool,
}

impl StringValue {
    pub fn is_non_whitespace(&self) -> bool {
        !self.is_newline && !self.is_inline_whitespace
    }
}

/// Ink runtime representation of a reference to a variable.
#[derive(Clone, PartialEq)]
pub struct VariablePointerValue {
    pub(crate) variable_name: String,

    // Where the variable is located
    // -1 = default, unknown, yet to be determined
    // 0  = in global scope
    // 1+ = callstack element index + 1 (so that the first doesn't conflict with special global scope)
    pub(crate) context_index: i32,
}
