use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompilerError {
    Unimplemented,
}

impl fmt::Display for CompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unimplemented => {
                write!(f, "ink compiler is not implemented yet")
            }
        }
    }
}

impl Error for CompilerError {}
