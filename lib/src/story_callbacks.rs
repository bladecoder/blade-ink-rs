use std::{rc::Rc, cell::RefCell, collections::HashSet};

use crate::{story::Story, value_type::ValueType, story_error::StoryError, push_pop::PushPopType, pointer::Pointer, container::Container, value::Value, object::RTObject, void::Void, divert::Divert};

pub trait VariableObserver  {
    fn changed(&mut self, variable_name: &str, value: &ValueType);
}

pub trait ExternalFunction  {
    fn call(&mut self, func_name: &str, args: Vec<ValueType>) -> Option<ValueType>;
}

pub(crate) struct ExternalFunctionDef {
    function: Rc<RefCell<dyn ExternalFunction>>,
    lookahead_safe: bool,
}

impl Story {

    pub fn observe_variable(&mut self, variable_name: &str, observer: Rc<RefCell<dyn VariableObserver>>) -> Result<(), StoryError> {
        self.if_async_we_cant("observe a new variable")?;

        if !self.get_state().variables_state.global_variable_exists_with_name(variable_name) {
            return Err(StoryError::BadArgument(
                format!("Cannot observe variable '{variable_name}' because it wasn't declared in the ink story.")));
        }

        match self.variable_observers.get_mut(variable_name) {
            Some(v) => {
                v.push(observer);
            }
            None => {
                    let v: Vec<Rc<RefCell<dyn VariableObserver>>> = vec![observer];
                    self.variable_observers.insert(variable_name.to_string(), v);
                }
        }

        Ok(())
    }

    pub fn remove_variable_observer(&mut self, observer: &Rc<RefCell<dyn VariableObserver>>, specific_variable_name: Option<&str>) -> Result<(), StoryError> {
        self.if_async_we_cant("remove a variable observer")?;

        // Remove observer for this specific variable
        match specific_variable_name {
            Some(specific_variable_name) => {
                if let Some(v) = self.variable_observers.get_mut(specific_variable_name) {
                    let index = v.iter().position(|x| Rc::ptr_eq(x, observer)).unwrap();
                    v.remove(index);
                    
                    if v.is_empty() {
                        self.variable_observers.remove(specific_variable_name);
                    }
                }     
            },
            None => {
                // Remove observer for all variables
                let mut keys_to_remove = Vec::new();
                            
                for (k,v) in self.variable_observers.iter_mut() {
                    let index = v.iter().position(|x| Rc::ptr_eq(x, observer)).unwrap();
                    v.remove(index);

                    if v.is_empty() {
                        keys_to_remove.push(k.to_string());
                    }
                }

                for key_to_remove in keys_to_remove.iter() {
                    self.variable_observers.remove(key_to_remove);
                }          
            }
        }

        Ok(())
    }

    pub(crate) fn notify_variable_changed(&self, variable_name: &str, value: &ValueType) {
        let observers = self.variable_observers.get(variable_name);

        if let Some(observers) = observers {
            for o in observers.iter() {
                o.borrow_mut().changed(variable_name, value);
            }
        }
    }

    pub fn bind_external_function(&mut self, func_name: &str, function: Rc<RefCell<dyn ExternalFunction>>, lookahead_safe: bool) -> Result<(), StoryError> {
        self.if_async_we_cant("bind an external function")?;

        if self.externals.contains_key(func_name) {
             return Err(StoryError::BadArgument(format!("Function '{func_name}' has already been bound.")));
        }

        let external_function_def = ExternalFunctionDef {function, lookahead_safe};

        self.externals.insert(func_name.to_string(), external_function_def);

        Ok(())
    }

