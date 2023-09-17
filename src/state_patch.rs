use std::{
    rc::Rc, collections::{HashMap, HashSet},
};

use crate::object::RTObject;

#[derive(Clone)]
pub(crate) struct StatePatch {
    pub globals: HashMap<String, Rc<dyn RTObject>>,
    pub changed_variables: HashSet<String>,
    pub visit_counts: HashMap<String, usize>,
    pub turn_indices: HashMap<String, usize>,
}

impl StatePatch {
    pub fn new(to_copy: Option<&StatePatch>) -> StatePatch {
        match to_copy {
            Some(to_copy) => StatePatch {
                globals: to_copy.globals.clone(),
                changed_variables: to_copy.changed_variables.clone(),
                visit_counts: to_copy.visit_counts.clone(),
                turn_indices: to_copy.turn_indices.clone(),
            },
            None => StatePatch {
                globals: HashMap::new(),
                changed_variables: HashSet::new(),
                visit_counts:  HashMap::new(),
                turn_indices:  HashMap::new(),
            },
        }
    }
}