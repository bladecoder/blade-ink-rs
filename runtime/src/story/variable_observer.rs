#[allow(unused_imports)]
use crate::prelude::*;

use crate::compat::{collections::HashSet, error::Error, fmt};

use crate::{story::Story, story_error::StoryError, value_type::ValueType};

/// An error returned by a client-provided variable observer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableObserverError(String);

impl VariableObserverError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for VariableObserverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for VariableObserverError {}

pub type VariableObserverResult = Result<(), VariableObserverError>;

/// Defines a callback invoked when an observed global variable changes.
pub trait VariableObserver {
    fn changed(&mut self, variable_name: &str, value: &ValueType) -> VariableObserverResult;
}

impl<F> VariableObserver for F
where
    F: FnMut(&str, &ValueType) -> VariableObserverResult,
{
    fn changed(&mut self, variable_name: &str, value: &ValueType) -> VariableObserverResult {
        self(variable_name, value)
    }
}

/// Identifies a registered variable observer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VariableObserverHandle(pub(crate) u64);

pub(crate) struct VariableObserverDef {
    observer: Box<dyn VariableObserver>,
}

impl Story {
    /// Observes one global variable and returns a handle that can unsubscribe it.
    pub fn observe_variable<F>(
        &mut self,
        variable_name: &str,
        observer: F,
    ) -> Result<VariableObserverHandle, StoryError>
    where
        F: FnMut(&str, &ValueType) -> VariableObserverResult + 'static,
    {
        self.observe_variables(&[variable_name], observer)
    }

    /// Observes several global variables with one callback and returns one handle.
    ///
    /// Every variable is validated before registration, so an invalid name
    /// leaves no partial subscription behind. Duplicate names are observed once.
    pub fn observe_variables<F>(
        &mut self,
        variable_names: &[&str],
        observer: F,
    ) -> Result<VariableObserverHandle, StoryError>
    where
        F: FnMut(&str, &ValueType) -> VariableObserverResult + 'static,
    {
        self.observe_variables_box(variable_names, Box::new(observer))
    }

    /// Observes one global variable with a [`VariableObserver`] implementation.
    pub fn observe_variable_handler<F>(
        &mut self,
        variable_name: &str,
        observer: F,
    ) -> Result<VariableObserverHandle, StoryError>
    where
        F: VariableObserver + 'static,
    {
        self.observe_variables_handler(&[variable_name], observer)
    }

    /// Observes several global variables with a [`VariableObserver`] implementation.
    pub fn observe_variables_handler<F>(
        &mut self,
        variable_names: &[&str],
        observer: F,
    ) -> Result<VariableObserverHandle, StoryError>
    where
        F: VariableObserver + 'static,
    {
        self.observe_variables_box(variable_names, Box::new(observer))
    }

    fn observe_variables_box(
        &mut self,
        variable_names: &[&str],
        observer: Box<dyn VariableObserver>,
    ) -> Result<VariableObserverHandle, StoryError> {
        self.if_async_we_cant("observe a new variable")?;
        let variable_names = variable_names.iter().copied().collect::<HashSet<_>>();
        for variable_name in &variable_names {
            if !self
                .get_state()
                .variables_state
                .global_variable_exists_with_name(variable_name)
            {
                return Err(StoryError::BadArgument(format!(
                    "Cannot observe variable '{variable_name}' because it wasn't declared in the ink story."
                )));
            }
        }

        let handle = VariableObserverHandle(self.next_variable_observer_id);
        self.next_variable_observer_id = self.next_variable_observer_id.wrapping_add(1);
        self.variable_observer_defs
            .insert(handle, VariableObserverDef { observer });
        for variable_name in variable_names {
            self.variable_observers
                .entry(variable_name.to_owned())
                .or_default()
                .push(handle);
        }
        Ok(handle)
    }

    /// Removes all subscriptions associated with a variable observer handle.
    pub fn remove_variable_observer(
        &mut self,
        handle: VariableObserverHandle,
    ) -> Result<bool, StoryError> {
        self.if_async_we_cant("remove a variable observer")?;
        if self.variable_observer_defs.remove(&handle).is_none() {
            return Ok(false);
        }
        self.variable_observers.retain(|_, handles| {
            handles.retain(|registered| *registered != handle);
            !handles.is_empty()
        });
        Ok(true)
    }

    pub(crate) fn notify_variable_changed(
        &mut self,
        variable_name: &str,
        value: &ValueType,
    ) -> Result<(), StoryError> {
        let handles = self
            .variable_observers
            .get(variable_name)
            .cloned()
            .unwrap_or_default();
        for handle in handles {
            if let Some(observer) = self.variable_observer_defs.get_mut(&handle) {
                observer
                    .observer
                    .changed(variable_name, value)
                    .map_err(|error| StoryError::VariableObserverFailed {
                        variable_name: variable_name.to_owned(),
                        error,
                    })?;
            }
        }
        Ok(())
    }
}
