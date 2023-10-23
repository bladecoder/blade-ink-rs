use crate::{container::Container, object::RTObject, threadsafe::Brc};

#[derive(Clone)]
pub struct SearchResult {
    pub obj: Brc<dyn RTObject>,
    pub approximate: bool,
}

impl SearchResult {
    pub fn new(obj: Brc<dyn RTObject>, approximate: bool) -> Self {
        SearchResult { obj, approximate }
    }

    pub fn from_search_result(sr: &SearchResult) -> Self {
        SearchResult {
            obj: sr.obj.clone(),
            approximate: sr.approximate,
        }
    }

    pub fn correct_obj(&self) -> Option<Brc<dyn RTObject>> {
        if self.approximate {
            None
        } else {
            Some(self.obj.clone())
        }
    }

    pub fn container(&self) -> Option<Brc<Container>> {
        let c = self.obj.clone().into_any().downcast::<Container>();

        match c {
            Ok(c) => Some(c),
            Err(_) => None,
        }
    }
}
