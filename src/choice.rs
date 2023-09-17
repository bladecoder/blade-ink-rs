use core::fmt;

use crate::{path::Path, callstack::Thread, object::{Object, RTObject}};

pub struct Choice {
    obj: Object,
    pub target_path: Path,
    pub is_invisible_default: bool,
    pub tags: Vec<String>,
    pub index: usize,
    pub original_thread_index: usize,
    pub text: String,
    pub(crate) thread_at_generation: Thread,
    pub source_path: String
}

impl Choice {
    pub(crate) fn new(target_path: Path, source_path: String, is_invisible_default: bool, tags: Vec<String>, thread_at_generation: Thread, text: String, index: usize, original_thread_index: usize) -> Choice {
        Choice {
            obj: Object::new(),
            target_path: target_path,
            is_invisible_default: is_invisible_default,
            tags: tags,
            index: index,
            original_thread_index: original_thread_index,
            text: text,
            thread_at_generation: thread_at_generation,
            source_path: source_path,
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