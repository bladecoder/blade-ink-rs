use std::{rc::Rc, cell::RefCell};

use crate::{ object::{Null, Object, RTObject}, value::Value, control_command::ControlCommand, container::Container};

#[derive(Clone)]
pub enum ObjectEnum {
    Value,
    Container,
    ControlCommand,
    Null
}