use std::{
    fmt,
    rc::Rc,
};

use as_any::Downcast;

use crate::{
    object::{Object, RTObject, Null},
    value::{ValueType, Value}, control_command::ControlCommand,
};

const COUNTFLAGS_VISITS: i32 = 1;
const COUNTFLAGS_TURNS: i32 = 2;
const COUNTFLAGS_COUNTSTARTONLY: i32 = 4;

pub struct Container {
    obj: Object,
    pub name: Option<String>,
    pub content: Vec<Rc<dyn RTObject>>,
    //named_content: HashMap<String, Container>
    pub visits_should_be_counted: bool,
    pub turn_index_should_be_counted: bool,
    pub counting_at_start_only: bool,
}

impl Container {
    pub fn new(name: Option<String>, count_flags: i32, content: Vec<Rc<dyn RTObject>>, ) -> Rc<Container> {

        let (visits_should_be_counted, turn_index_should_be_counted, counting_at_start_only) = Container::split_count_flags(count_flags);

        let c = Rc::new(Container {
            obj: Object::new(),
            content,
            name,
            visits_should_be_counted: visits_should_be_counted,
            turn_index_should_be_counted: turn_index_should_be_counted,
            counting_at_start_only: counting_at_start_only,
        });

        c.content.iter().for_each(|o| o.get_object().set_parent(&c));

        c
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
        pointed_obj: Option<&dyn RTObject>,
    ) {
        Container::append_indentation(sb, indentation);

        sb.push('[');

        if self.has_valid_name() {
            sb.push_str(" ({");
            sb.push_str(self.name.as_ref().unwrap());
            sb.push_str("})");
        }

        if let Some(pointed_obj) = pointed_obj {
            if let Some(c) = pointed_obj.downcast_ref::<Container>() {
                if std::ptr::eq(c, self) {
                    sb.push_str("  <---");
                }
            }
        }

        sb.push('\n');
        let indentation = indentation + 1;

        for (i, obj) in self.content.iter().enumerate() {
            if let Some(c) = obj.as_ref().downcast_ref::<Container>() {
                c.build_string_of_hierarchy(sb, indentation, pointed_obj);
            }

            if let Some(v) = obj.as_ref().downcast_ref::<Value>() {
                Container::append_indentation(sb, indentation);
                if let ValueType::String(s) = &v.value {
                    sb.push('\"');
                    sb.push_str(&&s.string.replace('\n', "\\n"));
                    sb.push('\"');
                } else {
                    sb.push_str(&v.to_string());
                }
            }

            if let Some(cc) = obj.as_ref().downcast_ref::<ControlCommand>() {
                Container::append_indentation(sb, indentation);
                sb.push_str(&cc.to_string());
            }

            if let Some(n) = obj.as_ref().downcast_ref::<Null>() {
                sb.push_str(&n.to_string());
            }

            if i != self.content.len() - 1 {
                sb.push(',');
            }

            if let Some(pointed_obj) = pointed_obj {
                if !pointed_obj.is::<Container>() {
                    if std::ptr::eq(obj.as_ref(), pointed_obj) {
                        sb.push_str("  <---");
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

    pub(crate) fn get_count_flags(&self) -> i32 {
        let mut flags: i32 = 0;
    
        if self.visits_should_be_counted {
            flags |= COUNTFLAGS_VISITS 
        }
    
        if self.turn_index_should_be_counted {
             flags |= COUNTFLAGS_TURNS;
        }
    
        if self.counting_at_start_only {
            flags |= COUNTFLAGS_COUNTSTARTONLY;
        }
    
        // If we're only storing CountStartOnly, it serves no purpose,
        // since it's dependent on the other two to be used at all.
        // (e.g. for setting the fact that *if* a gather or choice's
        // content is counted, then is should only be counter at the start)
        // So this is just an optimisation for storage.
        if flags == COUNTFLAGS_COUNTSTARTONLY {
            flags = 0;
        }
    
        return flags;
    }

    fn split_count_flags(value: i32) -> (bool, bool, bool) {

        let visits_should_be_counted = if (value & COUNTFLAGS_VISITS) > 0 { true } else { false} ;
    
        let turn_index_should_be_counted = if (value & COUNTFLAGS_TURNS) > 0 { true } else { false} ;
    
        let counting_at_start_only = if (value & COUNTFLAGS_COUNTSTARTONLY) > 0 { true } else { false} ;
    
        (visits_should_be_counted, turn_index_should_be_counted, counting_at_start_only)
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