use std::fmt;

use crate::{
    ink_list::InkList,
    object::{Object, RTObject},
    path::Path,
    story_error::StoryError,
    value_type::{StringValue, ValueType, VariablePointerValue},
};

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

impl<T: Into<ValueType>> From<T> for Value {
    fn from(value: T) -> Self {
        Self::new_value_type(value.into())
    }
}

impl<'val> TryFrom<&'val dyn RTObject> for &'val StringValue {
    type Error = ();
    fn try_from(o: &dyn RTObject) -> Result<&StringValue, Self::Error> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::String(v) => Ok(v),
                _ => Err(()),
            },
            None => Err(()),
        }
    }
}
impl<'val> TryFrom<&'val dyn RTObject> for &'val VariablePointerValue {
    type Error = ();
    fn try_from(o: &dyn RTObject) -> Result<&VariablePointerValue, Self::Error> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::VariablePointer(v) => Ok(v),
                _ => Err(()),
            },
            None => Err(()),
        }
    }
}
impl<'val> TryFrom<&'val dyn RTObject> for &'val Path {
    type Error = ();
    fn try_from(o: &dyn RTObject) -> Result<&Path, Self::Error> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::DivertTarget(p) => Ok(p),
                _ => Err(()),
            },
            None => Err(()),
        }
    }
}
impl TryFrom<&dyn RTObject> for i32 {
    type Error = ();
    fn try_from(o: &dyn RTObject) -> Result<i32, Self::Error> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::Int(v) => Ok(*v),
                _ => Err(()),
            },
            None => Err(()),
        }
    }
}
impl TryFrom<&dyn RTObject> for f32 {
    type Error = ();
    fn try_from(o: &dyn RTObject) -> Result<f32, Self::Error> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::Float(v) => Ok(*v),
                _ => Err(()),
            },
            None => Err(()),
        }
    }
}
impl<'val> TryFrom<&'val mut dyn RTObject> for &'val mut InkList {
    type Error = ();
    fn try_from(o: &mut dyn RTObject) -> Result<&mut InkList, Self::Error> {
        match o.as_any_mut().downcast_mut::<Value>() {
            Some(v) => match &mut v.value {
                ValueType::List(v) => Ok(v),
                _ => Err(()),
            },
            None => Err(()),
        }
    }
}
impl<'val> TryFrom<&'val dyn RTObject> for &'val InkList {
    type Error = ();
    fn try_from(o: &dyn RTObject) -> Result<&InkList, Self::Error> {
        match o.as_any().downcast_ref::<Value>() {
            Some(v) => match &v.value {
                ValueType::List(v) => Ok(v),
                _ => Err(()),
            },
            None => Err(()),
        }
    }
}

impl Value {
    pub fn new_value_type(valuetype: ValueType) -> Self {
        Self {
            obj: Object::new(),
            value: valuetype,
        }
    }

    pub fn new<T: Into<Value>>(v: T) -> Self {
        v.into()
    }

    pub fn new_variable_pointer(variable_name: &str, context_index: i32) -> Self {
        Self {
            obj: Object::new(),
            value: ValueType::VariablePointer(VariablePointerValue {
                variable_name: variable_name.to_string(),
                context_index,
            }),
        }
    }

