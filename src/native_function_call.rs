use std::{fmt, collections::HashMap, rc::Rc};

use crate::{object::{Object, RTObject}, value::{Value, ValueType}};

#[derive(Debug)]
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
    op: Op,
    number_of_parameters: i32,
}

impl NativeFunctionCall {
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

    pub fn new(op: Op) -> Self {
        Self {
            obj: Object::new(),
            op,
            number_of_parameters: 0,
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

    pub(crate) fn call(&self, params: Vec<Rc<dyn RTObject>>) -> std::rc::Rc<dyn RTObject> {

        let coerced_params = self.coerce_values_to_single_type(params);

        match self.op {
            Op::Add => self.add_op(coerced_params),
            Op::Subtract => self.subtract_op(coerced_params),
            Op::Divide => self.divide_op(coerced_params),
            Op::Multiply => self.multiply_op(coerced_params),
            Op::Mod => self.mod_op(coerced_params),
            Op::Negate => todo!(),
            Op::Equal => self.equal_op(coerced_params),
            Op::Greater => self.greater_op(coerced_params),
            Op::Less => todo!(),
            Op::GreaterThanOrEquals => todo!(),
            Op::LessThanOrEquals => todo!(),
            Op::NotEquals => self.not_equals_op(coerced_params),
            Op::Not => todo!(),
            Op::And => self.and_op(coerced_params),
            Op::Or => self.or_op(coerced_params),
            Op::Min => self.min_op(coerced_params),
            Op::Max => self.max_op(coerced_params),
            Op::Pow => todo!(),
            Op::Floor => todo!(),
            Op::Ceiling => todo!(),
            Op::Int => todo!(),
            Op::Float => todo!(),
            Op::Has => todo!(),
            Op::Hasnt => todo!(),
            Op::Intersect => todo!(),
            Op::ListMin => todo!(),
            Op::ListMax => todo!(),
            Op::All => todo!(),
            Op::Count => todo!(),
            Op::ValueOfList => todo!(),
            Op::Invert => todo!(),
        }
    }

    fn coerce_values_to_single_type(&self, params: Vec<Rc<dyn RTObject>>) -> Vec<Rc<Value>> {
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
                let casted_value = v.cast(dest_type);
                result.push(Rc::new(casted_value));
            } else {
                panic!("RTObject of type Value expected: {}", obj.to_string())
            }
        }

        return result;
    }

