use std::{
    fmt,
    rc::Rc,
};

use crate::{object::{Object, RTObject}, push_pop::PushPopType, pointer::{Pointer, self}, path::Path};


pub struct Divert {
    obj: Object,
    pub external_args: i32,
    pub is_conditional: bool,
    pub is_external: bool,
    pub pushes_to_stack: bool,
    pub stack_push_type: PushPopType,
    pub target_pointer: Pointer,
    pub target_path: Option<Path>,
    pub variable_divert_name: Option<String>,    
}

impl Divert {
    pub fn new(pushes_to_stack: bool, stack_push_type: PushPopType, is_external: bool, external_args: i32, is_conditional: bool, var_divert_name: Option<String>, target_path: Option<&str>) -> Self {
        Divert {
            obj: Object::new(),
            is_conditional,
            pushes_to_stack,
            stack_push_type,
            is_external,
            external_args,
            target_pointer: pointer::NULL.clone(),
            target_path: Self::target_path_string(target_path),
            variable_divert_name: var_divert_name,
        }
    }

    fn target_path_string(value: Option<&str>) -> Option<Path>{
        if let Some(value) = value {
            Some(Path::new_with_components_string(Some(value)))
        } else {
            None
        }
    }

    fn get_target_path_string(&self) -> Option<String> {
        if let Some(target_path) = &self.target_path {
            // TODO Some(compact_path_string(target_path))
            None
        } else {
            None
        }
    }

    pub fn has_variable_target(&self) -> bool {
        self.variable_divert_name.is_some()
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
        } else if self.target_path.is_none() {
            result.push_str("Divert(null)");
        } else {
            let mut sb = String::new();
            let target_str = self.target_path.as_ref().unwrap().to_string();
            // if let Some(target_line_num) = debug_line_number_of_path(self.get_target_path().unwrap()) {
            //     sb.push_str(&format!("line {}", target_line_num));
            // }

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

            result.push_str(&format!(" -> {} ({})", self.get_target_path_string().unwrap_or_default(), sb));
        }

        write!(f, "{result}")
    }
}