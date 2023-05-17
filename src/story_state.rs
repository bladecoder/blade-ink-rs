use crate::{pointer::Pointer, callstack::CallStack};

pub struct StoryState {

}

impl StoryState {
    pub fn new() -> StoryState{
        StoryState {}
    }

    pub fn can_continue(&self) -> bool {
        return !self.get_current_pointer().is_null() && !self.has_error();
    }

    pub fn has_error(&self) -> bool {
        // TODO return currentErrors != null && currentErrors.size() > 0;
        false
    }

    fn get_current_pointer(&self) -> Pointer {
        return self.get_callstack().get_current_element().current_pointer.clone();
    }

    fn get_callstack(&self) -> CallStack {
        todo!()
    }

}