use std::{
    cell::{RefCell, Ref},
    fmt,
    rc::Rc,
};

use crate::{
    object::{Object, RTObject},
    object_enum::ObjectEnum,
    value::ValueType,
};

pub struct Container {
    obj: Object,
    pub content: Vec<ObjectEnum>,
    pub name: Option<String>,
    pub count_flags: i32,
    //named_content: HashMap<String, Container>
}

impl Container {
    pub fn new(name: Option<String>, count_flags: i32) -> Container {
        Container {
            obj: Object::new(),
            content: Vec::new(),
            name,
            count_flags,
        }
    }

    pub fn add_contents(container: &Rc<RefCell<Container>>, objs: &Vec<ObjectEnum>) {
        objs.iter()
            .for_each(|o| Container::add_content(container, o));
    }

    pub fn add_content(container: &Rc<RefCell<Container>>, obj: &ObjectEnum) {
        container.as_ref().borrow_mut().content.push(obj.clone());
        obj.get_obj_mut().parent = Some(container.clone());
    }

    pub fn has_valid_name(&self) -> bool {
        self.name.is_some() && !self.name.as_ref().unwrap().is_empty()
    }

    pub(crate) fn get_name(&self) -> &str {
        todo!()
    }

    pub fn build_string_of_hierarchy(
        &self,
        sb: &mut String,
        indentation: usize,
        pointed_obj: Option<ObjectEnum>,
    ) {
        Container::append_indentation(sb, indentation);

        sb.push('[');

        if self.has_valid_name() {
            sb.push_str(" ({");
            sb.push_str(self.name.as_ref().unwrap());
            sb.push_str("})");
        }

        if let Some(pointed_obj) = pointed_obj {
            if let ObjectEnum::Container(obj) = pointed_obj {
                if std::ptr::eq(obj.as_ptr(), self) {
                    sb.push_str("  <---");
                }
            }
        }

        sb.push('\n');
        let indentation = indentation + 1;

        for (i, obj) in self.content.iter().enumerate() {
            match obj {
                ObjectEnum::Container(c) => {
                    c.as_ref()
                        .borrow()
                        .build_string_of_hierarchy(sb, indentation, pointed_obj);
                }

                ObjectEnum::Value(v) => {
                    Container::append_indentation(sb, indentation);
                    if let ValueType::String(s) = v.as_ref().borrow().value {
                        sb.push('\"');
                        sb.push_str(&s.clone().replace('\n', "\\n"));
                        sb.push('\"');
                    } else {
                        sb.push_str(&v.as_ref().borrow().to_string());
                    }
                }

                ObjectEnum::ControlCommand(o) => {
                    sb.push_str(&o.as_ref().borrow().to_string());
                }

                ObjectEnum::Null(o) => {
                    sb.push_str(&o.as_ref().borrow().to_string());
                }
            }

            if i != self.content.len() - 1 {
                sb.push(',');
            }

            if let Some(pointed_obj) = pointed_obj {
                if let ObjectEnum::Container(pointed_obj) = pointed_obj {
                    if let ObjectEnum::Container(obj) = obj {
                        if &obj.as_ref().borrow() as *const _
                            == &pointed_obj.as_ref().borrow() as *const _
                        {
                            sb.push_str("  <---");
                        }
                    }
                }
            }

            sb.push('\n');
        }

        /* TODO
        HashMap<String, INamedContent> onlyNamed = new HashMap<String, INamedContent>();

        for (Entry<String, INamedContent> objKV : getNamedContent().entrySet()) {
            if (getContent().contains(objKV.getValue())) {
                continue;
            } else {
                onlyNamed.put(objKV.getKey(), objKV.getValue());
            }
        }

        if (onlyNamed.size() > 0) {
            appendIndentation(sb, indentation);

            sb.append("-- named: --\n");

            for (Entry<String, INamedContent> objKV : onlyNamed.entrySet()) {
                // Debug.Assert(objKV.Value instanceof Container, "Can only
                // print out named Containers");
                Container container = (Container) objKV.getValue();
                container.buildStringOfHierarchy(sb, indentation, pointedObj);
                sb.append("\n");
            }
        }
        */

        let indentation = indentation - 1;
        Container::append_indentation(sb, indentation);
        sb.push(']');
    }

    fn append_indentation(sb: &mut String, indentation: usize) {
        const SPACES_PER_INDENT: usize = 4;

        for _ in 0..(SPACES_PER_INDENT * indentation) {
            sb.push(' ');
        }
    }
}

impl RTObject for Container {
    fn get_object(&self) -> &Object {
        &self.obj
    }
}

impl fmt::Display for Container {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "**Container**")
    }
}
