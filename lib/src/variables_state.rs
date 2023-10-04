use std::{collections::{HashMap, HashSet}, rc::Rc, cell::RefCell};

use serde_json::Map;

use crate::{callstack::CallStack, state_patch::StatePatch, variable_assigment::VariableAssignment, value::Value, list_definitions_origin::ListDefinitionsOrigin, value_type::{VariablePointerValue, ValueType}, json_write, json_read, story_error::StoryError};


#[derive(Clone)]
pub struct VariablesState {
    pub global_variables: HashMap<String, Rc<Value>>,
    pub default_global_variables: HashMap<String, Rc<Value>>,
    pub batch_observing_variable_changes: bool,
    pub callstack: Rc<RefCell<CallStack>>,
    pub changed_variables_for_batch_obs: Option<HashSet<String>>,
    pub variable_changed_event: Option<fn(variable_name: &str, newValue: &Value)>,
    list_defs_origin: Rc<ListDefinitionsOrigin>,
    pub patch: Option<StatePatch>,
}

impl VariablesState {
    pub fn new(callstack: Rc<RefCell<CallStack>>, list_defs_origin: Rc<ListDefinitionsOrigin>) -> VariablesState {
        VariablesState {
            global_variables: HashMap::new(),
            default_global_variables: HashMap::new(),
            batch_observing_variable_changes: false,
            callstack,
            changed_variables_for_batch_obs: None,
            variable_changed_event: None,
            patch: None,
            list_defs_origin,
        }
    }

    pub fn set_batch_observing_variable_changes(&mut self, value: bool) {
        self.batch_observing_variable_changes = value;

        if value {
            self.changed_variables_for_batch_obs = Some(HashSet::new());
        } else {
            // Finished observing variables in a batch - now send
            // notifications for changed variables all in one go.
            if self.changed_variables_for_batch_obs.is_some() {
                for variable_name in self.changed_variables_for_batch_obs.as_ref().unwrap() {
                    let current_value = self.global_variables.get(variable_name).unwrap();

                    (self.variable_changed_event.as_ref().unwrap())(variable_name, current_value.as_ref());
                }
            }

            self.changed_variables_for_batch_obs = None;
        }    
    }

    pub fn snapshot_default_globals(&mut self) {
        for (k,v) in self.global_variables.iter() {
            self.default_global_variables.insert(k.clone(), v.clone());
        }
    }

    pub fn apply_patch(&mut self) {
        for (name, value) in self.patch.as_ref().unwrap().globals.iter() {
            self.global_variables.insert(name.clone(), value.clone());
        }
    
        if let Some(changed_variables) = &mut self.changed_variables_for_batch_obs {
            for name in self.patch.as_ref().unwrap().changed_variables.iter() {
                changed_variables.insert(name.clone());
            }
        }
    
        self.patch = None;
    }

    pub fn assign (
        &mut self,
        var_ass: &VariableAssignment,
        value: Rc<Value>,
    ) {
        let mut name = var_ass.variable_name.to_string();
        let mut context_index = -1;
        let mut set_global = false;
    
        // Are we assigning to a global variable?
        if var_ass.is_new_declaration {
            set_global = var_ass.is_global;
        } else {
            set_global = self.global_variable_exists_with_name(&name);
        }
        
        let mut value = value;
        // Constructing new variable pointer reference
        if var_ass.is_new_declaration {
            if let Some(var_pointer) = Value::get_variable_pointer_value(value.as_ref()){
                value =
                    self.resolve_variable_pointer(var_pointer);
            }
        } else {
            // Assign to an existing variable pointer
            // Then assign to the variable that the pointer is pointing to by name.
            // De-reference variable reference to point to
            loop {
                let existing_pointer = self.get_raw_variable_with_name(&name, context_index);

                match existing_pointer {
                    Some(existing_pointer) => match Value::get_variable_pointer_value(existing_pointer.as_ref()) {
                        Some(pv) => {
                            name = pv.variable_name.to_string();
                            context_index =pv.context_index;
                            set_global = context_index == 0;
                        },
                        None => break,
                    },
                    None => break,
                }
            }
        }
    
        if set_global {
            self.set_global(&name, value);
        } else {
            self.callstack.borrow_mut().set_temporary_variable(name, value, var_ass.is_new_declaration, context_index);
        }
    }

