use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompilerError {
    InvalidSource(&'static str),
}

impl fmt::Display for CompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSource(message) => write!(f, "{message}"),
        }
    }
}

impl Error for CompilerError {}
