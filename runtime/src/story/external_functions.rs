#[allow(unused_imports)]
use crate::prelude::*;

use crate::{
    compat::{error::Error, fmt},
    value_type::ValueType,
};

/// An error returned by a client-provided external function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalFunctionError(String);

impl ExternalFunctionError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for ExternalFunctionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for ExternalFunctionError {}

/// The result of an external function call. `None` represents Ink's void value.
pub type ExternalFunctionResult = Result<Option<ValueType>, ExternalFunctionError>;

/// Defines a callback implementing an external Ink function.
pub trait ExternalFunction {
    fn call(&mut self, func_name: &str, args: &[ValueType]) -> ExternalFunctionResult;
}

impl<F> ExternalFunction for F
where
    F: FnMut(&str, &[ValueType]) -> ExternalFunctionResult,
{
    fn call(&mut self, func_name: &str, args: &[ValueType]) -> ExternalFunctionResult {
        self(func_name, args)
    }
}
