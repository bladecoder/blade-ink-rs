use std::{collections::HashMap, any::Any};

use crate::rt_object::RTObject;

pub struct Container {
    pub content: Vec<Box<dyn RTObject>>,
    pub name: Option<String>,
    pub count_flags: i32,
    //named_content: HashMap<String, Container>
}

impl Container {
    pub fn new(content: Vec<Box<dyn RTObject>>, name: Option<String>, count_flags: i32) -> Box<Self> {
        Box::new(Container{content, name, count_flags})
    }
}

impl RTObject for Container {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
