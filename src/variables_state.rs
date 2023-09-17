use std::{collections::{HashMap, HashSet}, rc::Rc, cell::RefCell};

use crate::{object::RTObject, callstack::CallStack, state_patch::StatePatch};

pub(crate) struct VariablesState {
    pub global_variables: HashMap<String, Rc<dyn RTObject>>,
    pub default_global_variables: Option<HashMap<String, Rc<dyn RTObject>>>,
    pub batch_observing_variable_changes: bool,
    pub callstack: Rc<RefCell<CallStack>>,
    pub changed_variables_for_batch_obs: Option<HashSet<String>>,
    pub variable_changed_event: Option<fn(variable_name: &str, newValue: &dyn RTObject)>,
    //TODO listDefsOrigin: ListDefinitionsOrigin
    pub patch: Option<StatePatch>,
}

impl VariablesState {
    pub(crate) fn new(callstack: Rc<RefCell<CallStack>>) -> VariablesState {
        VariablesState {
            global_variables: HashMap::new(),
            default_global_variables: None,
            batch_observing_variable_changes: false,
            callstack: callstack,
            changed_variables_for_batch_obs: None,
            variable_changed_event: None,
            patch: None,
        }
    }

    pub(crate) fn set_batch_observing_variable_changes(&mut self, value: bool) {
        self.batch_observing_variable_changes = value;

        if value {
            self.changed_variables_for_batch_obs = Some(HashSet::new());
        } else {
            // Finished observing variables in a batch - now send
            // notifications for changed variables all in one go.
            if self.changed_variables_for_batch_obs.is_some() {
                for variableName in self.changed_variables_for_batch_obs.as_ref().unwrap() {
                    let current_value = self.global_variables.get(variableName).unwrap();

                    (self.variable_changed_event.as_ref().unwrap())(variableName, current_value.as_ref());
                }
            }

            self.changed_variables_for_batch_obs = None;
        }    
    }

    pub(crate) fn snapshot_default_globals(&mut self) {
        self.default_global_variables = Some(self.global_variables.clone());
    }

    pub(crate) fn apply_patch(&self) {
        todo!()
    }
}
