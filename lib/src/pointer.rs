use std::{fmt, rc::Rc};

use crate::{
    container::Container,
    object::RTObject,
    path::{Component, Path},
};

pub const NULL: Pointer = Pointer::new(None, -1);

#[derive(Clone, Default)]
pub struct Pointer {
    pub container: Option<Rc<Container>>,
    pub index: i32,
}

impl Pointer {
    pub const fn new(container: Option<Rc<Container>>, index: i32) -> Pointer {
        Pointer { container, index }
    }

    pub fn resolve(&self) -> Option<Rc<dyn RTObject>> {
        match &self.container {
            Some(container) => {
                if self.index < 0 || container.content.is_empty() {
                    return Some(container.clone());
                }

                return match container.content.get(self.index as usize) {
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

        let container = self.container.as_ref().unwrap();

        if self.index >= 0 {
            let c = Component::new_i(self.index as usize);

            return Some(container.get_path().path_by_appending_component(c));
        }

        Some(container.get_path())
    }

    pub fn start_of(container: Rc<Container>) -> Pointer {
        Pointer {
            container: Some(container),
            index: 0,
        }
    }
}

impl fmt::Display for Pointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.container {
            Some(container) => write!(
                f,
                "Ink Pointer -> {} -- index {}",
                container.get_path(),
                self.index
            ),
            None => write!(f, "Ink Pointer (null)"),
        }
    }
}
