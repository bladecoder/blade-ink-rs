use std::rc::Rc;

use crate::{path::Path, callstack::Thread};

pub struct Choice {
    target_path: Path,
    is_invisible_default: bool,
    tags: Vec<String>,
    index: usize,
    original_thread_index: usize,
    text: String,
    thread_at_generation: Rc<Thread>,
    source_path: String
}

impl Choice {
    pub fn new() -> Choice {
        Choice {
            target_path: todo!(),
            is_invisible_default: todo!(),
            tags: todo!(),
            index: todo!(),
            original_thread_index: todo!(),
            text: todo!(),
            thread_at_generation: todo!(),
            source_path: todo!(),
        }
    }
}