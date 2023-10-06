use std::fmt;

use crate::{object::{RTObject, Object}, path::Path, ink_list::InkList, value_type::{StringValue, ValueType, VariablePointerValue}, story_error::StoryError};

const CAST_BOOL: u8 = 0;
const CAST_INT: u8 = 1;
const CAST_FLOAT: u8 = 2;
const CAST_LIST: u8 = 3;
const CAST_STRING: u8 = 4;
const CAST_DIVERT_TARGET: u8 = 5;
const CAST_VARIABLE_POINTER: u8 = 6;

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
            ValueType::DivertTarget(p) => write!(f, "DivertTargetValue({})", p),
            ValueType::VariablePointer(v) => write!(f, "VariablePointerValue({})", v.variable_name),
            ValueType::List(l) => write!(f, "{}", l),
        }
    }
}

impl Value {
    pub fn new(value:ValueType) -> Self {
        Self { obj: Object::new(), value }
    }

    pub fn new_bool(v:bool) -> Self {
        Self { obj: Object::new(), value: ValueType::Bool(v) }
    }

    pub fn new_int(v:i32) -> Self {
        Self { obj: Object::new(), value: ValueType::Int(v) }
    }

    pub fn new_float(v:f32) -> Self {
        Self { obj: Object::new(), value: ValueType::Float(v) }
    }

    pub fn new_string(v:&str) -> Self {
        
        Self { 
            obj: Object::new(), 
            value: ValueType::new_string(v),
        }
    }

    pub fn new_divert_target(p:Path) -> Self {
        Self { obj: Object::new(), value: ValueType::DivertTarget(p) }
    }

    pub fn new_variable_pointer(variable_name: &str, context_index: i32) -> Self {
        Self { obj: Object::new(), value: ValueType::VariablePointer(VariablePointerValue { variable_name: variable_name.to_string(), context_index }) }
    }

    pub fn new_list(l: InkList) -> Self {
        Self { obj: Object::new(), value: ValueType::List(l) }
    }

    pub fn from_value_type(value_type: ValueType) -> Self {
        Self { obj: Object::new(), value: value_type }
    }

    pub fn is_truthy(&self) -> Result<bool, StoryError> {
        match &self.value {
            ValueType::Bool(v) => Ok(*v),
            ValueType::Int(v) => Ok(*v != 0),
            ValueType::Float(v) => Ok(*v != 0.0),
            ValueType::String(v) => Ok(!v.string.is_empty()),
            ValueType::DivertTarget(_) => Err(StoryError::InvalidStoryState("Shouldn't be checking the truthiness of a divert target".to_owned())),
            ValueType::VariablePointer(_) => Err(StoryError::InvalidStoryState("Shouldn't be checking the truthiness of a variable pointer".to_owned())),
            ValueType::List(l) => Ok(!l.items.is_empty()),
        }
    }

