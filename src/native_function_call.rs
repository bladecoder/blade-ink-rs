use std::{fmt, rc::Rc};

use crate::{object::{Object, RTObject}, value::Value, void::Void, ink_list::InkList, value_type::ValueType, story_error::StoryError};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Op {
    Add,
    Subtract,
    Divide,
    Multiply,
    Mod,
    Negate,
    
    Equal,
    Greater,
    Less,
    GreaterThanOrEquals,
    LessThanOrEquals,
    NotEquals,
    Not,
    
    And,
    Or,

    Min,
    Max,

    Pow,
    Floor,
    Ceiling,
    Int,
    Float,

    Has,
    Hasnt,
    Intersect,

    ListMin,
    ListMax,
    All,
    Count,
    ValueOfList,
    Invert,
}

pub struct NativeFunctionCall {
    obj: Object,
    pub op: Op,
}

impl NativeFunctionCall {
    pub fn new(op: Op) -> Self {
        Self {
            obj: Object::new(),
            op,
        }
    }

    pub fn new_from_name(name: &str) -> Option<Self> {
        match name {
            "+" => Some(Self::new(Op::Add)),
            "-" => Some(Self::new(Op::Subtract)),
            "/" => Some(Self::new(Op::Divide)),
            "*" => Some(Self::new(Op::Multiply)),
            "%" => Some(Self::new(Op::Mod)),
            "_" => Some(Self::new(Op::Negate)),
            "==" => Some(Self::new(Op::Equal)),
            ">" => Some(Self::new(Op::Greater)),
            "<" => Some(Self::new(Op::Less)),
            ">=" => Some(Self::new(Op::GreaterThanOrEquals)),
            "<=" => Some(Self::new(Op::LessThanOrEquals)),
            "!=" => Some(Self::new(Op::NotEquals)),
            "!" => Some(Self::new(Op::Not)),
            "&&" => Some(Self::new(Op::And)),
            "||" => Some(Self::new(Op::Or)),
            "MIN" => Some(Self::new(Op::Min)),
            "MAX" => Some(Self::new(Op::Max)),
            "POW" => Some(Self::new(Op::Pow)),
            "FLOOR" => Some(Self::new(Op::Floor)),
            "CEILING" => Some(Self::new(Op::Ceiling)),
            "INT" => Some(Self::new(Op::Int)),
            "FLOAT" => Some(Self::new(Op::Float)),
            "?" => Some(Self::new(Op::Has)),
            "!?" => Some(Self::new(Op::Hasnt,)),
            "^" => Some(Self::new(Op::Intersect)),
            "LIST_MIN" => Some(Self::new(Op::ListMin)),
            "LIST_MAX" => Some(Self::new(Op::ListMax)),
            "LIST_ALL" => Some(Self::new(Op::All)),
            "LIST_COUNT" => Some(Self::new(Op::Count)),
            "LIST_VALUE" => Some(Self::new(Op::ValueOfList)),
            "LIST_INVERT" => Some(Self::new(Op::Invert)),
            _ => None,
        }
    }

    pub fn get_name(op: Op) -> String {
        match op {
            Op::Add => "+".to_owned(),
            Op::Subtract => "-".to_owned(),
            Op::Divide =>  "/".to_owned(),
            Op::Multiply => "*".to_owned(),
            Op::Mod => "%".to_owned(),
            Op::Negate => "_".to_owned(),
            Op::Equal => "==".to_owned(),
            Op::Greater => ">".to_owned(),
            Op::Less => "<".to_owned(),
            Op::GreaterThanOrEquals =>  ">=".to_owned(),
            Op::LessThanOrEquals => "<=".to_owned(),
            Op::NotEquals => "!=".to_owned(),
            Op::Not => "!".to_owned(),
            Op::And => "&&".to_owned(),
            Op::Or => "||".to_owned(),
            Op::Min => "MIN".to_owned(),
            Op::Max => "MAX".to_owned(),
            Op::Pow => "POW".to_owned(),
            Op::Floor => "FLOOR".to_owned(),
            Op::Ceiling => "CEILING".to_owned(),
            Op::Int => "INT".to_owned(),
            Op::Float => "FLOAT".to_owned(),
            Op::Has => "?".to_owned(),
            Op::Hasnt => "!?".to_owned(),
            Op::Intersect => "^".to_owned(),
            Op::ListMin => "LIST_MIN".to_owned(),
            Op::ListMax => "LIST_MAX".to_owned(),
            Op::All => "LIST_ALL".to_owned(),
            Op::Count => "LIST_COUNT".to_owned(),
            Op::ValueOfList => "LIST_VALUE".to_owned(),
            Op::Invert => "LIST_INVERT".to_owned(),
        }
    }

