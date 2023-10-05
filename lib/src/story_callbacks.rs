use std::{rc::Rc, cell::RefCell};

use crate::{story::Story, value_type::ValueType, story_error::StoryError};

pub trait VariableObserver  {
    fn changed(&mut self, variable_name: &str, value: &ValueType);
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

    pub fn remove_variable_observer(&mut self, observer: &Rc<RefCell<dyn VariableObserver>>, specific_variable_name: Option<&str>) -> Result<(), StoryError>  {
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
}

