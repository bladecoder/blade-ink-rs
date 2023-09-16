use std::{rc::Rc, cell::RefCell};

use crate::{callstack::CallStack, story::Story, choice::Choice, object::RTObject};

pub(crate) struct Flow {
    pub name: String,
    pub callstack: Rc<RefCell<CallStack>>,
    pub output_stream: Vec<Rc<dyn RTObject>>,
    pub current_choices: Vec<Choice>
}

impl Flow {
    pub fn new(name: &str, story: &Story) -> Flow {
        Flow { 
            name: name.to_string(),
            callstack: Rc::new(RefCell::new(CallStack::new(story))),
            output_stream: Vec::new(),
            current_choices: Vec::new()
        }
    }
}