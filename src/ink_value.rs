// downcast using Any: https://bennett.dev/rust/downcast-trait-object/
// enum with integers: https://enodev.fr/posts/rusticity-convert-an-integer-to-an-enum.html

use std::any::Any;

use crate::rt_object::RTObject;

enum ValueType {
    Bool = -1,
    Int,
    Float,
    List,
    String,

    // Not used for coersion described above
    DivertTarget,
    VariablePointer,
}

trait InkValue: RTObject {
    fn value_type() -> ValueType;
    fn is_truthy(&self) -> bool;
    //fn cast() -> dyn InkValue;
    //fn value_object() -> object;
}

// Bool Value
pub struct BoolValue {
    value: bool,
}

impl RTObject for BoolValue {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl InkValue for BoolValue {
    fn value_type() -> ValueType {
        return ValueType::Bool;
    }

    fn is_truthy(&self) -> bool {
        return self.value;
    }
}

impl BoolValue {
    pub fn new(value: bool) -> Box<Self> {
        Box::new(BoolValue { value })
    }
}

// Int Value
pub struct IntValue {
    value: i32,
}

impl RTObject for IntValue {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl InkValue for IntValue {
    fn value_type() -> ValueType {
        ValueType::Int
    }

    fn is_truthy(&self) -> bool {
        self.value != 0
    }
}

impl IntValue {
    pub fn new(value: i32) -> Box<Self>  {
        Box::new(IntValue { value })
    }
}

// Float Value
pub struct FloatValue {
    value: f32,
}

impl RTObject for FloatValue {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl InkValue for FloatValue {
    fn value_type() -> ValueType {
        ValueType::Float
    }

    fn is_truthy(&self) -> bool {
        self.value != 0.0
    }
}

impl FloatValue {
    pub fn new(value: f32) -> Box<Self>  {
        Box::new(FloatValue { value })
    }
}

// String Value
pub struct StringValue {
    value: String,
}

impl RTObject for StringValue {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl InkValue for StringValue {
    fn value_type() -> ValueType {
        ValueType::String
    }

    fn is_truthy(&self) -> bool {
        self.value.len() > 0
    }
}

impl StringValue {
    pub fn new(value: String) -> Box<Self>  {
        Box::new(StringValue { value })
    }
}
