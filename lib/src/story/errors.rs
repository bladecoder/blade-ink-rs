use std::{cell::RefCell, rc::Rc};

use crate::story::Story;

/// Defines the method that will be called when an error occurs while executing
/// the story.
pub trait ErrorHandler {
    fn error(&mut self, message: &str, error_type: ErrorType);
}

/// Types of errors an Ink story might throw.
#[derive(PartialEq, Clone, Copy)]
pub enum ErrorType {
    /// Problem that is not critical, but should be fixed.
    Warning,
    /// Critical error that can't be recovered from.
    Error,
}

/// # Errors
/// Methods to check for errors.
impl Story {
    /// Assign the error handler for all runtime errors in ink -- i.e. problems
    /// with the source ink itself that are only discovered when playing
    /// the story.
    /// It's strongly recommended that you assign an error handler to your
    /// story instance, to avoid getting panics for ink errors.
    pub fn set_error_handler(&mut self, err_handler: Rc<RefCell<dyn ErrorHandler>>) {
        self.on_error = Some(err_handler);
    }

    pub(crate) fn add_error(&mut self, message: &str, is_warning: bool) {
        let error_type_str = if is_warning { "WARNING" } else { "ERROR" };

        let m = if !self.get_state().get_current_pointer().is_null() {
            format!(
                "RUNTIME {}: ({}): {}",
                error_type_str,
                self.get_state().get_current_pointer().get_path().unwrap(),
                message
            )
        } else {
            format!("RUNTIME {}: {}", error_type_str, message)
        };

        self.get_state_mut().add_error(m, is_warning);

        if !is_warning {
            self.get_state_mut().force_end();
        }
    }

    pub(crate) fn reset_errors(&mut self) {
        self.get_state_mut().reset_errors();
    }

    /// Whether the `currentErrors` list contains any errors.
    ///
    /// THIS METHOD MAY BE REMOVED IN FUTURE -- you should be setting an
    /// error handler directly using Story.onError.
    pub fn has_error(&self) -> bool {
        self.get_state().has_error()
    }

    /// Any critical errors generated during evaluation of the `Story`.
    pub fn get_current_errors(&self) -> &Vec<String> {
        self.get_state().get_current_errors()
    }

    /// Any warnings generated during evaluation of the `Story`.
    pub fn get_current_warnings(&self) -> &Vec<String> {
        self.get_state().get_current_warnings()
    }
}
