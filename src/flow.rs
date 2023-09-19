use std::{rc::Rc, cell::RefCell};

use crate::{callstack::CallStack, choice::Choice, object::RTObject, container::Container};

pub struct Flow {
    pub name: String,
    pub callstack: Rc<RefCell<CallStack>>,
    pub output_stream: Vec<Rc<dyn RTObject>>,
    pub current_choices: Vec<Rc<Choice>>
}

impl Flow {
    pub fn new(name: &str, main_content_container: Rc<Container>) -> Flow {
        Flow { 
            name: name.to_string(),
            callstack: Rc::new(RefCell::new(CallStack::new(main_content_container))),
            output_stream: Vec::new(),
            current_choices: Vec::new()
        }
    }
}