    fn global_variable_exists_with_name(&self, name: &str) -> bool {
        self.global_variables.contains_key(name)
            || self
                .default_global_variables.contains_key(name)
    }

    // Given a variable pointer with just the name of the target known, resolve
    // to a variable
    // pointer that more specifically points to the exact instance: whether it's
    // global,
    // or the exact position of a temporary on the callstack.
    fn resolve_variable_pointer(&self, var_pointer: &VariablePointerValue) -> Rc<Value> {
        let mut context_index = var_pointer.context_index;
        if context_index == -1 {
            context_index = self.get_context_index_of_variable_named(&var_pointer.variable_name);
        }
    
        let value_of_variable_pointed_to = self.get_raw_variable_with_name(&var_pointer.variable_name, context_index);
        // Extra layer of indirection:
        // When accessing a pointer to a pointer (e.g. when calling nested or
        // recursive functions that take a variable references, ensure we don't
        // create
        // a chain of indirection by just returning the final target.
        if let Some(value_of_variable_pointed_to) = value_of_variable_pointed_to {
            if let Some(double_redirection_pointer) = Value::get_variable_pointer_value(value_of_variable_pointed_to.as_ref()) {
                return value_of_variable_pointed_to;
            }
        }
            
        Rc::new(Value::new_variable_pointer(&var_pointer.variable_name, context_index))
    }

    pub fn set(&mut self, variable_name: &str, value_type: ValueType) -> Result<(), StoryError> {

        if !self.default_global_variables.contains_key(variable_name) {
            return Err(StoryError::BadArgument(format!("Cannot assign to a variable {} that hasn't been declared in the story", variable_name)));
        }

        let val = Value::from_value_type(value_type);

        self.set_global(variable_name, Rc::new(val));

        Ok(())
    }

    pub fn get(&self, variable_name: &str) -> Option<ValueType> {

        if self.patch.is_some() {
            if let Some(var) = self.patch.as_ref().unwrap().get_global(variable_name) {
                return Some(var.value.clone());
            }
        }

        // Search main dictionary first.
        // If it's not found, it might be because the story content has changed,
        // and the original default value hasn't be instantiated.
        // Should really warn somehow, but it's difficult to see how...!
        if let Some(var_contents) = self.global_variables.get(variable_name) {
            return Some(var_contents.value.clone());
        } else if let Some(var_contents) = self.default_global_variables.get(variable_name) {
            return Some(var_contents.value.clone());
        }

        None

    }

    // Make copy of the variable pointer so we're not using the value direct
    // from
    // the runtime. Temporary must be local to the current scope.
    // 0 if named variable is global
    // 1+ if named variable is a temporary in a particular call stack element
    fn get_context_index_of_variable_named(&self, var_name: &str) -> i32 {
        if self.global_variable_exists_with_name(var_name) {
            return 0;
        }

        return self.callstack.borrow().get_current_element_index();
    }

    fn get_raw_variable_with_name(&self, name: &str, context_index: i32) -> Option<Rc<Value>> {
        // 0 context = global
        if context_index == 0 || context_index == -1 {
            if let Some(patch) = &self.patch {
                if let Some(global) = patch.get_global(name) {
                    return Some(global);
                }
            }

            if let Some(global) = self.global_variables.get(name) {
                return Some(global.clone());
            }

            // Getting variables can actually happen during globals set up since you can do
            // VAR x = A_LIST_ITEM
            // So _default_global_variables may be None.
            // We need to do this check though in case a new global is added, so we need to
            // revert to the default globals dictionary since an initial value hasn't yet
            // been set.

            if let Some(default_global) = self.default_global_variables.get(name) {
                return Some(default_global.clone());
            }

            if let Some(list_item_value) = self.list_defs_origin.find_single_item_list_with_name(name) {
                return Some(list_item_value.clone());
            }
        }

        // Temporary
        let var_value = self.callstack.borrow().get_temporary_variable_with_name(name, context_index);

        var_value
    }

