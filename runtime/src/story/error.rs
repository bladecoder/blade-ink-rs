//! Errors that happen at runtime, when running a [`Story`](crate::story::Story).
#[allow(unused_imports)]
use crate::prelude::*;
use core::fmt;

use crate::{
    story::external_functions::ExternalFunctionError,
    story::variable_observer::VariableObserverError,
};

/// Error that represents an error when running a [`Story`](crate::story::Story) at runtime.
/// An error of this type typically means there's
/// a bug in your ink, rather than in the ink engine itself!
#[derive(Debug)]
pub enum StoryError {
    /// Story is in an invalid state.
    InvalidStoryState(String),
    /// JSON for the ink was not valid.
    BadJson(String),
    /// A binary story image was invalid or incompatible.
    BadImage(String),
    /// A method was called with an inappropriate argument.
    BadArgument(String),
    /// A client-provided external function failed.
    ExternalFunctionFailed {
        function_name: String,
        error: ExternalFunctionError,
    },
    /// A client-provided variable observer failed.
    VariableObserverFailed {
        variable_name: String,
        error: VariableObserverError,
    },
}

impl StoryError {
    pub(crate) fn get_message(&self) -> String {
        match self {
            StoryError::InvalidStoryState(msg)
            | StoryError::BadJson(msg)
            | StoryError::BadImage(msg)
            | StoryError::BadArgument(msg) => msg.clone(),
            StoryError::ExternalFunctionFailed { error, .. } => error.to_string(),
            StoryError::VariableObserverFailed { error, .. } => error.to_string(),
        }
    }
}

impl crate::compat::error::Error for StoryError {}

impl crate::compat::convert::From<crate::compat::io::Error> for StoryError {
    fn from(err: crate::compat::io::Error) -> StoryError {
        StoryError::BadJson(err.to_string())
    }
}

impl fmt::Display for StoryError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StoryError::InvalidStoryState(desc) => write!(f, "Invalid story state: {}", desc),
            StoryError::BadJson(desc) => write!(f, "Error parsing JSON: {}", desc),
            StoryError::BadImage(desc) => write!(f, "Error reading story image: {}", desc),
            StoryError::BadArgument(arg) => write!(f, "Bad argument: {}", arg),
            StoryError::ExternalFunctionFailed {
                function_name,
                error,
            } => write!(f, "External function '{function_name}' failed: {error}"),
            StoryError::VariableObserverFailed {
                variable_name,
                error,
            } => write!(f, "Variable observer for '{variable_name}' failed: {error}"),
        }
    }
}

/// Receives runtime errors and warnings while playing a story.
pub trait ErrorHandler {
    fn error(&mut self, message: &str, error_type: ErrorType);
}

/// Severity of an Ink runtime diagnostic.
#[derive(PartialEq, Clone, Copy)]
pub enum ErrorType {
    /// Problem that is not critical, but should be fixed.
    Warning,
    /// Critical error that cannot be recovered from.
    Error,
}
