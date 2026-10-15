#[allow(unused_imports)]
use crate::prelude::*;

use crate::{
    compat::{error::Error, fmt},
    value_type::ValueType,
};

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
