//! Errors that happen at runtime, when running a [`Story`](crate::story::Story).
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
            | StoryError::BadArgument(msg) => msg.clone(),
            StoryError::ExternalFunctionFailed { error, .. } => error.to_string(),
            StoryError::VariableObserverFailed { error, .. } => error.to_string(),
        }
    }
}

impl std::error::Error for StoryError {}

impl std::convert::From<std::io::Error> for StoryError {
    fn from(err: std::io::Error) -> StoryError {
        StoryError::BadJson(err.to_string())
    }
}

impl fmt::Display for StoryError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StoryError::InvalidStoryState(desc) => write!(f, "Invalid story state: {}", desc),
            StoryError::BadJson(desc) => write!(f, "Error parsing JSON: {}", desc),
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
