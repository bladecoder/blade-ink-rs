use std::rc::{Rc, Weak};

#[derive(Default)]
pub struct ParsedBase<T: ?Sized> {
    pub parent: Option<Weak<T>>,
}

pub type ParsedRef<T> = Rc<T>;
