//! A generated Choice from the story.
use core::fmt;
use std::cell::RefCell;

use crate::{
    callstack::Thread,
    object::{Object, RTObject},
    path::Path,
};

pub struct Choice {
    obj: Object,
    thread_at_generation: RefCell<Option<Thread>>,
    pub(crate) original_thread_index: RefCell<usize>,
    /// Get the path to the original choice point - where was this choice defined in the story?
    pub(crate) source_path: String,
    pub(crate) target_path: Path,
    pub(crate) is_invisible_default: bool,
    pub tags: Vec<String>,
    /// The original index into currentChoices list on the Story when
    /// this Choice was generated, for convenience.
    pub index: RefCell<usize>,
    /// The main text to presented to the player for this Choice.
    pub text: String,
}

impl Choice {
    pub(crate) fn new(
        target_path: Path,
        source_path: String,
        is_invisible_default: bool,
        tags: Vec<String>,
        thread_at_generation: Thread,
        text: String,
    ) -> Choice {
        Self {
            obj: Object::new(),
            target_path,
            is_invisible_default,
            tags,
            index: RefCell::new(0),
            original_thread_index: RefCell::new(0),
            text,
            thread_at_generation: RefCell::new(Some(thread_at_generation)),
            source_path,
        }
    }

    pub(crate) fn new_from_json(
        path_string_on_choice: &str,
        source_path: String,
        text: &str,
        index: usize,
        original_thread_index: usize,
    ) -> Choice {
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

    pub(crate) fn set_thread_at_generation(&self, thread: Thread) {
        self.thread_at_generation.replace(Some(thread));
    }

    pub(crate) fn get_thread_at_generation(&self) -> Option<Thread> {
        self.thread_at_generation
            .borrow()
            .as_ref()
            .map(|t| t.copy())
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