    pub fn get_string_value(o: &dyn RTObject) -> Option<&StringValue> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::String(v) => Some(v),
                _ => None,
            },
            None => None,
        }
    }

    pub fn get_variable_pointer_value(o: &dyn RTObject) -> Option<&VariablePointerValue> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::VariablePointer(v) => Some(v),
                _ => None,
            },
            None => None,
        }
    }

    pub fn get_divert_target_value(o: &dyn RTObject) -> Option<&Path> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::DivertTarget(p) => Some(p),
                _ => None,
            },
            None => None,
        }
    }

    pub(crate) fn get_bool_value(o: &dyn RTObject) -> Option<bool> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::Bool(v) => Some(*v),
                _ => None,
            },
            None => None,
        }
    }

    pub fn get_int_value(o: &dyn RTObject) -> Option<i32> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::Int(v) => Some(*v),
                _ => None,
            },
            None => None,
        }
    }

    pub fn get_float_value(o: &dyn RTObject) -> Option<f32> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::Float(v) => Some(*v),
                _ => None,
            },
            None => None,
        }
    }

    pub fn get_list_value_mut(o: &mut dyn RTObject) -> Option<&mut InkList> {
        match o.as_any_mut().downcast_mut::<Value>() {
            Some(v) => match &mut v.value {
                ValueType::List(v) => Some(v),
                _ => None,
            },
            None => None,
        }
    }

    pub fn get_list_value(o: &dyn RTObject) -> Option<&InkList> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::List(v) => Some(v),
                _ => None,
            },
            None => None,
        }
    }

    pub fn get_divert_value(o: &dyn RTObject) -> Option<&Path> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::DivertTarget(v) => Some(v),
                _ => None,
            },
            None => None,
        }
    }

    pub fn retain_list_origins_for_assignment(old_value: &dyn RTObject, new_value: &dyn RTObject) {

        if let Some(old_list) = Self::get_list_value(old_value) {
            if let Some(new_list) = Self::get_list_value(new_value) {
                if new_list.items.is_empty() {
                    new_list.set_initial_origin_names(old_list.get_origin_names());
                }
            }
        }
    }

    pub fn get_cast_ordinal(&self) -> u8 {
        let v = &self.value;

        let ptr_to_option = (v as *const ValueType) as *const u8;
        unsafe {
            *ptr_to_option
        }
    }

    // If None is returned means that casting is not needed
    pub fn cast(&self, cast_dest_type: u8) -> Result<Option<Value>, StoryError> {
        match &self.value {
            ValueType::Bool(v) => {
                match cast_dest_type {
                    CAST_BOOL => Ok(None),
                    CAST_INT => if *v {
                        Ok(Some(Self::new_int(1)))
                    } else {
                        Ok(Some(Self::new_int(0)))
                    },
                    CAST_FLOAT => if *v {
                        Ok(Some(Self::new_float(1.0)))
                    } else {
                        Ok(Some(Self::new_float(0.0)))
                    },
                    CAST_STRING => if *v {
                        Ok(Some(Self::new_string("true")))
                    } else {
                        Ok(Some(Self::new_string("false")))
                    },
                    _ => Err(StoryError::InvalidStoryState("Cast not allowed for bool".to_owned())),
                }
            },
            ValueType::Int(v) => {
                match cast_dest_type {
                    CAST_BOOL => if *v == 0 {
                        Ok(Some(Self::new_bool(false)))
                    } else {
                        Ok(Some(Self::new_bool(true)))
                    },
                    CAST_INT => Ok(None),
                    CAST_FLOAT => Ok(Some(Self::new_float(*v as f32))),
                    CAST_STRING => Ok(Some(Self::new_string(&v.to_string()))),
                    _ => Err(StoryError::InvalidStoryState("Cast not allowed for int".to_owned())),
                }
            },
            ValueType::Float(v) => {
                match cast_dest_type {
                    CAST_BOOL => if *v == 0.0 {
                        Ok(Some(Self::new_bool(false)))
                    } else {
                        Ok(Some(Self::new_bool(true)))
                    },
                    CAST_INT => Ok(Some(Self::new_int(*v as i32))),
                    CAST_FLOAT => Ok(None),
                    CAST_STRING => Ok(Some(Self::new_string(&v.to_string()))),
                    _ => Err(StoryError::InvalidStoryState("Cast not allowed for float".to_owned())),
                }
            },
            ValueType::String(v) => {
                match cast_dest_type {
                    CAST_INT => Ok(Some(Self::new_int(v.string.parse::<i32>().unwrap()))),
                    CAST_FLOAT => Ok(Some(Self::new_float(v.string.parse::<f32>().unwrap()))),
                    CAST_STRING => Ok(None),
                    _ => Err(StoryError::InvalidStoryState("Cast not allowed for string".to_owned())),
                }
            },
            ValueType::List(l) => {
                match cast_dest_type {
                    CAST_INT => {
                        let max = l.get_max_item();
                        match max {
                            Some(i) => Ok(Some(Self::new_int(i.1))),
                            None => Ok(Some(Self::new_int(0)))
                        }
                    },
                    CAST_FLOAT => {
                        let max = l.get_max_item();
                        match max {
                            Some(i) => Ok(Some(Self::new_float(i.1 as f32))),
                            None => Ok(Some(Self::new_float(0.0)))
                        }
                    },
                    CAST_LIST => Ok(None),
                    CAST_STRING => {
                        let max = l.get_max_item();
                        match max {
                            Some(i) =>  Ok(Some(Self::new_string(&i.0.to_string()))),
                            None => Ok(Some(Self::new_string("")))
                        }
                    },
                    _ => Err(StoryError::InvalidStoryState("Cast not allowed for list".to_owned())),
                }
            },
            ValueType::DivertTarget(_) => {
                match cast_dest_type {
                    CAST_DIVERT_TARGET => {
                        Ok(None)
                    },
                    _ => Err(StoryError::InvalidStoryState("Cast not allowed for divert".to_owned())),
                }
            },
            ValueType::VariablePointer(_) => {
                match cast_dest_type {
                    CAST_VARIABLE_POINTER => {
                        Ok(None)
                    },
                    _ => Err(StoryError::InvalidStoryState("Cast not allowed for variable pointer".to_owned())),
                }
            },
        }
    }
}