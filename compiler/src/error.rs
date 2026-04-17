use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompilerError {
    InvalidSource {
        message: String,
        file: Option<String>,
        line: Option<usize>,
    },
    UnsupportedFeature {
        message: String,
        file: Option<String>,
        line: Option<usize>,
    },
}

impl CompilerError {
    pub fn invalid_source(message: impl Into<String>) -> Self {
        Self::InvalidSource {
            message: message.into(),
            file: None,
            line: None,
        }
    }

    pub fn unsupported_feature(message: impl Into<String>) -> Self {
        Self::UnsupportedFeature {
            message: message.into(),
            file: None,
            line: None,
        }
    }

    /// Attach a 1-based line number to the error, if one is not already set.
    pub fn with_line(self, line: usize) -> Self {
        match self {
            Self::InvalidSource {
                message,
                file,
                line: None,
            } => Self::InvalidSource {
                message,
                file,
                line: Some(line),
            },
            Self::UnsupportedFeature {
                message,
                file,
                line: None,
            } => Self::UnsupportedFeature {
                message,
                file,
                line: Some(line),
            },
            other => other,
        }
    }

    /// Attach a filename to the error, if one is not already set.
    pub fn with_file(self, file: impl Into<String>) -> Self {
        match self {
            Self::InvalidSource {
                message,
                file: None,
                line,
            } => Self::InvalidSource {
                message,
                file: Some(file.into()),
                line,
            },
            Self::UnsupportedFeature {
                message,
                file: None,
                line,
            } => Self::UnsupportedFeature {
                message,
                file: Some(file.into()),
                line,
            },
            other => other,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::InvalidSource { message, .. } | Self::UnsupportedFeature { message, .. } => {
                message
            }
        }
    }
}

impl fmt::Display for CompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (message, file, line) = match self {
            Self::InvalidSource {
                message,
                file,
                line,
            }
            | Self::UnsupportedFeature {
                message,
                file,
                line,
            } => (message, file, line),
        };

        match (file, line) {
            (Some(file), Some(line)) => write!(f, "{file}:{line}: {message}"),
            (Some(file), None) => write!(f, "{file}: {message}"),
            (None, Some(line)) => write!(f, "line {line}: {message}"),
            (None, None) => write!(f, "{message}"),
        }
    }
}

impl Error for CompilerError {}
