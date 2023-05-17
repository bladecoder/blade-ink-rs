use core::fmt;
use std::{fmt::Display, rc::Rc, cell::RefCell};

use as_any::AsAny;

use crate::{
    container::Container,
    path::{Component, Path},
    search_result::SearchResult, object_enum::ObjectEnum,
};

pub struct Object {
    pub parent: Option<Rc<RefCell<Container>>>,
    path: Option<Path>,
    //debug_metadata: DebugMetadata,
}

impl Object {
    pub fn new() -> Object {
        Object {
            parent: None,
            path: None
        }
    }

    pub fn is_root(&self) -> bool {
        self.parent.is_none()
    }

    pub fn get_path(oe: ObjectEnum) -> &'static Path {
        match oe.get_obj().parent {
            Some(_) => {
                let mut comps: Vec<Component> = Vec::new();
                let mut child = oe;

                let mut container = child.get_obj().parent;

                while let Some(c) = container {
                    let mut child_valid_name = false;

                    if let ObjectEnum::Container(cc) = child {
                        if cc.borrow().has_valid_name() {
                            child_valid_name = true;
                            comps.push(Component::new(cc.borrow().get_name()));
                        }
                    }

                    if !child_valid_name {
                        comps.push(Component::new_i(
                            c.borrow().content
                                .iter()
                                .position(|r| r as *const _ == &child )
                                .unwrap(),
                        ));
                    }


                    child = ObjectEnum::Container(c);
                    container = c.borrow().get_object().parent;
                }

                // Reverse list because components are searched in reverse order.
                comps.reverse();

                oe.get_obj().path = Some(Path::new(&comps, Path::default().is_relative()))
            },
            None => oe.get_obj().path = Some(Path::new_with_defaults()),
        }

        oe.get_obj().path.as_ref().unwrap()
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

    pub fn get_root_container(oe: ObjectEnum) -> Rc<RefCell<Container>> {
        let mut ancestor = oe;

        while let Some(p) = ancestor.get_obj().parent {
            ancestor =  ObjectEnum::Container(p);
        }

        match ancestor {
            ObjectEnum::Container(c) => c,
            _ => panic!("Impossible")
        }
    }
}

pub trait RTObject: Display + AsAny {
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
