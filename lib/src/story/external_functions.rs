use std::{cell::RefCell, collections::HashSet, rc::Rc};

use crate::{
    container::Container, divert::Divert, object::RTObject, pointer::Pointer,
    push_pop::PushPopType, story::Story, story_error::StoryError, value::Value,
    value_type::ValueType, void::Void,
};

/// Defines the method callback implementing an external function.
pub trait ExternalFunction {
    fn call(&mut self, func_name: &str, args: Vec<ValueType>) -> Option<ValueType>;
}

pub(crate) struct ExternalFunctionDef {
    function: Rc<RefCell<dyn ExternalFunction>>,
    lookahead_safe: bool,
}

/// # External Functions
/// Methods dealing with external function call handlers that will be called
/// while [`Story`] is processing.
impl Story {
    /// An ink file can provide a fallback function for when when an `EXTERNAL`
    /// has been left unbound by the client, in which case the fallback will
    /// be called instead. Useful when testing a story in play-mode, when
    /// it's not possible to write a client-side external function, but when
    /// you don't want it to completely fail to run.
    pub fn set_allow_external_function_fallbacks(&mut self, v: bool) {
        self.allow_external_function_fallbacks = v;
    }

    /// Bind a Rust function to an ink `EXTERNAL` function declaration.
    ///
    /// Arguments:
    /// * `func_name` - The name of the function you're binding the handler to.
    /// * `function` - The handler that will be called whenever Ink runs that
    /// `EXTERNAL` function.
    /// * `lookahead_safe` - The ink engine often evaluates further
    /// than you might expect beyond the current line just in case it sees
    /// glue that will the current line with the next. It's
    /// possible that a function can appear to be called twice,
    /// and earlier than expected. If it's safe for your
    /// function to be called in this way (since the result and side effect
    /// of the function will not change), then you can pass `true`.
    /// If your function might have side effects or return different results
    /// each time it's called, pass `false` to avoid these extra calls,
    /// especially if you want some action to be performed in game code when
    /// this function is called.
    pub fn bind_external_function(
        &mut self,
        func_name: &str,
        function: Rc<RefCell<dyn ExternalFunction>>,
        lookahead_safe: bool,
    ) -> Result<(), StoryError> {
        self.if_async_we_cant("bind an external function")?;

        if self.externals.contains_key(func_name) {
            return Err(StoryError::BadArgument(format!(
                "Function '{func_name}' has already been bound."
            )));
        }

        let external_function_def = ExternalFunctionDef {
            function,
            lookahead_safe,
        };

        self.externals
            .insert(func_name.to_string(), external_function_def);

        Ok(())
    }

    /// Remove the binding for a named EXTERNAL ink function.
    pub fn unbind_external_function(&mut self, func_name: &str) -> Result<(), StoryError> {
        self.if_async_we_cant("unbind an external a function")?;

        if !self.externals.contains_key(func_name) {
            return Err(StoryError::BadArgument(format!(
                "Function '{func_name}' has not been bound."
            )));
        }

        self.externals.remove(func_name);

        Ok(())
    }

