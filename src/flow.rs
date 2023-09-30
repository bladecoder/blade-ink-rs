use std::{rc::Rc, cell::RefCell};

use serde_json::Map;

use crate::{callstack::CallStack, choice::Choice, object::RTObject, container::Container, json_write};

#[derive(Clone)]
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

    pub(crate) fn write_json(&self) -> serde_json::Value {
        let mut flow: Map<String, serde_json::Value> = Map::new();

        flow.insert("callstack".to_owned(), self.callstack.borrow().write_json());
        flow.insert("outputStream".to_owned(), json_write::write_list_rt_objs(&self.output_stream));
        
        // choiceThreads: optional
        // Has to come BEFORE the choices themselves are written out
        // since the originalThreadIndex of each choice needs to be set
        let mut has_choice_threads = false;
        let mut jct: Map<String, serde_json::Value> = Map::new();
        for c in self.current_choices.iter() {
            // c.original_thread_index = c.get_thread_at_generation().unwrap().thread_index;
            let original_thread_index = match c.get_thread_at_generation() {
                Some(t) => Some(t.thread_index),
                None => None,
            }.unwrap();

            if self.callstack.borrow().get_thread_with_index(original_thread_index).is_none() {
                if !has_choice_threads {
                    has_choice_threads = true;
                }

                jct.insert(original_thread_index.to_string(), c.get_thread_at_generation().unwrap().write_json());
            }
        }

        if (has_choice_threads) {
            flow.insert("choiceThreads".to_owned(), serde_json::Value::Object(jct));
        }

        let mut c_array: Vec<serde_json::Value> = Vec::new();
        for c in self.current_choices.iter() {
            c_array.push(json_write::write_choice(c));
        }

        flow.insert("currentChoices".to_owned(), serde_json::Value::Array(c_array));

        serde_json::Value::Object(flow)
    }
}