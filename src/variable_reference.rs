use std::{fmt, rc::Rc};

use crate::{object::{Object, RTObject}, path::Path, container::Container};


pub struct VariableReference {
    obj: Object,
    pub name: String,
    pub path_for_count: Option<Path>,    
}

impl VariableReference {
    pub fn new(name: &str) -> Self {
        VariableReference {obj: Object::new(), name: name.to_string(), path_for_count: None}
    }

    pub fn from_path_for_count(path_for_count: &str) -> Self {
        VariableReference {obj: Object::new(), name: String::new(), path_for_count:  Some(Path::new_with_components_string(Some(path_for_count)))}
    }

    pub fn get_container_for_count(self: &Rc<Self>) -> Result<Rc<Container>, String> {
        if let Some(path) = &self.path_for_count {
            Ok(Object::resolve_path(self.clone(), path).container().unwrap())
        } else {
            Err("Path for count is not set.".to_owned())
        }
    }

    pub fn get_path_string_for_count(self: Rc<Self>) -> Option<String> {
        if let Some(path_for_count) = &self.path_for_count {
            Some(Object::compact_path_string(self.clone(), path_for_count))
        } else {
            None
        }
    }
}

impl RTObject for VariableReference {
    fn get_object(&self) -> &Object {
        &self.obj
    }
}

impl fmt::Display for VariableReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            name if !name.is_empty() => write!(f, "var({})", name),
            _ => match &self.path_for_count {
                Some(path) => write!(f, "read_count({})", &path.to_string()), // TODO needs an RC path.compact_path_string(path)),
                None => write!(f, "read_count(null)"),
            },
        }
    }
}