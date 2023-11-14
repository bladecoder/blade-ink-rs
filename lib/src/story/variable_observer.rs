//! For setting the variable observer function callbacks that will be called
//! while the [`Story`] is processing.
use std::{cell::RefCell, rc::Rc};

use crate::{story::Story, story_error::StoryError, value_type::ValueType};

/// Defines the method that will be called when an observed global variable
/// changes.
pub trait VariableObserver {
    fn changed(&mut self, variable_name: &str, value: &ValueType);
}

/// # Callbacks
/// Methods dealing with callback handlers.
impl Story {
    /// When the specified global variable changes it's value, the observer will
    /// be called to notify it of the change. Note that if the value changes
    /// multiple times within the ink, the observer will only be called
    /// once, at the end of the ink's evaluation. If, during the evaluation,
    /// it changes and then changes back again to its original value, it
    /// will still be called. Note that the observer will also be fired if
    /// the value of the variable is changed externally to the ink, by
    /// directly setting a value in
    /// [`story.set_variable`](Story::set_variable).
    pub fn observe_variable(
        &mut self,
        variable_name: &str,
        observer: Rc<RefCell<dyn VariableObserver>>,
    ) -> Result<(), StoryError> {
        self.if_async_we_cant("observe a new variable")?;

        if !self
            .get_state()
            .variables_state
            .global_variable_exists_with_name(variable_name)
        {
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

    /// Removes a variable observer, to stop getting variable change
    /// notifications. If you pass a specific variable name, it will stop
    /// observing that particular one. If you pass None, then the observer
    /// will be removed from all variables that it's subscribed to.
    pub fn remove_variable_observer(
        &mut self,
        observer: &Rc<RefCell<dyn VariableObserver>>,
        specific_variable_name: Option<&str>,
    ) -> Result<(), StoryError> {
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
            }
            None => {
                // Remove observer for all variables
                let mut keys_to_remove = Vec::new();

                for (k, v) in self.variable_observers.iter_mut() {
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