    pub(crate) fn call_external_function(
        &mut self,
        func_name: &str,
        number_of_arguments: usize,
    ) -> Result<(), StoryError> {
        // Should this function break glue? Abort run if we've already seen a newline.
        // Set a bool to tell it to restore the snapshot at the end of this instruction.
        if let Some(func_def) = self.externals.get(func_name) {
            if func_def.lookahead_safe && self.get_state().in_string_evaluation() {
                // 16th Jan 2023: Example ink that was failing:
                //
                //      A line above
                //      ~ temp text = "{theFunc()}"
                //      {text}
                //
                //      === function theFunc()
                //          { external():
                //              Boom
                //          }
                //
                //      EXTERNAL external()
                //
                // What was happening: The external() call would exit out early due to
                // _stateSnapshotAtLastNewline having a value, leaving the evaluation stack
                // without a return value on it. When the if-statement tried to pop a value,
                // the evaluation stack would be empty, and there would be an exception.
                //
                // The snapshot rewinding code is only designed to work when outside of
                // string generation code (there's a check for that in the snapshot rewinding code),
                // hence these things are incompatible, you can't have unsafe functions that
                // cause snapshot rewinding in the middle of string generation.
                //
                self.add_error(&format!("External function {} could not be called because 1) it wasn't marked as lookaheadSafe when BindExternalFunction was called and 2) the story is in the middle of string generation, either because choice text is being generated, or because you have ink like \"hello {{func()}}\". You can work around this by generating the result of your function into a temporary variable before the string or choice gets generated: ~ temp x = {}()", func_name, func_name), false);

                return Ok(());
            }

            if !func_def.lookahead_safe && self.state_snapshot_at_last_new_line.is_some() {
                self.saw_lookahead_unsafe_function_after_new_line = true;
                return Ok(());
            }
        } else {
            // Try to use fallback function?
            if self.allow_external_function_fallbacks {
                if let Some(fallback_function_container) = self.knot_container_with_name(func_name)
                {
                    // Divert direct into fallback function and we're done
                    self.get_state().get_callstack().borrow_mut().push(
                        PushPopType::Function,
                        0,
                        self.get_state().get_output_stream().len() as i32,
                    );
                    self.get_state_mut()
                        .set_diverted_pointer(Pointer::start_of(fallback_function_container));
                    return Ok(());
                } else {
                    return Err(StoryError::InvalidStoryState(format!(
                        "Trying to call EXTERNAL function '{}' which has not been bound, and fallback ink function could not be found.",
                        func_name
                    )));
                }
            } else {
                return Err(StoryError::InvalidStoryState(format!(
                    "Trying to call EXTERNAL function '{}' which has not been bound (and ink fallbacks disabled).",
                    func_name
                )));
            }
        }

        // Pop arguments
        let mut arguments: Vec<ValueType> = Vec::new();
        for _ in 0..number_of_arguments {
            let popped_obj = self.get_state_mut().pop_evaluation_stack();
            let value_obj = popped_obj.into_any().downcast::<Value>();

            if let Ok(value_obj) = value_obj {
                arguments.push(value_obj.value.clone());
            } else {
                return Err(StoryError::InvalidStoryState(format!(
                    "Trying to call EXTERNAL function '{}' with arguments which are not values.",
                    func_name
                )));
            }
        }

        // Reverse arguments from the order they were popped,
        // so they're the right way round again.
        arguments.reverse();

        // Run the function!
        let func_def = self.externals.get(func_name);
        let func_result = func_def
            .unwrap()
            .function
            .borrow_mut()
            .call(func_name, arguments);

        // Convert return value (if any) to a type that the ink engine can use
        let return_obj: Rc<dyn RTObject> = match func_result {
            Some(func_result) => Rc::new(Value::new(func_result)),
            None => Rc::new(Void::new()),
        };

        self.get_state_mut().push_evaluation_stack(return_obj);

        Ok(())
    }

    pub(crate) fn validate_external_bindings(&mut self) -> Result<(), StoryError> {
        let mut missing_externals: HashSet<String> = HashSet::new();

        self.validate_external_bindings_container(
            &self.get_main_content_container(),
            &mut missing_externals,
        )?;

        if missing_externals.is_empty() {
            self.has_validated_externals = true;
        } else {
            let join: String = missing_externals
                .iter()
                .cloned()
                .collect::<Vec<String>>()
                .join(", ");
            let message = format!(
                "ERROR: Missing function binding for external{}: '{}' {}",
                if missing_externals.len() > 1 { "s" } else { "" },
                join,
                if self.allow_external_function_fallbacks {
                    ", and no fallback ink function found."
                } else {
                    " (ink fallbacks disabled)"
                }
            );

            return Err(StoryError::InvalidStoryState(message));
        }

        Ok(())
    }

    fn validate_external_bindings_container(
        &self,
        c: &Rc<Container>,
        missing_externals: &mut std::collections::HashSet<String>,
    ) -> Result<(), StoryError> {
        for inner_content in c.content.iter() {
            let container = inner_content
                .clone()
                .into_any()
                .downcast::<Container>()
                .ok();

            match &container {
                Some(container) => {
                    if !container.has_valid_name() {
                        self.validate_external_bindings_container(container, missing_externals)?;
                    }
                }
                None => {
                    self.validate_external_bindings_rtobject(inner_content, missing_externals)?;
                }
            }

            if container.is_none() || !container.as_ref().unwrap().has_valid_name() {
                self.validate_external_bindings_rtobject(inner_content, missing_externals)?;
            }
        }

        for inner_key_value in c.named_content.values() {
            self.validate_external_bindings_container(inner_key_value, missing_externals)?;
        }

        Ok(())
    }

    fn validate_external_bindings_rtobject(
        &self,
        o: &Rc<dyn RTObject>,
        missing_externals: &mut std::collections::HashSet<String>,
    ) -> Result<(), StoryError> {
        let divert = o.clone().into_any().downcast::<Divert>().ok();

        if let Some(divert) = divert {
            if divert.is_external {
                let name = divert.get_target_path_string().unwrap();

                if !self.externals.contains_key(&name) {
                    if self.allow_external_function_fallbacks {
                        let fallback_found = self
                            .get_main_content_container()
                            .named_content
                            .contains_key(&name);
                        if !fallback_found {
                            missing_externals.insert(name);
                        }
                    } else {
                        missing_externals.insert(name);
                    }
                }
            }
        }

        Ok(())
    }
}