    pub(crate) fn call_external_function(
        &mut self,
        func_name: &str,
        number_of_arguments: usize,
    ) -> Result<(), StoryError> {
    
    
        // Should this function break glue? Abort run if we've already seen a newline.
        // Set a bool to tell it to restore the snapshot at the end of this instruction.
        if let Some(func_def) = self.externals.get(func_name) {
            if !func_def.lookahead_safe && self.state_snapshot_at_last_new_line.is_some() {
                self.saw_lookahead_unsafe_function_after_new_line = true;
                return Ok(());
            }
        } else {  
        // Try to use fallback function?
            if self.allow_external_function_fallbacks {
                if let Some(fallback_function_container) = self.knot_container_with_name(func_name) {
                    // Divert direct into fallback function and we're done
                    self.get_state()
                        .get_callstack().borrow_mut()
                        .push(PushPopType::Function, 0, self.get_state().get_output_stream().len() as i32);
                    self.get_state_mut().set_diverted_pointer(Pointer::start_of(fallback_function_container));
                    return Ok(());
                } else {
                    return Err(StoryError::InvalidStoryState(format!(
                        "Trying to call EXTERNAL function '{}' which has not been bound, and fallback ink function could not be found.",
                        func_name
                    )));
                }
            } else {
                return Err(StoryError::InvalidStoryState(format!(
                    "Trying to call EXTERNAL function '{}' which has not been bound (and ink fallbacks disabled).",
                    func_name
                )));
            }
        }
    
        // Pop arguments
        let mut arguments: Vec<ValueType> = Vec::new();
        for _ in 0..number_of_arguments {
            let popped_obj = self.get_state_mut().pop_evaluation_stack();
            let value_obj = popped_obj.into_any().downcast::<Value>();

            if let Ok(value_obj) = value_obj {
                arguments.push(value_obj.value.clone());
            } else {
                return Err(StoryError::InvalidStoryState(format!(
                    "Trying to call EXTERNAL function '{}' with arguments which are not values.",
                    func_name
                )));
            }
        }
    
        // Reverse arguments from the order they were popped,
        // so they're the right way round again.
        arguments.reverse();
    
        // Run the function!
        let func_def = self.externals.get(func_name);
        let func_result = func_def.unwrap().function.borrow_mut().call(func_name,arguments);
    
        // Convert return value (if any) to a type that the ink engine can use
        let return_obj: Rc<dyn RTObject> = match func_result {
            Some(func_result) => {
                Rc::new(Value::new(func_result))
            }
            None => Rc::new(Void::new()),
        };
    
        self.get_state_mut().push_evaluation_stack(return_obj);

        Ok(())
    }

    pub(crate) fn validate_external_bindings(&mut self) -> Result<(), StoryError> {
        let mut missing_externals: HashSet<String> = HashSet::new();

        self.validate_external_bindings_container(&self.get_main_content_container(), &mut missing_externals)?;

        if missing_externals.is_empty() {
            self.has_validated_externals = true;
        } else {
            let join: String = missing_externals.iter().cloned().collect::<Vec<String>>().join(", ");
            let message = format!(
                "ERROR: Missing function binding for external{}: '{}' {}",
                if missing_externals.len() > 1 { "s" } else { "" },
                join,
                if self.allow_external_function_fallbacks {
                    ", and no fallback ink function found."
                } else {
                    " (ink fallbacks disabled)"
                }
            );

            return Err(StoryError::InvalidStoryState(message));
        }

        Ok(())
    }

    fn validate_external_bindings_container(&self, c: &Rc<Container>, missing_externals: &mut std::collections::HashSet<String>) -> Result<(), StoryError> {
        for  inner_content in c.content.iter() {
            let container = inner_content.clone().into_any().downcast::<Container>().ok();

            match &container {
                Some(container) => if !container.has_valid_name(){
                    self.validate_external_bindings_container(container, missing_externals)?;
                },
                None =>  {self.validate_external_bindings_rtobject(inner_content, missing_externals)?;},
            }
            
            if container.is_none() || !container.as_ref().unwrap().has_valid_name() {
                self.validate_external_bindings_rtobject(inner_content, missing_externals)?;
            }
        }

        for inner_key_value in c.named_content.values() {
            self.validate_external_bindings_container(inner_key_value, missing_externals)?;
        }

        Ok(())
    }

    fn validate_external_bindings_rtobject(&self, o: &Rc<dyn RTObject>, missing_externals: &mut std::collections::HashSet<String>) -> Result<(), StoryError> { 
        let divert = o.clone().into_any().downcast::<Divert>().ok();

        if let Some(divert) = divert {
            if divert.is_external {
                let name = divert.get_target_path_string().unwrap();

                if !self.externals.contains_key(&name) {

                    if self.allow_external_function_fallbacks {
                        let fallback_found =
                                self.get_main_content_container().named_content.contains_key(&name);
                        if !fallback_found {
                            missing_externals.insert(name);
                        }
                    } else {
                        missing_externals.insert(name);
                    }
                }
            }
        }

        Ok(())
    }

}