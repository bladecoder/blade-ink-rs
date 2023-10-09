use std::rc::Rc;

use crate::{container::Container, object::RTObject};

#[derive(Clone)]
pub struct SearchResult {
    pub obj: Rc<dyn RTObject>,
    pub approximate: bool,
}

impl SearchResult {
    pub fn new(obj: Rc<dyn RTObject>, approximate: bool) -> Self {
        SearchResult { obj, approximate }
    }

    pub fn from_search_result(sr: &SearchResult) -> Self {
        SearchResult {
            obj: sr.obj.clone(),
            approximate: sr.approximate,
        }
    }

    pub fn correct_obj(&self) -> Option<Rc<dyn RTObject>> {
        if self.approximate {
            None
        } else {
            Some(self.obj.clone())
        }
    }

    pub fn container(&self) -> Option<Rc<Container>> {
        let c = self.obj.clone().into_any().downcast::<Container>();

        match c {
            Ok(c) => Some(c),
            Err(_) => None,
        }
    }
}
