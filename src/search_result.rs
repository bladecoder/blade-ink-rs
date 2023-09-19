use std::rc::Rc;

use crate::{object::RTObject, container::Container};


#[derive(Clone)]
pub struct SearchResult {
    pub obj: Rc<dyn RTObject>,
    pub approximate: bool,
}

impl SearchResult {
    pub fn new(obj: Rc<dyn RTObject>, approximate: bool) -> Self {
        SearchResult {
            obj,
            approximate,
        }
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

    pub fn get_container(&self) -> Option<Rc<Container>> {
        match self.obj.clone().into_any().downcast::<Container>() {
            Ok(c) => Some(c),
            Err(_) => None,
        }
    }
}