    pub fn get_number_of_parameters(&self) -> usize {
        match self.op {
            Op::Add => 2,
            Op::Subtract => 2,
            Op::Divide => 2,
            Op::Multiply => 2,
            Op::Mod => 2,
            Op::Negate => 1,
            Op::Equal => 2,
            Op::Greater => 2,
            Op::Less => 2,
            Op::GreaterThanOrEquals => 2,
            Op::LessThanOrEquals => 2,
            Op::NotEquals => 2,
            Op::Not => 1,
            Op::And => 2,
            Op::Or => 2,
            Op::Min => 2,
            Op::Max => 2,
            Op::Pow => 2,
            Op::Floor => 1,
            Op::Ceiling => 1,
            Op::Int => 1,
            Op::Float => 1,
            Op::Has => 2,
            Op::Hasnt => 2,
            Op::Intersect => 2,
            Op::ListMin => 1,
            Op::ListMax => 1,
            Op::All => 1,
            Op::Count => 1,
            Op::ValueOfList => 1,
            Op::Invert => 1,
        }
    }

    pub(crate) fn call(&self, params: Vec<Rc<dyn RTObject>>) -> Result<Rc<dyn RTObject>, StoryError> {

        if self.get_number_of_parameters() != params.len() {
            return Err(StoryError::InvalidStoryState("Unexpected number of parameters".to_owned()));
        }

        let mut has_list = false;

        for p in &params {
            if p.as_ref().as_any().is::<Void>() {
                return Err(StoryError::InvalidStoryState("Attempting to perform operation on a void value. Did you forget to 'return' a value from a function you called here?".to_owned()));
            }

            if Value::get_list_value(p.as_ref()).is_some() {
                has_list = true;
            }
        }

        // Binary operations on lists are treated outside of the standard
        // coerscion rules
        if params.len() == 2 && has_list {
            return self.call_binary_list_operation(&params);
        }

        let coerced_params = self.coerce_values_to_single_type(params)?;

        self.call_type(coerced_params)
    }

    fn call_binary_list_operation(&self, params: &Vec<Rc<dyn RTObject>>) -> Result<Rc<dyn RTObject>, StoryError> {
        // List-Int addition/subtraction returns a List (e.g., "alpha" + 1 = "beta")
        if (self.op == Op::Add || self.op == Op::Subtract) && 
                Value::get_list_value(params[0].as_ref()).is_some() &&
                Value::get_int_value(params[1].as_ref()).is_some() {
            return Ok(self.call_list_increment_operation(params));
        }

        let v1 = params[0].clone().into_any().downcast::<Value>().unwrap();
        let v2 = params[1].clone().into_any().downcast::<Value>().unwrap();

        // And/or with any other type requires coercion to bool
        if (self.op == Op::And || self.op == Op::Or) &&
                ( Value::get_list_value(params[0].as_ref()).is_none() ||
                Value::get_list_value(params[1].as_ref()).is_none()) {
            
            let result = {
                if self.op == Op::And {
                    v1.is_truthy()? && v2.is_truthy()?
                } else {
                    v1.is_truthy()? || v2.is_truthy()?
                }
            };

            return Ok(Rc::new(Value::new_bool(result)));
        }

        // Normal (list • list) operation
        if Value::get_list_value(params[0].as_ref()).is_some() &&
                Value::get_list_value(params[1].as_ref()).is_some() {
            let mut p = vec![v1.clone(), v2.clone()];
            
            return self.call_type(p);
        }

        Err(StoryError::InvalidStoryState(format!(
            "Can not call use '{}' operation on {} and {}",
            Self::get_name(self.op), // TODO implement Display for op
            v1,
            v2
        )))
    }