    pub fn get_value<'val, T>(o: &'val dyn RTObject) -> Option<T>
    where
        &'val dyn RTObject: TryInto<T>,
    {
        o.try_into().ok()
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

    pub fn is_truthy(&self) -> Result<bool, StoryError> {
        match &self.value {
            ValueType::Bool(v) => Ok(*v),
            ValueType::Int(v) => Ok(*v != 0),
            ValueType::Float(v) => Ok(*v != 0.0),
            ValueType::String(v) => Ok(!v.string.is_empty()),
            ValueType::DivertTarget(_) => Err(StoryError::InvalidStoryState(
                "Shouldn't be checking the truthiness of a divert target".to_owned(),
            )),
            ValueType::VariablePointer(_) => Err(StoryError::InvalidStoryState(
                "Shouldn't be checking the truthiness of a variable pointer".to_owned(),
            )),
            ValueType::List(l) => Ok(!l.items.is_empty()),
        }
    }

    pub fn retain_list_origins_for_assignment(old_value: &dyn RTObject, new_value: &dyn RTObject) {
        if let Some(old_list) = Self::get_value::<&InkList>(old_value) {
            if let Some(new_list) = Self::get_value::<&InkList>(new_value) {
                if new_list.items.is_empty() {
                    new_list.set_initial_origin_names(old_list.get_origin_names());
                }
            }
        }
    }

    pub fn get_cast_ordinal(&self) -> u8 {
        let v = &self.value;

        // SAFETY: `ValueType` is `repr(u8)` so every variant has the layout
        // of a struct with its first field being the `u8` discriminant,
        // ensuring the `u8` can be read from a pointer to the enum.
        // See e.g. https://doc.rust-lang.org/std/mem/fn.discriminant.html#accessing-the-numeric-value-of-the-discriminant
        let ptr_to_option = (v as *const ValueType) as *const u8;
        unsafe { *ptr_to_option }
    }

    // If None is returned means that casting is not needed
    pub fn cast(&self, cast_dest_type: u8) -> Result<Option<Value>, StoryError> {
        match &self.value {
            ValueType::Bool(v) => match cast_dest_type {
                CAST_BOOL => Ok(None),
                CAST_INT => {
                    if *v {
                        Ok(Some(Self::new::<i32>(1)))
                    } else {
                        Ok(Some(Self::new::<i32>(0)))
                    }
                }
                CAST_FLOAT => {
                    if *v {
                        Ok(Some(Self::new::<f32>(1.0)))
                    } else {
                        Ok(Some(Self::new::<f32>(0.0)))
                    }
                }
                CAST_STRING => {
                    if *v {
                        Ok(Some(Self::new::<&str>("true")))
                    } else {
                        Ok(Some(Self::new::<&str>("false")))
                    }
                }
                _ => Err(StoryError::InvalidStoryState(
                    "Cast not allowed for bool".to_owned(),
                )),
            },
            ValueType::Int(v) => match cast_dest_type {
                CAST_BOOL => {
                    if *v == 0 {
                        Ok(Some(Self::new::<bool>(false)))
                    } else {
                        Ok(Some(Self::new::<bool>(true)))
                    }
                }
                CAST_INT => Ok(None),
                CAST_FLOAT => Ok(Some(Self::new::<f32>(*v as f32))),
                CAST_STRING => Ok(Some(Self::new::<&str>(&v.to_string()))),
                _ => Err(StoryError::InvalidStoryState(
                    "Cast not allowed for int".to_owned(),
                )),
            },
            ValueType::Float(v) => match cast_dest_type {
                CAST_BOOL => {
                    if *v == 0.0 {
                        Ok(Some(Self::new::<bool>(false)))
                    } else {
                        Ok(Some(Self::new::<bool>(true)))
                    }
                }
                CAST_INT => Ok(Some(Self::new::<i32>(*v as i32))),
                CAST_FLOAT => Ok(None),
                CAST_STRING => Ok(Some(Self::new::<&str>(&v.to_string()))),
                _ => Err(StoryError::InvalidStoryState(
                    "Cast not allowed for float".to_owned(),
                )),
            },
            ValueType::String(v) => match cast_dest_type {
                CAST_INT => Ok(Some(Self::new::<i32>(v.string.parse::<i32>().unwrap()))),
                CAST_FLOAT => Ok(Some(Self::new::<f32>(v.string.parse::<f32>().unwrap()))),
                CAST_STRING => Ok(None),
                _ => Err(StoryError::InvalidStoryState(
                    "Cast not allowed for string".to_owned(),
                )),
            },
            ValueType::List(l) => match cast_dest_type {
                CAST_INT => {
                    let max = l.get_max_item();
                    match max {
                        Some(i) => Ok(Some(Self::new::<i32>(i.1))),
                        None => Ok(Some(Self::new::<i32>(0))),
                    }
                }
                CAST_FLOAT => {
                    let max = l.get_max_item();
                    match max {
                        Some(i) => Ok(Some(Self::new::<f32>(i.1 as f32))),
                        None => Ok(Some(Self::new::<f32>(0.0))),
                    }
                }
                CAST_LIST => Ok(None),
                CAST_STRING => {
                    let max = l.get_max_item();
                    match max {
                        Some(i) => Ok(Some(Self::new::<&str>(&i.0.to_string()))),
                        None => Ok(Some(Self::new::<&str>(""))),
                    }
                }
                _ => Err(StoryError::InvalidStoryState(
                    "Cast not allowed for list".to_owned(),
                )),
            },
            ValueType::DivertTarget(_) => match cast_dest_type {
                CAST_DIVERT_TARGET => Ok(None),
                _ => Err(StoryError::InvalidStoryState(
                    "Cast not allowed for divert".to_owned(),
                )),
            },
            ValueType::VariablePointer(_) => match cast_dest_type {
                CAST_VARIABLE_POINTER => Ok(None),
                _ => Err(StoryError::InvalidStoryState(
                    "Cast not allowed for variable pointer".to_owned(),
                )),
            },
        }
    }
}
