#[allow(unused_imports)]
use crate::prelude::*;

use crate::compat::{fmt, rc::Rc};

use crate::{container::Container, object::RTObject};

#[derive(Clone, Default)]
pub struct Pointer {
    pub container: Option<Rc<Container>>,
    pub index: i32,
}

impl Pointer {
    pub fn resolve(&self) -> Option<Rc<dyn RTObject>> {
        match &self.container {
            Some(container) => {
                if self.index < 0 || container.content.is_empty() {
                    return Some(container.clone());
                }

                match container.content.get(self.index as usize) {
                    Some(o) => Some(o.clone()),
                    None => None,
                }
            }
            None => None,
        }
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
