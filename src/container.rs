use std::{
    fmt,
    rc::Rc, collections::HashMap,
};

use as_any::Downcast;

use crate::{
    object::{Object, RTObject},
    value::{ValueType, Value}, path::{Path, Component}, search_result::SearchResult,
};

const COUNTFLAGS_VISITS: i32 = 1;
const COUNTFLAGS_TURNS: i32 = 2;
const COUNTFLAGS_COUNTSTARTONLY: i32 = 4;

pub struct Container {
    obj: Object,
    pub name: Option<String>,
    pub content: Vec<Rc<dyn RTObject>>,
    pub named_content: HashMap<String, Rc<Container>>,
    pub visits_should_be_counted: bool,
    pub turn_index_should_be_counted: bool,
    pub counting_at_start_only: bool,
}

impl Container {
    pub fn new(name: Option<String>, count_flags: i32, content: Vec<Rc<dyn RTObject>>, named_content: HashMap<String, Rc<Container>>) -> Rc<Container> {

        let mut named_content = named_content;
        
        content.iter().for_each(|o| {
            if let Ok(c) = o.clone().into_any().downcast::<Container>() {
                if c.has_valid_name() {
                    named_content.insert(c.name.as_ref().unwrap().to_string(), c);
                }
            }
        });

        let (visits_should_be_counted, turn_index_should_be_counted, counting_at_start_only) = Container::split_count_flags(count_flags);

        let c = Rc::new(Container {
            obj: Object::new(),
            content,
            named_content,
            name,
            visits_should_be_counted: visits_should_be_counted,
            turn_index_should_be_counted: turn_index_should_be_counted,
            counting_at_start_only: counting_at_start_only,
        });

        c.content.iter().for_each(|o| o.get_object().set_parent(&c));
        c.named_content.values().for_each(|o| o.get_object().set_parent(&c));

        c
    }

    pub fn has_valid_name(&self) -> bool {
        self.name.is_some() && !self.name.as_ref().unwrap().is_empty()
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
            } else if let Some(v) = obj.as_ref().downcast_ref::<Value>() {
                Container::append_indentation(sb, indentation);
                if let ValueType::String(s) = &v.value {
                    sb.push('\"');
                    sb.push_str(&&s.string.replace('\n', "\\n"));
                    sb.push('\"');
                } else {
                    sb.push_str(&v.to_string());
                }
            } else {
                Container::append_indentation(sb, indentation);
                sb.push_str(&obj.to_string());
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

            sb.push_str("  (");
            sb.push_str(&Object::get_path(obj.as_ref()).to_string());
            sb.push(')');

            sb.push('\n');
        }

        let mut only_named: HashMap<String,Rc<Container>> = HashMap::new();

        for  (k, v) in self.named_content.iter() {
            let o: Rc<dyn RTObject> = v.clone();
            if self.content.iter().any(|e| Rc::ptr_eq(e, &o)) {
                continue;
            } else {
                only_named.insert(k.clone(), v.clone());
            }
        }

       

        if only_named.len() > 0 {
            Container::append_indentation(sb, indentation);

            sb.push_str("-- named: --\n");

            for v in only_named.values() {
                // Debug.Assert(objKV.Value instanceof Container, "Can only
                // print out named Containers");
                v.build_string_of_hierarchy(sb, indentation, pointed_obj);
                sb.push('\n');
            }
        }

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

    pub fn get_path(self: &Rc<Self>) -> Path {
        Object::get_path(self.as_ref())
    }

    pub fn content_at_path(
        self: &Rc<Self>,
        path: &Path,
        partial_path_start: usize,
        mut partial_path_length: i32,
    ) -> SearchResult {

        if partial_path_length == -1 {
            partial_path_length = path.len() as i32;
        }  
       
        let mut approximate = false;
    
        let mut current_container = Some(self.clone());
        let mut current_obj:Rc<dyn RTObject> = self.clone();
    
        for i in partial_path_start..partial_path_length as usize {
            let comp = path.get_component(i);
    
            // Path component was wrong type
            if current_container.is_none() {
                approximate = true;
                break;
            }
    
            let found_obj = current_container
                .unwrap()
                .content_with_path_component(comp.unwrap());
    
            // Couldn't resolve entire path?
            if found_obj.is_none() {
                approximate = true;
                break;
            }

            current_obj = found_obj.unwrap().clone();
            current_container = if let Ok(container) = current_obj.clone().into_any().downcast::<Container>() {


                Some(container)
            } else {
                None
            };
        }

    
        SearchResult::new(current_obj, approximate)
    }
    

    pub fn get_count_flags(&self) -> i32 {
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

    fn content_with_path_component(&self, component: &Component) -> Option<Rc<dyn RTObject>> {
        if component.is_index() {
            if let Some(index) = component.index {
                if index < self.content.len() {
                    return Some(self.content[index].clone());
                }
            }
        } else if component.is_parent() {
            // When path is out of range, quietly return None
            // (useful as we step/increment forwards through content)
            return match self.get_object().get_parent() {
                Some(o) => Some(o as Rc<dyn RTObject>),
                None => None,
            } 
        } else if let Some(found_content) = self.named_content.get(component.name.as_ref().unwrap()) {
            return Some(found_content.clone());
        }

        None    
    }

    pub fn get_named_only_content(&self) -> HashMap<String, Rc<Container>> {
        let mut named_only_content_dict = HashMap::new();
    
        for (key, value) in self.named_content.iter() {
            named_only_content_dict.insert(key.clone(), value.clone());
        }
    
        for c in &self.content {
            if let Some(named) = c.as_any().downcast_ref::<Container>() {
                if named.has_valid_name() {
                    named_only_content_dict.remove(named.name.as_ref().unwrap());
                }
            }
        }
    
        named_only_content_dict
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