    fn call_list_increment_operation(&self, list_int_params: &Vec<Rc<dyn RTObject>>) -> Rc<Value> {
        let list_val = Value::get_list_value(list_int_params[0].as_ref()).unwrap();
        let int_val = Value::get_int_value(list_int_params[1].as_ref()).unwrap();
    
        let mut result_raw_list = InkList::new();
    
        for (list_item, list_item_value) in list_val.items.iter() {
            
            let target_int = {
                if self.op == Op::Add {
                    list_item_value + int_val
                } else {
                    list_item_value - int_val
                }
            };

            let origins = list_val.origins.borrow();
    
            let item_origin = origins.iter().find(|origin| {
                origin.get_name() == list_item.get_origin_name().unwrap_or(&"".to_owned())
            });
    
            if let Some(item_origin) = item_origin {
                if let Some(incremented_item) = item_origin.get_item_with_value(target_int) {
                    result_raw_list.items.insert(incremented_item.clone(), target_int);
                }
            }
        }
    
        Rc::new(Value::new_list(result_raw_list))
    }

    fn call_type(&self, coerced_params: Vec<Rc<Value>>) -> Result<Rc<dyn RTObject>, StoryError> {
        match self.op {
            Op::Add => self.add_op(&coerced_params),
            Op::Subtract => self.subtract_op(&coerced_params),
            Op::Divide => self.divide_op(&coerced_params),
            Op::Multiply => self.multiply_op(&coerced_params),
            Op::Mod => self.mod_op(&coerced_params),
            Op::Negate => self.negate_op(&coerced_params),
            Op::Equal => self.equal_op(&coerced_params),
            Op::Greater => self.greater_op(&coerced_params),
            Op::Less => self.less_op(&coerced_params),
            Op::GreaterThanOrEquals => self.greater_than_or_equals_op(&coerced_params),
            Op::LessThanOrEquals => self.less_than_or_equals_op(&coerced_params),
            Op::NotEquals => self.not_equals_op(&coerced_params),
            Op::Not => self.not_op(&coerced_params),
            Op::And => self.and_op(&coerced_params),
            Op::Or => self.or_op(&coerced_params),
            Op::Min => self.min_op(&coerced_params),
            Op::Max => self.max_op(&coerced_params),
            Op::Pow => self.pow_op(&coerced_params),
            Op::Floor => self.floor_op(&coerced_params),
            Op::Ceiling => self.ceiling_op(&coerced_params),
            Op::Int => self.int_op(&coerced_params),
            Op::Float => self.float_op(&coerced_params),
            Op::Has => self.has(&coerced_params),
            Op::Hasnt => self.hasnt(&coerced_params),
            Op::Intersect => self.intersect_op(&coerced_params),
            Op::ListMin => self.list_min_op(&coerced_params),
            Op::ListMax => self.list_max_op(&coerced_params),
            Op::All => self.all_op(&coerced_params),
            Op::Count => self.count_op(&coerced_params),
            Op::ValueOfList => self.value_of_list_op(&coerced_params),
            Op::Invert => self.inverse_op(&coerced_params),
        }
    }

    fn coerce_values_to_single_type(&self, params: Vec<Rc<dyn RTObject>>) -> Result<Vec<Rc<Value>>, StoryError> {
        let mut dest_type = 1; // Int
        let mut result: Vec<Rc<Value>> = Vec::new();

        for obj in params.iter() {
            // Find out what the output type is
            // "higher level" types infect both so that binary operations
            // use the same type on both sides. e.g. binary operation of
            // int and float causes the int to be casted to a float.
            if let Some(v) = obj.as_ref().as_any().downcast_ref::<Value>() {
                if v.get_cast_ordinal() > dest_type {
                    dest_type = v.get_cast_ordinal();
                }
            }
        }

        for obj in params.iter() {
            if let Some(v) = obj.as_ref().as_any().downcast_ref::<Value>() {                
                match v.cast(dest_type)?  {
                    Some(casted_value) => result.push(Rc::new(casted_value)),
                    None => {
                        if let Ok(obj) = obj.clone().into_any().downcast::<Value>() {
                            result.push(obj); 
                        }
                    },
                }
            } else {
                return Err(StoryError::InvalidStoryState(format!("RTObject of type Value expected: {}", obj.to_string())));
            }
        }

        Ok(result)
    }

