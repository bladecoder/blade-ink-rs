use std::{rc::Rc, cell::RefCell};

use crate::{ object::{Null, Object, RTObject}, value::Value, control_command::ControlCommand, container::Container};

#[derive(Clone)]
pub enum ObjectEnum {
    Value(Rc<RefCell<Value>>),
    Container(Rc<RefCell<Container>>),
    ControlCommand(Rc<RefCell<ControlCommand>>),
    Null(Rc<RefCell<Null>>)
}

impl ObjectEnum {
    pub fn get_obj(&self) -> &Object {
        match self {
            ObjectEnum::Value(o) => o.borrow().get_object(),
            ObjectEnum::Container(o) => o.borrow().get_object(),
            ObjectEnum::ControlCommand(o) => o.borrow().get_object(),
            ObjectEnum::Null(o) => o.borrow().get_object(),
        }
    }

    pub fn get_obj_mut(&self) -> &Object {
        match self {
            ObjectEnum::Value(o) => o.borrow_mut().get_object(),
            ObjectEnum::Container(o) => o.borrow_mut().get_object(),
            ObjectEnum::ControlCommand(o) => o.borrow_mut().get_object(),
            ObjectEnum::Null(o) => o.borrow_mut().get_object(),
        }
    }
}

