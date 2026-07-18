use std::{collections::HashSet, error::Error, fmt, rc::Rc};

use crate::{
    container::Container, divert::Divert, object::RTObject, pointer::Pointer,
    push_pop::PushPopType, story::Story, story_error::StoryError, value::Value,
    value_type::ValueType, void::Void,
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

pub(crate) struct ExternalFunctionDef {
    function: Box<dyn ExternalFunction>,
    lookahead_safe: bool,
}

/// # External Functions
/// Methods dealing with external function call handlers that will be called
/// while [`Story`] is processing.
impl Story {
    /// Enables fallback Ink functions for unbound `EXTERNAL` declarations.
    ///
    /// When enabled, an unbound external can divert to an Ink function with the
    /// same name instead of producing an error.
    pub fn set_allow_external_function_fallbacks(&mut self, v: bool) {
        self.allow_external_function_fallbacks = v;
    }

    /// Binds an owned Rust closure to an Ink `EXTERNAL` declaration.
    ///
    /// `lookahead_safe` must be `true` only when repeated speculative calls
    /// have no observable side effects and always produce the same result.
    pub fn bind_external_function<F>(
        &mut self,
        func_name: &str,
        function: F,
        lookahead_safe: bool,
    ) -> Result<(), StoryError>
    where
        F: FnMut(&str, &[ValueType]) -> ExternalFunctionResult + 'static,
    {
        self.bind_external_function_box(func_name, Box::new(function), lookahead_safe)
    }

    /// Binds a type implementing [`ExternalFunction`] to an Ink `EXTERNAL` declaration.
    pub fn bind_external_function_handler<F>(
        &mut self,
        func_name: &str,
        function: F,
        lookahead_safe: bool,
    ) -> Result<(), StoryError>
    where
        F: ExternalFunction + 'static,
    {
        self.bind_external_function_box(func_name, Box::new(function), lookahead_safe)
    }

    fn bind_external_function_box(
        &mut self,
        func_name: &str,
        function: Box<dyn ExternalFunction>,
        lookahead_safe: bool,
    ) -> Result<(), StoryError> {
        self.if_async_we_cant("bind an external function")?;

        if self.externals.contains_key(func_name) {
            return Err(StoryError::BadArgument(format!(
                "Function '{func_name}' has already been bound."
            )));
        }

        self.externals.insert(
            func_name.to_owned(),
            ExternalFunctionDef {
                function,
                lookahead_safe,
            },
        );
        Ok(())
    }

    pub fn unbind_external_function(&mut self, func_name: &str) -> Result<(), StoryError> {
        self.if_async_we_cant("unbind an external function")?;
        if self.externals.remove(func_name).is_none() {
            return Err(StoryError::BadArgument(format!(
                "Function '{func_name}' has not been bound."
            )));
        }
        Ok(())
    }

    pub(crate) fn call_external_function(
        &mut self,
        func_name: &str,
        number_of_arguments: usize,
    ) -> Result<(), StoryError> {
        if let Some(func_def) = self.externals.get(func_name) {
            if func_def.lookahead_safe && self.get_state().in_string_evaluation() {
                self.add_error(&format!("External function {func_name} could not be called because it wasn't marked as lookaheadSafe and the story is in the middle of string generation."), false);
                return Ok(());
            }
            if !func_def.lookahead_safe && self.state_snapshot_at_last_new_line.is_some() {
                self.saw_lookahead_unsafe_function_after_new_line = true;
                return Ok(());
            }
        } else if self.allow_external_function_fallbacks {
            if let Some(fallback_function_container) = self.knot_container_with_name(func_name) {
                self.get_state().get_callstack().borrow_mut().push(
                    PushPopType::Function,
                    0,
                    self.get_state().get_output_stream().len() as i32,
                );
                self.get_state_mut()
                    .set_diverted_pointer(Pointer::start_of(fallback_function_container));
                return Ok(());
            }
            return Err(StoryError::InvalidStoryState(format!(
                "Trying to call EXTERNAL function '{func_name}' which has not been bound, and fallback ink function could not be found."
            )));
        } else {
            return Err(StoryError::InvalidStoryState(format!(
                "Trying to call EXTERNAL function '{func_name}' which has not been bound (and ink fallbacks disabled)."
            )));
        }

        let mut arguments = Vec::with_capacity(number_of_arguments);
        for _ in 0..number_of_arguments {
            let popped_obj = self.get_state_mut().pop_evaluation_stack();
            let value_obj = popped_obj.into_any().downcast::<Value>().map_err(|_| {
                StoryError::InvalidStoryState(format!(
                    "Trying to call EXTERNAL function '{func_name}' with arguments which are not values."
                ))
            })?;
            arguments.push(value_obj.value.clone());
        }
        arguments.reverse();

        let func_result = self
            .externals
            .get_mut(func_name)
            .expect("external function was checked above")
            .function
            .call(func_name, &arguments)
            .map_err(|error| StoryError::ExternalFunctionFailed {
                function_name: func_name.to_owned(),
                error,
            })?;

        let return_obj: Rc<dyn RTObject> = match func_result {
            Some(value) => Rc::new(Value::new_value_type(value)),
            None => Rc::new(Void::new()),
        };
        self.get_state_mut().push_evaluation_stack(return_obj);
        Ok(())
    }

    pub(crate) fn validate_external_bindings(&mut self) -> Result<(), StoryError> {
        let mut missing_externals = HashSet::new();
        self.validate_external_bindings_container(
            &self.get_main_content_container(),
            &mut missing_externals,
        )?;
        if missing_externals.is_empty() {
            self.has_validated_externals = true;
            return Ok(());
        }
        let missing_count = missing_externals.len();
        let join = missing_externals.into_iter().collect::<Vec<_>>().join(", ");
        Err(StoryError::InvalidStoryState(format!(
            "ERROR: Missing function binding for external{}: '{}' {}",
            if missing_count > 1 { "s" } else { "" },
            join,
            if self.allow_external_function_fallbacks {
                ", and no fallback ink function found."
            } else {
                " (ink fallbacks disabled)"
            }
        )))
    }

    fn validate_external_bindings_container(
        &self,
        container: &Rc<Container>,
        missing_externals: &mut HashSet<String>,
    ) -> Result<(), StoryError> {
        for content in &container.content {
            if let Ok(child) = content.clone().into_any().downcast::<Container>() {
                if !child.has_valid_name() {
                    self.validate_external_bindings_container(&child, missing_externals)?;
                }
            } else {
                self.validate_external_bindings_rtobject(content, missing_externals)?;
            }
        }
        for child in container.named_content.values() {
            self.validate_external_bindings_container(child, missing_externals)?;
        }
        Ok(())
    }

    fn validate_external_bindings_rtobject(
        &self,
        object: &Rc<dyn RTObject>,
        missing_externals: &mut HashSet<String>,
    ) -> Result<(), StoryError> {
        let divert = object.clone().into_any().downcast::<Divert>().ok();
        if let Some(divert) = divert
            && divert.is_external
        {
            let name = divert.get_target_path_string().unwrap();
            if !self.externals.contains_key(&name)
                && (!self.allow_external_function_fallbacks
                    || !self
                        .get_main_content_container()
                        .named_content
                        .contains_key(&name))
            {
                missing_externals.insert(name);
            }
        }
        Ok(())
    }
}
