use std::{rc::Rc, fmt, cell::RefCell};

use crate::{container::Container, object::{RTObject, Object}, path::{Path, Component}, object_enum::ObjectEnum};

pub const NULL: Pointer = Pointer::new(None, -1);


#[derive(Clone)]
pub struct Pointer {
    container: Option<Rc<RefCell<Container>>>,
    index: i32,
}

impl Pointer {
    pub const fn new(container: Option<Rc<RefCell<Container>>>, index: i32) -> Pointer {
        Pointer { container, index }
    }

    pub fn resolve(&self) -> Option<ObjectEnum> {
        match &self.container {
            Some(container) => {
                if self.index < 0 || container.borrow().content.len() == 0 {
                    return Some(ObjectEnum::Container(container.clone()));
                }

                return match container.borrow().content.get(self.index as usize) {
                    Some(o) => Some(o.clone()),
                    None => None,
                };
            }
            None => None,
        }
    }

    pub fn is_null(&self) -> bool {
        self.container.is_none()
    }

    pub fn get_path(&self) -> Option<Path> {
        if self.is_null() {
            return None;
        }

        let container = ObjectEnum::Container(self.container.as_ref().unwrap().clone());

        if self.index >= 0 {
            let components: Vec<Component> = Vec::new();
            let c = Component::new_i(self.index as usize);

            return Some(Object::get_path(container)
                .path_by_appending_component(c)); 
        }

        Some(Object::get_path(container).clone())
    }

    pub(crate) fn start_of(container:Option<Rc<RefCell<Container>>>) -> Pointer {
        return Pointer{container, index:0};
    }
}

impl fmt::Display for Pointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.container {
            Some(container) => write!(f, "Ink Pointer -> {} -- index {}", Object::get_path(ObjectEnum::Container(self.container.as_ref().unwrap().clone())).to_string(), self.index),
            None => write!(f, "Ink Pointer (null)"),
        }
    }
}