    fn and_op(&self, params: Vec<Rc<Value>>) -> Rc<dyn RTObject> {
        match params[0].value {
            ValueType::Bool(op1) => match params[1].value {
                ValueType::Bool(op2) => Rc::new(Value::new_bool(op1 && op2)),
                _ => panic!()
            },
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Rc::new(Value::new_bool(op1 != 0 && op2 != 0)),
                _ => panic!()
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Rc::new(Value::new_bool(op1 != 0.0 && op2 != 0.0)),
                _ => panic!()
            },
            ValueType::List() => todo!(),
            _ => panic!()
        }
    }

    fn greater_op(&self, params: Vec<Rc<Value>>) -> Rc<dyn RTObject> {
        match params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Rc::new(Value::new_bool(op1 > op2)),
                _ => panic!()
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Rc::new(Value::new_bool(op1 > op2)),
                _ => panic!()
            },
            ValueType::List() => todo!(),
            _ => panic!()
        }
    }

    fn subtract_op(&self, params: Vec<Rc<Value>>) -> Rc<dyn RTObject> {
        match params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Rc::new(Value::new_int(op1 - op2)),
                _ => panic!()
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Rc::new(Value::new_float(op1 - op2)),
                _ => panic!()
            },
            ValueType::List() => todo!(),
            _ => panic!()
        }
    }

    fn add_op(&self, params: Vec<Rc<Value>>) -> Rc<dyn RTObject> {
        match &params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Rc::new(Value::new_int(op1 + op2)),
                _ => panic!()
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Rc::new(Value::new_float(op1 + op2)),
                _ => panic!()
            },
            ValueType::String(op1) => match &params[1].value {
                ValueType::String(op2) => {
                    let mut sb = String::new();
                    sb.push_str(&op1.string);
                    sb.push_str(&op2.string);
                    Rc::new(Value::new_string(&sb))
                },
                _ => panic!()
            },
            ValueType::List() => todo!(),
            _ => panic!()
        }
    }

    fn divide_op(&self, params: Vec<Rc<Value>>) -> Rc<dyn RTObject> {
        match params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Rc::new(Value::new_int(op1 / op2)),
                _ => panic!()
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Rc::new(Value::new_float(op1 / op2)),
                _ => panic!()
            },
            _ => panic!()
        }
    }

    fn multiply_op(&self, params: Vec<Rc<Value>>) -> Rc<dyn RTObject> {
        match params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Rc::new(Value::new_int(op1 * op2)),
                _ => panic!()
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Rc::new(Value::new_float(op1 * op2)),
                _ => panic!()
            },
            _ => panic!()
        }
    }

    fn or_op(&self, params: Vec<Rc<Value>>) -> Rc<dyn RTObject> {
        match params[0].value {
            ValueType::Bool(op1) => match params[1].value {
                ValueType::Bool(op2) => Rc::new(Value::new_bool(op1 || op2)),
                _ => panic!()
            },
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Rc::new(Value::new_bool(op1 != 0 || op2 != 0)),
                _ => panic!()
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Rc::new(Value::new_bool(op1 != 0.0 || op2 != 0.0)),
                _ => panic!()
            },
            ValueType::List() => todo!(),
            _ => panic!()
        }
    }

    fn min_op(&self, params: Vec<Rc<Value>>) -> Rc<dyn RTObject> {
        match params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Rc::new(Value::new_int(i32::min(op1, op2))),
                _ => panic!()
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Rc::new(Value::new_float(f32::min(op1, op2))),
                _ => panic!()
            },
            ValueType::List() => todo!(),
            _ => panic!()
        }
    }

    fn max_op(&self, params: Vec<Rc<Value>>) -> Rc<dyn RTObject> {
        match params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Rc::new(Value::new_int(i32::max(op1, op2))),
                _ => panic!()
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Rc::new(Value::new_float(f32::max(op1, op2))),
                _ => panic!()
            },
            ValueType::List() => todo!(),
            _ => panic!()
        }
    }

    fn equal_op(&self, params: Vec<Rc<Value>>) -> Rc<dyn RTObject> {
        match &params[0].value {
            ValueType::Bool(op1) => match params[1].value {
                ValueType::Bool(op2) => Rc::new(Value::new_bool(*op1 == op2)),
                _ => panic!()
            },
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Rc::new(Value::new_bool(*op1 == op2)),
                _ => panic!()
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Rc::new(Value::new_bool(*op1 == op2)),
                _ => panic!()
            },
            ValueType::String(op1) => match &params[1].value {
                ValueType::String(op2) => Rc::new(Value::new_bool(op1.string.eq(&op2.string))),
                _ => panic!()
            },
            ValueType::List() => todo!(),
            _ => panic!()
        }
    }

    fn not_equals_op(&self, params: Vec<Rc<Value>>) -> Rc<dyn RTObject> {
        match &params[0].value {
            ValueType::Bool(op1) => match params[1].value {
                ValueType::Bool(op2) => Rc::new(Value::new_bool(*op1 != op2)),
                _ => panic!()
            },
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Rc::new(Value::new_bool(*op1 != op2)),
                _ => panic!()
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Rc::new(Value::new_bool(*op1 != op2)),
                _ => panic!()
            },
            ValueType::String(op1) => match &params[1].value {
                ValueType::String(op2) => Rc::new(Value::new_bool(!op1.string.eq(&op2.string))),
                _ => panic!()
            },
            ValueType::List() => todo!(),
            _ => panic!()
        }
    }

    fn mod_op(&self, params: Vec<Rc<Value>>) -> Rc<dyn RTObject> {
        match params[0].value {
            ValueType::Int(op1) => match params[1].value {
                ValueType::Int(op2) => Rc::new(Value::new_int(op1 % op2)),
                _ => panic!()
            },
            ValueType::Float(op1) => match params[1].value {
                ValueType::Float(op2) => Rc::new(Value::new_float(op1 % op2)),
                _ => panic!()
            },
            _ => panic!()
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