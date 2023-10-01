use core::fmt;
use std::cell::RefCell;

use crate::{path::Path, callstack::Thread, object::{Object, RTObject}};

pub struct Choice {
    obj: Object,
    pub target_path: Path,
    pub is_invisible_default: bool,
    pub tags: Vec<String>,
    pub index: RefCell<usize>,
    pub original_thread_index: RefCell<usize>,
    pub text: String,
    thread_at_generation: RefCell<Option<Thread>>,
    pub source_path: String
}

impl Choice {
    pub fn new(target_path: Path, source_path: String, is_invisible_default: bool, tags: Vec<String>, thread_at_generation: Thread, text: String, index: usize, original_thread_index: usize) -> Choice {
        Self {
            obj: Object::new(),
            target_path,
            is_invisible_default,
            tags,
            index: RefCell::new(index),
            original_thread_index: RefCell::new(original_thread_index),
            text,
            thread_at_generation: RefCell::new(Some(thread_at_generation)),
            source_path,
        }
    }

    pub fn new_from_json(path_string_on_choice: &str, source_path: String, text: &str, index: usize, original_thread_index: usize) -> Choice {
        
        Choice {
            obj: Object::new(),
            target_path: Path::new_with_components_string(Some(path_string_on_choice)),
            is_invisible_default: false,
            tags: Vec::new(),
            index: RefCell::new(index),
            original_thread_index: RefCell::new(original_thread_index),
            text: text.to_string(),
            thread_at_generation: RefCell::new(None),
            source_path,
        }
    }

    pub fn set_thread_at_generation(&self, thread: Thread) {
        self.thread_at_generation.replace(Some(thread));
    }

    pub fn get_thread_at_generation(&self) -> Option<Thread> {
        match self.thread_at_generation.borrow().as_ref() {
            Some(t) => Some(t.copy()),
            None => None,
        }
    }
}

impl RTObject for Choice {
    fn get_object(&self) -> &Object {
        &self.obj
    }
}

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "**Choice**")
    }
}