    fn set_global(&mut self, name: &str, value: Rc<Value>) {
        let mut old_value: Option<Rc<Value>> = None;

        if let Some(patch) = &self.patch {
            old_value = patch.get_global(name);
        }

        if old_value.is_none() {
            old_value = self.global_variables.get(name).cloned();
        }

        if let Some(old_value) = &old_value {
            Value::retain_list_origins_for_assignment(old_value.as_ref(), value.as_ref());
        }

        if let Some(patch) = &mut self.patch {
            patch.set_global(name, value.clone());
        } else {
            self.global_variables.insert(name.to_string(), value.clone());
        }

        if let Some(variable_changed_event) = &self.variable_changed_event {
            if !Rc::ptr_eq(old_value.as_ref().unwrap(), &value) {
                if self.batch_observing_variable_changes {
                    if let Some(patch) = &mut self.patch {
                        patch.add_changed_variable(name);
                    } else if let Some(changed_variables) = &mut self.changed_variables_for_batch_obs {
                        changed_variables.insert(name.to_string());
                    }
                } else {
                    variable_changed_event(name, value.as_ref());
                }
            }
        }
    }

    pub fn get_variable_with_name(&self, name: &str, context_index: i32) -> Option<Rc<Value>> {
        let var_value = self.get_raw_variable_with_name(name, context_index);
        // Get value from pointer?
        if let Some(vv) = var_value.clone() {
            if let Some(var_pointer) = Value::get_variable_pointer_value(vv.as_ref()) {
                return self.value_at_variable_pointer(var_pointer);
            }
        }

        var_value
    }

    fn value_at_variable_pointer(&self, pointer: &VariablePointerValue) -> Option<Rc<Value>> {
        self.get_variable_with_name(&pointer.variable_name, pointer.context_index)
    }

    pub fn set_callstack(&mut self, callstack: Rc<RefCell<CallStack>>) {
        self.callstack = callstack;
    } 

    pub(crate) fn write_json(&self) -> Result<serde_json::Value, StoryError> {
        let mut jobj: Map<String, serde_json::Value> = Map::new();

        for (name, val) in self.global_variables.iter() {

            // Don't write out values that are the same as the default global values
            let default_val = self.default_global_variables.get(name);
            if let Some(default_val) = default_val {
                if self.val_equal(val, default_val) {continue;}
            }

            jobj.insert(name.clone(), json_write::write_rtobject(val.clone())?);
        }

        Ok(serde_json::Value::Object(jobj))
    }

    fn val_equal(&self, val: &Value, default_val: &Value) -> bool {
        match &val.value {
            ValueType::Bool(val) => match default_val.value {
                ValueType::Bool(default_val) => *val == default_val,
                _ => false,
            },
            ValueType::Int(val) => match default_val.value {
                ValueType::Int(default_val) => *val == default_val,
                _ => false,
            },
            ValueType::Float(val) => match default_val.value {
                ValueType::Float(default_val) => *val == default_val,
                _ => false,
            },
            ValueType::List(val) => match &default_val.value {
                ValueType::List(default_val) => *val == *default_val,
                _ => false,
            },
            ValueType::String(val) => match &default_val.value {
                ValueType::String(default_val) => val.string.eq(&default_val.string),
                _ =>  false,
            },
            ValueType::DivertTarget(val) => match &default_val.value {
                ValueType::DivertTarget(default_val) => *val == *default_val,
                _ => false,
            },
            ValueType::VariablePointer(val) => match &default_val.value {
                ValueType::VariablePointer(default_val) => *val == *default_val,
                _ => false,
            },
        }
    }

    pub(crate) fn load_json(&mut self, jobj: &Map<String, serde_json::Value>) ->  Result<(), StoryError> {
        self.global_variables.clear();

        for (k, v) in self.default_global_variables.iter() {
            let loaded_token = jobj.get(k);

            if let Some(loaded_token) = loaded_token {
                self.global_variables.insert(k.to_string(), json_read::jtoken_to_runtime_object(loaded_token, None)?.into_any().downcast::<Value>().unwrap());
            } else {
                self.global_variables.insert(k.clone(), v.clone());
            }
        }

        Ok(())   
    }

}
