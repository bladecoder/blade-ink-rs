use std::{
    fmt, rc::Rc, cell::RefCell,
};

use crate::{object::{Object, RTObject}, push_pop::PushPopType, pointer::{Pointer, self}, path::{Path, Component}, container::Container};


pub struct Divert {
    obj: Object,
    pub external_args: usize,
    pub is_conditional: bool,
    pub is_external: bool,
    pub pushes_to_stack: bool,
    pub stack_push_type: PushPopType,
    target_pointer: RefCell<Pointer>,
    target_path: RefCell<Option<Path>>,
    pub variable_divert_name: Option<String>,    
}

impl Divert {
    pub fn new(pushes_to_stack: bool, stack_push_type: PushPopType, is_external: bool, external_args: usize, is_conditional: bool, var_divert_name: Option<String>, target_path: Option<&str>) -> Self {
        Divert {
            obj: Object::new(),
            is_conditional,
            pushes_to_stack,
            stack_push_type,
            is_external,
            external_args,
            target_pointer: RefCell::new(pointer::NULL.clone()),
            target_path: RefCell::new(Self::target_path_string(target_path)),
            variable_divert_name: var_divert_name,
        }
    }

    fn target_path_string(value: Option<&str>) -> Option<Path>{
        value.map(|value| Path::new_with_components_string(Some(value)))
    }

    pub fn get_target_path_string(self: &Rc<Self>) -> Option<String> {
        self.get_target_path().as_ref().map(|p| self.compact_path_string(p))
    }

    pub fn has_variable_target(&self) -> bool {
        self.variable_divert_name.is_some()
    }

    fn compact_path_string(&self, other_path: &Path) -> String {
        let global_path_str;
        let relative_path_str;
        
        if other_path.is_relative() {
            relative_path_str = other_path.get_components_string();
            global_path_str = Object::get_path(self).path_by_appending_path(other_path).get_components_string();
        } else {
            let relative_path = self.convert_path_to_relative(other_path);
            relative_path_str = relative_path.get_components_string();
            global_path_str = other_path.get_components_string();
        }
        
        if relative_path_str.len() < global_path_str.len() {
            relative_path_str.clone()
        } else {
            global_path_str.clone()
        }
    }

    pub fn get_target_pointer(self: &Rc<Self>) -> Pointer {
        let target_pointer_null = self.target_pointer.borrow().is_null();
        if target_pointer_null {
            let target_obj = Object::resolve_path(self.clone(), self.target_path.borrow().as_ref().unwrap()).obj.clone();
    
            if self.target_path.borrow().as_ref().unwrap().get_last_component().unwrap().is_index() {
                self.target_pointer.borrow_mut().container = target_obj.get_object().get_parent();
                self.target_pointer.borrow_mut().index = self.target_path.borrow().as_ref().unwrap().get_last_component().unwrap().index.unwrap() as i32;
            } else {
                let c = target_obj.into_any().downcast::<Container>();
                self.target_pointer.replace(Pointer::start_of(c.unwrap()));
            }
        }
        
        self.target_pointer.borrow().clone()
    }
    
    pub fn get_target_path(self: &Rc<Self>) -> Option<Path> {
        // Resolve any relative paths to global ones as we come across them
        let target_path = self.target_path.borrow();
        
        match target_path.as_ref() {
            Some(target_path) => {
                    if target_path.is_relative() {
                        let target_obj = self.get_target_pointer().resolve();
            
                        if let Some(target_obj) = target_obj {
                            self.target_path.replace(Some(Object::get_path(target_obj.as_ref())));
                        }
                    }
                    Some(self.target_path.borrow().as_ref().unwrap().clone())
                },
            None => None,
        }   
    }
    

    fn convert_path_to_relative(&self, global_path: &Path) -> Path {
        let own_path = Object::get_path(self);
        let min_path_length = std::cmp::min(global_path.len(), own_path.len());
        let mut last_shared_path_comp_index: i32 = -1;
    
        for i in 0..min_path_length {
            let own_comp = own_path.get_component(i);
            let other_comp = global_path.get_component(i);
    
            if own_comp.eq(&other_comp) {
                last_shared_path_comp_index = i as i32;
            } else {
                break;
            }
        }
    
        if last_shared_path_comp_index == -1 {
            return global_path.clone();
        }
    
        let num_upwards_moves = (own_path.len() - 1) - last_shared_path_comp_index as usize;
        let mut new_path_comps = Vec::new();
    
        for _ in 0..num_upwards_moves {
            new_path_comps.push(Component::to_parent());
        }
    
        for down in (last_shared_path_comp_index  as usize + 1)..global_path.len() {
            new_path_comps.push(global_path.get_component(down).unwrap().clone());
        }
    
        Path::new(&new_path_comps, true)
    }
}

impl RTObject for Divert {
    fn get_object(&self) -> &Object {
        &self.obj
    }
}

impl fmt::Display for Divert {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut result = String::new();

        if let Some(variable_diver_name) = &self.variable_divert_name {
            result.push_str(&format!("Divert(variable: {})", variable_diver_name));
        } else if self.target_path.borrow().is_none() {
            result.push_str("Divert(null)");
        } else {
            let target_str = self.target_path.borrow().as_ref().unwrap().get_components_string();

            result.push_str("Divert");

            if self.is_conditional {
                result.push('?');
            }

            if self.pushes_to_stack {
                if self.stack_push_type == PushPopType::Function {
                    result.push_str(" function");
                } else {
                    result.push_str(" tunnel");
                }
            }

            // TODO result.push_str(&format!(" -> {} ({})", self.get_target_path_string().unwrap_or_default(), target_str));
            result.push_str(&format!(" -> {} ({})", self.target_path.borrow().as_ref().unwrap(), target_str));
        }

        write!(f, "{result}")
    }
}