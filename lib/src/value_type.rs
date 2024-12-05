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
    /// Ink string, constructed with [`new_string`](ValueType::new::<&str>)
    String(StringValue),
    /// Reference to an Ink divert.
    DivertTarget(Path),
    /// Reference to an Ink variable.
    VariablePointer(VariablePointerValue),
}

impl From<bool> for ValueType {
    fn from(value: bool) -> ValueType {
        ValueType::Bool(value)
    }
}

impl From<i32> for ValueType {
    fn from(value: i32) -> ValueType {
        ValueType::Int(value)
    }
}

impl From<f32> for ValueType {
    fn from(value: f32) -> ValueType {
        ValueType::Float(value)
    }
}

impl From<&str> for ValueType {
    fn from(value: &str) -> ValueType {
        let inline_ws = value.chars().all(|c| c == ' ' || c == '\t');

        ValueType::String(StringValue {
            string: value.to_string(),
            is_inline_whitespace: inline_ws,
            is_newline: value.eq("\n"),
        })
    }
}

impl From<InkList> for ValueType {
    fn from(value: InkList) -> ValueType {
        ValueType::List(value)
    }
}

impl From<Path> for ValueType {
    fn from(value: Path) -> ValueType {
        ValueType::DivertTarget(value)
    }
}

impl From<VariablePointerValue> for ValueType {
    fn from(value: VariablePointerValue) -> Self {
        ValueType::VariablePointer(value)
    }
}

impl TryFrom<&ValueType> for bool {
    type Error = ();
    fn try_from(value: &ValueType) -> Result<Self, Self::Error> {
        match value {
            ValueType::Bool(v) => Ok(*v),
            _ => Err(()),
        }
    }
}

impl TryFrom<&ValueType> for i32 {
    type Error = ();
    fn try_from(value: &ValueType) -> Result<Self, Self::Error> {
        match value {
            ValueType::Int(v) => Ok(*v),
            _ => Err(()),
        }
    }
}

impl TryFrom<&ValueType> for f32 {
    type Error = ();
    fn try_from(value: &ValueType) -> Result<Self, Self::Error> {
        match value {
            ValueType::Float(v) => Ok(*v),
            _ => Err(()),
        }
    }
}

impl<'val> TryFrom<&'val ValueType> for &'val str {
    type Error = ();
    fn try_from(value: &'val ValueType) -> Result<Self, Self::Error> {
        match value {
            ValueType::String(v) => Ok(&v.string),
            _ => Err(()),
        }
    }
}

impl ValueType {
    pub fn new<T: Into<ValueType>>(v: T) -> Self {
        v.into()
    }

    pub fn get<'val, T>(&'val self) -> Option<T>
    where
        &'val Self: TryInto<T>,
    {
        self.try_into().ok()
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
