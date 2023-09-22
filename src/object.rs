use core::fmt;
use std::{fmt::Display, rc::{Weak, Rc}, cell::RefCell, any::Any};

use as_any::{AsAny, Downcast};

use crate::{
    container::Container,
    path::{Component, Path},
    search_result::SearchResult
};

pub struct Object {
    parent: RefCell<Weak<Container>>,
    path: RefCell<Option<Path>>,
    //debug_metadata: DebugMetadata,
}

impl Object {
    pub fn new() -> Object {
        Object {
            parent: RefCell::new(Weak::new()),
            path: RefCell::new(None),
        }
    }

    pub fn is_root(&self) -> bool {
        self.parent.borrow().upgrade().is_none()
    }

    pub fn get_parent(&self) -> Option<Rc<Container>> {
        self.parent.borrow().upgrade()
    }

    pub fn set_parent(&self, parent: &Rc<Container>) {
        self.parent.replace(Rc::downgrade(parent));
    }

    pub fn get_path(rtobject: &dyn RTObject) -> Path {
        if let Some(p) = rtobject.get_object().path.borrow().as_ref() {
            return p.clone();
        }

        match rtobject.get_object().get_parent() {
            Some(_) => {
                let mut comps: Vec<Component> = Vec::new();
                
                let mut container = rtobject.get_object().get_parent();
                let mut child = rtobject.clone();
                let mut child_rc = None;

                while let Some(c) = container {
                    let mut child_valid_name = false;

                    if let Some(cc) = child.downcast_ref::<Container>() {
                        if cc.has_valid_name() {
                            child_valid_name = true;
                            comps.push(Component::new(cc.name.as_ref().unwrap()));
                        }
                    }

                    if !child_valid_name {
                        comps.push(Component::new_i(
                            c.content
                                .iter()
                                .position(|r| {
                                    let a = r.as_ref() as *const _ as *const ();
                                    let b = child as *const _ as *const ();
                                    std::ptr::eq(a, b)
                                }).unwrap(),
                        ));
                    }

                    container = c.get_object().get_parent();
                    child_rc = Some(c);
                    child = child_rc.as_ref().unwrap().as_ref();
                }

                // Reverse list because components are searched in reverse order.
                comps.reverse();

                rtobject.get_object().path.replace(Some(Path::new(&comps, Path::default().is_relative())));
            },
            None => {
                rtobject.get_object().path.replace(Some(Path::new_with_defaults()));
            },
        }

        rtobject.get_object().path.borrow().as_ref().unwrap().clone()
    }

    pub fn resolve_path(rtobject: Rc<dyn RTObject>, path: &Path) -> SearchResult {
        if path.is_relative() {
            let mut p = path.clone();
            let mut nearest_container = rtobject.clone().into_any().downcast::<Container>().ok();
            
            if nearest_container.is_none() {
                nearest_container = rtobject.get_object().get_parent();
                p = path.get_tail();
            };

            return nearest_container.unwrap().content_at_path(&p, 0, -1);
    
        } else {
            Object::get_root_container(rtobject).content_at_path(path, 0, -1)
        }
    }

    pub fn convert_path_to_relative(rtobject: &Rc<dyn RTObject>, global_path: &Path) -> Path {
        // 1. Find last shared ancestor
        // 2. Drill up using ".." style (actually represented as "^")
        // 3. Re-build downward chain from common ancestor
        let own_path = rtobject.get_object().path.borrow();
        let min_path_length = std::cmp::min(global_path.len(), own_path.as_ref().unwrap().len());
        let mut last_shared_path_comp_index:i32 = -1;

        for i in 0..min_path_length {
            let own_comp = &own_path.as_ref().unwrap().get_component(i as usize);
            let other_comp = &global_path.get_component(i);

            if own_comp == other_comp {
                last_shared_path_comp_index = i as i32;
            } else {
                break;
            }
        }

        // No shared path components, so just use the global path
        if last_shared_path_comp_index == -1 {
            return global_path.clone();
        }

        let num_upwards_moves = (own_path.as_ref().unwrap().len() - 1) - last_shared_path_comp_index as usize;
        let mut new_path_comps = Vec::new();

        for _ in 0..num_upwards_moves {
            new_path_comps.push(Component::to_parent());
        }

        for down in (last_shared_path_comp_index as usize + 1)..global_path.len() {
            new_path_comps.push(global_path.get_component(down).unwrap().clone());
        }

        Path::new(&new_path_comps, true)
    }

    pub fn compact_path_string(rtobject: Rc<dyn RTObject>, other_path: &Path) -> String {
        let global_path_str: String;
        let relative_path_str: String;
    
        if other_path.is_relative() {
            relative_path_str = other_path.get_components_string();
            global_path_str = Object::get_path(rtobject.as_ref()).path_by_appending_path(other_path).get_components_string();
        } else {
            let relative_path = Object::convert_path_to_relative(&rtobject, other_path);
            relative_path_str = relative_path.get_components_string();
            global_path_str = other_path.get_components_string();
        }
    
        if relative_path_str.len() < global_path_str.len() {
            relative_path_str
        } else {
            global_path_str
        }
    }

    pub fn get_root_container(rtobject: Rc<dyn RTObject>) -> Rc<Container> {
        let mut ancestor = rtobject;

        while let Some(p) = ancestor.get_object().get_parent() {
            ancestor =  p;
        }

        match ancestor.into_any().downcast::<Container>() {
            Ok(c) => c.clone(),
            _ => panic!("Impossible")
        }
    }
}

pub trait IntoAny: AsAny {
    fn into_any(self: Rc<Self>) -> Rc<dyn Any>;
}

impl<T: Any> IntoAny for T {
    #[inline(always)]
    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

pub trait RTObject: Display + IntoAny {
    fn get_object(&self) -> &Object;
}

// TODO Temporal RTObject. Maybe we sould return Optional::None in null json.
pub struct Null {
    obj: Object,
}

impl Null {
    pub fn new() -> Null {
        Null {
            obj: Object::new(),
        }
    }
}

impl RTObject for Null {
    fn get_object(&self) -> &Object {
        &self.obj
    }
}

impl fmt::Display for Null {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "**Null**")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn get_path_test() {
        let container1 = Container::new(None, 0, Vec::new(), HashMap::new());
        let container21 = Container::new(None, 0, Vec::new(), HashMap::new());
        let container2 = Container::new(None, 0, vec![container21.clone()], HashMap::new());
        let root = Container::new(None, 0, vec![container1.clone(), container2.clone()], HashMap::new());

        let mut sb = String::new();

        root.build_string_of_hierarchy(&mut  sb, 0, None);

        println!("root c:{:p}", &*root);
        println!("container1 p:{:p} c:{:p}", &*(container1.get_object().get_parent().unwrap()), &*container1);
        println!("container2 p:{:p} c:{:p}", &*(container2.get_object().get_parent().unwrap()), &*container2);
        println!("container21 p:{:p} c:{:p}", &*(container21.get_object().get_parent().unwrap()), &*container21);

        println!("root: {}", sb);

        assert_eq!(Object::get_path(container1.as_ref()).to_string(), "0");
        assert_eq!(Object::get_path(container2.as_ref()).to_string(), "1");
        assert_eq!(Object::get_path(container21.as_ref()).to_string(), "1.0");
        assert_eq!(Object::get_path(root.as_ref()).to_string(), "");
    }
}
