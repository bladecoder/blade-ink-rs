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