    fn and_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Bool(op1) => match params[1].value {
                ValueType::Bool(op2) => Ok(Rc::new(Value::new_bool(*op1 && op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_bool(*op1 != 0 && op2 != 0))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_bool(*op1 != 0.0 && op2 != 0.0))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(op1) => match &params[1].value {
                ValueType::List(op2) => Ok(Rc::new(Value::new_bool(!op1.items.is_empty()  && !op2.items.is_empty()))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn greater_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_bool(*op1 > op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_bool(*op1 > op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(op1) => match &params[1].value {
                ValueType::List(op2) => Ok(Rc::new(Value::new_bool(op1.greater_than(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }


    fn less_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_bool(*op1 < op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_bool(*op1 < op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(op1) => match &params[1].value {
                ValueType::List(op2) => Ok(Rc::new(Value::new_bool(op1.less_than(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn greater_than_or_equals_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_bool(*op1 >= op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_bool(*op1 >= op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(op1) => match &params[1].value {
                ValueType::List(op2) => Ok(Rc::new(Value::new_bool(op1.greater_than_or_equals(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn less_than_or_equals_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_bool(*op1 <= op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_bool(*op1 <= op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(op1) => match &params[1].value {
                ValueType::List(op2) => Ok(Rc::new(Value::new_bool(op1.less_than_or_equals(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn subtract_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_int(*op1 - op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_float(*op1 - op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(op1) => match &params[1].value {
                ValueType::List(op2) => Ok(Rc::new(Value::new_list(op1.without(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn add_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_int(op1 + op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_float(op1 + op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::String(op1) => match &params[1].value {
                ValueType::String(op2) => {
                    let mut sb = String::new();
                    sb.push_str(&op1.string);
                    sb.push_str(&op2.string);
                    Ok(Rc::new(Value::new_string(&sb)))
                },
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(op1) => match &params[1].value {
                ValueType::List(op2) => Ok(Rc::new(Value::new_list(op1.union(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn divide_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_int(op1 / op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_float(op1 / op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn pow_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_float((op1 as f32).powf(op2 as f32)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_float(op1.powf(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn multiply_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_int(op1 * op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_float(op1 * op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn or_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Bool(op1) => match params[1].value {
                ValueType::Bool(op2) => Ok(Rc::new(Value::new_bool(*op1 || op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_bool(*op1 != 0 || op2 != 0))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_bool(*op1 != 0.0 || op2 != 0.0))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(op1) => match &params[1].value {
                ValueType::List(op2) => Ok(Rc::new(Value::new_bool(!op1.items.is_empty()  || !op2.items.is_empty()))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn not_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => Ok(Rc::new(Value::new_bool(*op1 == 0))),
            ValueType::Float(op1) => Ok(Rc::new(Value::new_bool(*op1 == 0.0))),
            ValueType::List(op1) =>   Ok(Rc::new(Value::new_int(match op1.items.is_empty() {
                true => 1,
                false => 0,
            } ))),
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn min_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_int(i32::min(*op1, op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_float(f32::min(*op1, op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(l) => todo!(),
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn max_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_int(i32::max(*op1, op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_float(f32::max(*op1, op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(l) => todo!(),
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn equal_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Bool(op1) => match params[1].value {
                ValueType::Bool(op2) => Ok(Rc::new(Value::new_bool(*op1 == op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_bool(*op1 == op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_bool(*op1 == op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::String(op1) => match &params[1].value {
                ValueType::String(op2) => Ok(Rc::new(Value::new_bool(op1.string.eq(&op2.string)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(op1) => match &params[1].value {
                ValueType::List(op2) => Ok(Rc::new(Value::new_bool(op1.eq(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::DivertTarget(op1) => match &params[1].value {
                ValueType::DivertTarget(op2) => Ok(Rc::new(Value::new_bool(op1.eq(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn not_equals_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Bool(op1) => match params[1].value {
                ValueType::Bool(op2) => Ok(Rc::new(Value::new_bool(*op1 != op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_bool(*op1 != op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_bool(*op1 != op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::String(op1) => match &params[1].value {
                ValueType::String(op2) => Ok(Rc::new(Value::new_bool(!op1.string.eq(&op2.string)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(op1) => match &params[1].value {
                ValueType::List(op2) => Ok(Rc::new(Value::new_bool(!op1.eq(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::DivertTarget(op1) => match &params[1].value {
                ValueType::DivertTarget(op2) => Ok(Rc::new(Value::new_bool(!op1.eq(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn mod_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Ok(Rc::new(Value::new_int(op1 % op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Ok(Rc::new(Value::new_float(op1 % op2))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn intersect_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::List(op1) => match &params[1].value {
                ValueType::List(op2) => Ok(Rc::new(Value::new_list(op1.intersect(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn has(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::String(op1) => match &params[1].value {
                ValueType::String(op2) => Ok(Rc::new(Value::new_bool(op1.string.contains(&op2.string)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(op1) => match &params[1].value {
                ValueType::List(op2) => Ok(Rc::new(Value::new_bool(op1.contains(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn hasnt(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::String(op1) => match &params[1].value {
                ValueType::String(op2) => Ok(Rc::new(Value::new_bool(!op1.string.contains(&op2.string)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            ValueType::List(op1) => match &params[1].value {
                ValueType::List(op2) => Ok(Rc::new(Value::new_bool(!op1.contains(op2)))),
                _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn value_of_list_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::List(op1) => {
                match op1.get_max_item() {
                    Some(i) => Ok(Rc::new(Value::new_int(i.1))),
                    None => Ok(Rc::new(Value::new_int(0))),
                }
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn all_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::List(op1) => {
                Ok(Rc::new(Value::new_list(op1.get_all())))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn inverse_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::List(op1) => {
                Ok(Rc::new(Value::new_list(op1.inverse())))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn count_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::List(op1) => {
                Ok(Rc::new(Value::new_int(op1.items.len() as i32)))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn list_max_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::List(op1) => {
                Ok(Rc::new(Value::new_list(op1.max_as_list())))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn list_min_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::List(op1) => {
                Ok(Rc::new(Value::new_list(op1.min_as_list())))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn negate_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => {
                Ok(Rc::new(Value::new_int(-op1)))
            },
            ValueType::Float(op1) => {
                Ok(Rc::new(Value::new_float(-op1)))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn floor_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => {
                Ok(Rc::new(Value::new_int(*op1)))
            },
            ValueType::Float(op1) => {
                Ok(Rc::new(Value::new_float(op1.floor())))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn ceiling_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => {
                Ok(Rc::new(Value::new_int(*op1)))
            },
            ValueType::Float(op1) => {
                Ok(Rc::new(Value::new_float(op1.ceil())))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn int_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => {
                Ok(Rc::new(Value::new_int(*op1)))
            },
            ValueType::Float(op1) => {
                Ok(Rc::new(Value::new_int(*op1 as i32)))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }

    fn float_op(&self, params: &[Rc<Value>]) -> Result<Rc<dyn RTObject>, StoryError> {
        match &params[0].value {
            ValueType::Int(op1) => {
                Ok(Rc::new(Value::new_float(*op1 as f32)))
            },
            ValueType::Float(op1) => {
                Ok(Rc::new(Value::new_float(*op1)))
            },
            _ => Err(StoryError::InvalidStoryState("Operation not available for type.".to_owned()))
        }
    }
}

impl RTObject for NativeFunctionCall {
    fn get_object(&self) -> &Object {
        &self.obj
    }
}

impl fmt::Display for NativeFunctionCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Native '{:?}'", self.op)
    }
}