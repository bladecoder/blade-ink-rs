use core::fmt;
use std::{fmt::Display, rc::{Weak, Rc}, cell::RefCell, any::Any, borrow::BorrowMut};

use as_any::{AsAny, Downcast};

use crate::{
    container::Container,
    path::{Component, Path},
    search_result::SearchResult
};

pub struct Object {
    parent: RefCell<Weak<Container>>,
    path: RefCell<Option<Rc<Path>>>,
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

    pub(crate) fn set_parent(&self, parent: &Rc<Container>) {
        self.parent.replace(Rc::downgrade(parent));
    }

    pub fn get_path(rtobject: Rc<dyn RTObject>) -> Rc<Path> {
        if let Some(p) = rtobject.get_object().path.borrow().as_ref() {
            return p.clone();
        }

        match rtobject.get_object().get_parent() {
            Some(_) => {
                let mut comps: Vec<Component> = Vec::new();
                
                let mut container = rtobject.get_object().get_parent();
                let mut child = rtobject.clone();

                while let Some(c) = container {
                    let mut child_valid_name = false;

                    if let Some(cc) = child.as_ref().downcast_ref::<Container>() {
                        if cc.has_valid_name() {
                            child_valid_name = true;
                            comps.push(Component::new(cc.get_name()));
                        }
                    }

                    if !child_valid_name {
                        comps.push(Component::new_i(
                            c.content
                                .iter()
                                .position(|r| Rc::ptr_eq( r, &child) )
                                .unwrap(),
                        ));
                    }

                    container = c.get_object().get_parent();
                    child = c;
                }

                // Reverse list because components are searched in reverse order.
                comps.reverse();

                rtobject.get_object().path.replace(Some(Rc::new(Path::new(&comps, Path::default().is_relative()))));
            },
            None => {
                rtobject.get_object().path.replace(Some(Rc::new(Path::new_with_defaults())));
            },
        }

        rtobject.get_object().path.borrow().as_ref().unwrap().clone()
    }


    pub fn resolve_path(&self) -> Result<SearchResult, String> {
        todo!()
    }

    pub fn convert_path_to_relative(&self, global_path: Path) -> Path {
        todo!()
    }

    pub fn compact_path_string(&self, other_path: Path) -> Path {
        todo!()
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
    pub(crate) fn new() -> Null {
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
    use super::*;

    #[test]
    fn get_path_test() {
        let container1 = Container::new(None, 0, Vec::new());
        let container21 = Container::new(None, 0, Vec::new());
        let container2 = Container::new(None, 0, vec![container21.clone()]);
        let root = Container::new(None, 0, vec![container1.clone(), container2.clone()]);

        let mut sb = String::new();

        root.build_string_of_hierarchy(&mut  sb, 0, None);

        println!("root c:{:p}", &*root);
        println!("container1 p:{:p} c:{:p}", &*(container1.get_object().get_parent().unwrap()), &*container1);
        println!("container2 p:{:p} c:{:p}", &*(container2.get_object().get_parent().unwrap()), &*container2);
        println!("container21 p:{:p} c:{:p}", &*(container21.get_object().get_parent().unwrap()), &*container21);

        println!("root: {}", sb);

        assert_eq!(Object::get_path(container1).to_string(), "0");
        assert_eq!(Object::get_path(container2).to_string(), "1");
        assert_eq!(Object::get_path(container21).to_string(), "1.0");
        assert_eq!(Object::get_path(root).to_string(), "");
    }
}