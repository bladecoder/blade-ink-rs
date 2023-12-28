use crate::{
    choice::Choice,
    choice_point::ChoicePoint,
    container::Container,
    control_command::{CommandType, ControlCommand},
    object::RTObject,
    pointer::{self, Pointer},
    push_pop::PushPopType,
    story::{errors::ErrorType, OutputStateChange, Story},
    story_error::StoryError,
    value::Value,
    void::Void,
};
use std::{self, rc::Rc};

/// # Story Progress
/// Methods to move the story forwards.
impl Story {
    /// `true` if the story is not waiting for user input from
    /// [`choose_choice_index`](Story::choose_choice_index).
    pub fn can_continue(&self) -> bool {
        self.get_state().can_continue()
    }

    /// Tries to continue pulling text from the story.
    pub fn cont(&mut self) -> Result<String, StoryError> {
        self.continue_async(0.0)?;
        self.get_current_text()
    }

    /// Continues the story until a choice or error is reached.
    /// If a choice is reached, returns all text produced along the way.
    pub fn continue_maximally(&mut self) -> Result<String, StoryError> {
        self.if_async_we_cant("continue_maximally")?;

        let mut sb = String::new();

        while self.can_continue() {
            sb.push_str(&self.cont()?);
        }

        Ok(sb)
    }

    /// Continues running the story code for the specified number of
    /// milliseconds.
    pub fn continue_async(&mut self, millisecs_limit_async: f32) -> Result<(), StoryError> {
        if !self.has_validated_externals {
            self.validate_external_bindings()?;
        }

        self.continue_internal(millisecs_limit_async)
    }

    pub(crate) fn if_async_we_cant(&self, activity_str: &str) -> Result<(), StoryError> {
        if self.async_continue_active {
            return Err(StoryError::InvalidStoryState(format!("Can't {}. Story is in the middle of a continue_async(). Make more continue_async() calls or a single cont() call beforehand.", activity_str)));
        }

        Ok(())
    }

    pub(crate) fn continue_internal(
        &mut self,
        millisecs_limit_async: f32,
    ) -> Result<(), StoryError> {
        let is_async_time_limited = millisecs_limit_async > 0.0;

        self.recursive_continue_count += 1;

        // Doing either:
        // - full run through non-async (so not active and don't want to be)
        // - Starting async run-through
        if !self.async_continue_active {
            self.async_continue_active = is_async_time_limited;
            if !self.can_continue() {
                return Err(StoryError::InvalidStoryState(
                    "Can't continue - should check can_continue before calling Continue".to_owned(),
                ));
            }

            self.get_state_mut().set_did_safe_exit(false);

            self.get_state_mut().reset_output(None);

            // It's possible for ink to call game to call ink to call game etc
            // In this case, we only want to batch observe variable changes
            // for the outermost call.
            if self.recursive_continue_count == 1 {
                self.state
                    .variables_state
                    .start_batch_observing_variable_changes();
            }
        }

        // Start timing (only when necessary)
        let duration_stopwatch = match self.async_continue_active {
            true => Some(instant::Instant::now()),
            false => None,
        };

        let mut output_stream_ends_in_newline = false;
        self.saw_lookahead_unsafe_function_after_new_line = false;

        loop {
            match self.continue_single_step() {
                Ok(r) => output_stream_ends_in_newline = r,
                Err(e) => {
                    self.add_error(e.get_message(), false);
                    break;
                }
            }

            if output_stream_ends_in_newline {
                break;
            }

            // Run out of async time?
            if self.async_continue_active
                && duration_stopwatch.as_ref().unwrap().elapsed().as_millis() as f32
                    > millisecs_limit_async
            {
                break;
            }

            if !self.can_continue() {
                break;
            }
        }

        // 4 outcomes:
        // - got newline (so finished this line of text)
        // - can't continue (e.g. choices or ending)
        // - ran out of time during evaluation
        // - error
        //
        // Successfully finished evaluation in time (or in error)
        if output_stream_ends_in_newline || !self.can_continue() {
            // Need to rewind, due to evaluating further than we should?
            if self.state_snapshot_at_last_new_line.is_some() {
                self.restore_state_snapshot();
            }

            // Finished a section of content / reached a choice point?
            if !self.can_continue() {
                if self.state.get_callstack().borrow().can_pop_thread() {
                    self.add_error("Thread available to pop, threads should always be flat by the end of evaluation?", false);
                }

                if self.state.get_generated_choices().is_empty()
                    && !self.get_state().is_did_safe_exit()
                    && self.temporary_evaluation_container.is_none()
                {
                    if self
                        .state
                        .get_callstack()
                        .borrow()
                        .can_pop_type(Some(PushPopType::Tunnel))
                    {
                        self.add_error("unexpectedly reached end of content. Do you need a '->->' to return from a tunnel?", false);
                    } else if self
                        .state
                        .get_callstack()
                        .borrow()
                        .can_pop_type(Some(PushPopType::Function))
                    {
                        self.add_error(
                            "unexpectedly reached end of content. Do you need a '~ return'?",
                            false,
                        );
                    } else if !self.get_state().get_callstack().borrow().can_pop() {
                        self.add_error(
                            "ran out of content. Do you need a '-> DONE' or '-> END'?",
                            false,
                        );
                    } else {
                        self.add_error("unexpectedly reached end of content for unknown reason. Please debug compiler!", false);
                    }
                }
            }
            self.get_state_mut().set_did_safe_exit(false);
            self.saw_lookahead_unsafe_function_after_new_line = false;

            if self.recursive_continue_count == 1 {
                let changed = self
                    .state
                    .variables_state
                    .stop_batch_observing_variable_changes();

                for (variable_name, value) in changed {
                    self.notify_variable_changed(&variable_name, &value);
                }
            }

            self.async_continue_active = false;
        }

        self.recursive_continue_count -= 1;

        // Report any errors that occured during evaluation.
        // This may either have been StoryExceptions that were thrown
        // and caught during evaluation, or directly added with AddError.
        if self.get_state().has_error() || self.get_state().has_warning() {
            match &self.on_error {
                Some(on_err) => {
                    if self.get_state().has_error() {
                        for err in self.get_state().get_current_errors() {
                            on_err.borrow_mut().error(err, ErrorType::Error);
                        }
                    }

                    if self.get_state().has_warning() {
                        for err in self.get_state().get_current_warnings() {
                            on_err.borrow_mut().error(err, ErrorType::Warning);
                        }
                    }

                    self.reset_errors();
                }
                // Throw an exception since there's no error handler
                None => {
                    let mut sb = String::new();
                    sb.push_str("Ink had ");

                    if self.get_state().has_error() {
                        sb.push_str(&self.get_state().get_current_errors().len().to_string());

                        if self.get_state().get_current_errors().len() == 1 {
                            sb.push_str(" error");
                        } else {
                            sb.push_str(" errors");
                        }

                        if self.get_state().has_warning() {
                            sb.push_str(" and ");
                        }
                    }

                    if self.get_state().has_warning() {
                        sb.push_str(
                            self.get_state()
                                .get_current_warnings()
                                .len()
                                .to_string()
                                .as_str(),
                        );
                        if self.get_state().get_current_errors().len() == 1 {
                            sb.push_str(" warning");
                        } else {
                            sb.push_str(" warnings");
                        }
                    }

                    sb.push_str(". It is strongly suggested that you assign an error handler to story.onError. The first issue was: ");

                    if self.get_state().has_error() {
                        sb.push_str(self.get_state().get_current_errors()[0].as_str());
                    } else {
                        sb.push_str(
                            self.get_state().get_current_warnings()[0]
                                .to_string()
                                .as_str(),
                        );
                    }

                    return Err(StoryError::InvalidStoryState(sb));
                }
            }
        }

        Ok(())
    }

    pub(crate) fn continue_single_step(&mut self) -> Result<bool, StoryError> {
        // Run main step function (walks through content)
        self.step()?;

        // Run out of content and we have a default invisible choice that we can follow?
        if !self.can_continue()
            && !self
                .get_state()
                .get_callstack()
                .borrow()
                .element_is_evaluate_from_game()
        {
            self.try_follow_default_invisible_choice()?;
        }

        // Don't save/rewind during string evaluation, which is e.g. used for choices
        if !self.get_state().in_string_evaluation() {
            // We previously found a newline, but were we just double checking that
            // it wouldn't immediately be removed by glue?
            if let Some(state_snapshot_at_last_new_line) =
                self.state_snapshot_at_last_new_line.as_mut()
            {
                // Has proper text or a tag been added? Then we know that the newline
                // that was previously added is definitely the end of the line.
                let change = Story::calculate_newline_output_state_change(
                    &state_snapshot_at_last_new_line.get_current_text(),
                    &self.state.get_current_text(),
                    state_snapshot_at_last_new_line.get_current_tags().len() as i32,
                    self.state.get_current_tags().len() as i32,
                );

                // The last time we saw a newline, it was definitely the end of the line, so we
                // want to rewind to that point.
                if change == OutputStateChange::ExtendedBeyondNewline
                    || self.saw_lookahead_unsafe_function_after_new_line
                {
                    self.restore_state_snapshot();

                    // Hit a newline for sure, we're done
                    return Ok(true);
                }
                // Newline that previously existed is no longer valid - e.g.
                // glue was encounted that caused it to be removed.
                else if change == OutputStateChange::NewlineRemoved {
                    self.state_snapshot_at_last_new_line = None;
                    self.discard_snapshot();
                }
            }

            // Current content ends in a newline - approaching end of our evaluation
            if self.get_state().output_stream_ends_in_newline() {
                // If we can continue evaluation for a bit:
                // Create a snapshot in case we need to rewind.
                // We're going to continue stepping in case we see glue or some
                // non-text content such as choices.
                if self.can_continue() {
                    // Don't bother to record the state beyond the current newline.
                    // e.g.:
                    // Hello world\n // record state at the end of here
                    // ~ complexCalculation() // don't actually need this unless it generates
                    // text
                    if self.state_snapshot_at_last_new_line.is_none() {
                        self.state_snapshot();
                    }
                }
                // Can't continue, so we're about to exit - make sure we
                // don't have an old state hanging around.
                else {
                    self.discard_snapshot();
                }
            }
        }

        Ok(false)
    }

    pub(crate) fn step(&mut self) -> Result<(), StoryError> {
        let mut should_add_to_stream = true;

        // Get current content
        let mut pointer = self.get_state().get_current_pointer().clone();

        if pointer.is_null() {
            return Ok(());
        }

        // Step directly to the first element of content in a container (if
        // necessary)
        let r = pointer.resolve();

        let mut container_to_enter = match r {
            Some(o) => match o.into_any().downcast::<Container>() {
                Ok(c) => Some(c),
                Err(_) => None,
            },
            None => None,
        };

        while let Some(cte) = container_to_enter.as_ref() {
            // Mark container as being entered
            self.visit_container(cte, true);

            // No content? the most we can do is step past it
            if cte.content.is_empty() {
                break;
            }

            pointer = Pointer::start_of(cte.clone());

            let r = pointer.resolve();

            container_to_enter = match r {
                Some(o) => match o.into_any().downcast::<Container>() {
                    Ok(c) => Some(c),
                    Err(_) => None,
                },
                None => None,
            };
        }

        self.get_state_mut().set_current_pointer(pointer.clone());

        // Is the current content Object:
        // - Normal content
        // - Or a logic/flow statement - if so, do it
        // Stop flow if we hit a stack pop when we're unable to pop (e.g.
        // return/done statement in knot
        // that was diverted to rather than called as a function)
        let mut current_content_obj = pointer.resolve();

        let is_logic_or_flow_control = self.perform_logic_and_flow_control(&current_content_obj)?;

        // Has flow been forced to end by flow control above?
        if self.get_state().get_current_pointer().is_null() {
            return Ok(());
        }

        if is_logic_or_flow_control {
            should_add_to_stream = false;
        }

        // Choice with condition?
        if let Some(cco) = &current_content_obj {
            // If the container has no content, then it will be
            // the "content" itself, but we skip over it.
            if cco.as_any().is::<Container>() {
                should_add_to_stream = false;
            }

            if let Ok(choice_point) = cco.clone().into_any().downcast::<ChoicePoint>() {
                let choice = self.process_choice(&choice_point)?;
                if let Some(choice) = choice {
                    self.get_state_mut()
                        .get_generated_choices_mut()
                        .push(choice);
                }

                current_content_obj = None;
                should_add_to_stream = false;
            }
        }

        // Content to add to evaluation stack or the output stream
        if should_add_to_stream {
            // If we're pushing a variable pointer onto the evaluation stack,
            // ensure that it's specific
            // to our current (possibly temporary) context index. And make a
            // copy of the pointer
            // so that we're not editing the original runtime Object.
            let var_pointer =
                Value::get_variable_pointer_value(current_content_obj.as_ref().unwrap().as_ref());

            if let Some(var_pointer) = var_pointer {
                if var_pointer.context_index == -1 {
                    // Create new Object so we're not overwriting the story's own
                    // data
                    let context_idx = self
                        .get_state()
                        .get_callstack()
                        .borrow()
                        .context_for_variable_named(&var_pointer.variable_name);
                    current_content_obj = Some(Rc::new(Value::new_variable_pointer(
                        &var_pointer.variable_name,
                        context_idx as i32,
                    )));
                }
            }

            // Expression evaluation content
            if self.get_state().get_in_expression_evaluation() {
                self.get_state_mut()
                    .push_evaluation_stack(current_content_obj.as_ref().unwrap().clone());
            }
            // Output stream content (i.e. not expression evaluation)
            else {
                self.get_state_mut()
                    .push_to_output_stream(current_content_obj.as_ref().unwrap().clone());
            }
        }

        // Increment the content pointer, following diverts if necessary
        self.next_content()?;

        // Starting a thread should be done after the increment to the content
        // pointer,
        // so that when returning from the thread, it returns to the content
        // after this instruction.
        if current_content_obj.is_some() {
            if let Some(control_cmd) = current_content_obj
                .as_ref()
                .unwrap()
                .as_any()
                .downcast_ref::<ControlCommand>()
            {
                if control_cmd.command_type == CommandType::StartThread {
                    self.get_state().get_callstack().borrow_mut().push_thread();
                }
            }
        }

        Ok(())
    }

    pub(crate) fn next_content(&mut self) -> Result<(), StoryError> {
        // Setting previousContentObject is critical for
        // VisitChangedContainersDueToDivert
        let cp = self.get_state().get_current_pointer();
        self.get_state_mut().set_previous_pointer(cp);

        // Divert step?
        if !self.get_state().diverted_pointer.is_null() {
            let dp = self.get_state().diverted_pointer.clone();
            self.get_state_mut().set_current_pointer(dp);
            self.get_state_mut()
                .set_diverted_pointer(pointer::NULL.clone());

            // Internally uses state.previousContentObject and
            // state.currentContentObject
            self.visit_changed_containers_due_to_divert();

            // Diverted location has valid content?
            if !self.get_state().get_current_pointer().is_null() {
                return Ok(());
            }

            // Otherwise, if diverted location doesn't have valid content,
            // drop down and attempt to increment.
            // This can happen if the diverted path is intentionally jumping
            // to the end of a container - e.g. a Conditional that's
            // re-joining
        }

        let successful_pointer_increment = self.increment_content_pointer();

        // Ran out of content? Try to auto-exit from a function,
        // or finish evaluating the content of a thread
        if !successful_pointer_increment {
            let mut did_pop = false;

            let can_pop_type = self
                .get_state()
                .get_callstack()
                .as_ref()
                .borrow()
                .can_pop_type(Some(PushPopType::Function));
            if can_pop_type {
                // Pop from the call stack
                self.get_state_mut()
                    .pop_callstack(Some(PushPopType::Function))?;

                // This pop was due to dropping off the end of a function that
                // didn't return anything,
                // so in this case, we make sure that the evaluator has
                // something to chomp on if it needs it
                if self.get_state().get_in_expression_evaluation() {
                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Void::new()));
                }

                did_pop = true;
            } else if self
                .get_state()
                .get_callstack()
                .as_ref()
                .borrow()
                .can_pop_thread()
            {
                self.get_state()
                    .get_callstack()
                    .as_ref()
                    .borrow_mut()
                    .pop_thread()?;

                did_pop = true;
            } else {
                self.get_state_mut()
                    .try_exit_function_evaluation_from_game();
            }

            // Step past the point where we last called out
            if did_pop && !self.get_state().get_current_pointer().is_null() {
                self.next_content()?;
            }
        }

        Ok(())
    }

    pub(crate) fn increment_content_pointer(&self) -> bool {
        let mut successful_increment = true;

        let mut pointer = self
            .get_state()
            .get_callstack()
            .as_ref()
            .borrow()
            .get_current_element()
            .current_pointer
            .clone();
        pointer.index += 1;

        let mut container = pointer.container.as_ref().unwrap().clone();

        // Each time we step off the end, we fall out to the next container, all
        // the
        // while we're in indexed rather than named content
        while pointer.index >= container.content.len() as i32 {
            successful_increment = false;

            let next_ancestor = container.get_object().get_parent();

            if next_ancestor.is_none() {
                break;
            }

            let rto: Rc<dyn RTObject> = container;
            let index_in_ancestor = next_ancestor
                .as_ref()
                .unwrap()
                .content
                .iter()
                .position(|s| Rc::ptr_eq(s, &rto));
            if index_in_ancestor.is_none() {
                break;
            }

            pointer = Pointer::new(next_ancestor, index_in_ancestor.unwrap() as i32);
            container = pointer.container.as_ref().unwrap().clone();

            // Increment to next content in outer container
            pointer.index += 1;

            successful_increment = true;
        }

        if !successful_increment {
            pointer = pointer::NULL.clone();
        }

        self.get_state()
            .get_callstack()
            .as_ref()
            .borrow_mut()
            .get_current_element_mut()
            .current_pointer = pointer;

        successful_increment
    }

    pub(crate) fn calculate_newline_output_state_change(
        prev_text: &str,
        curr_text: &str,
        prev_tag_count: i32,
        curr_tag_count: i32,
    ) -> OutputStateChange {
        // Simple case: nothing's changed, and we still have a newline
        // at the end of the current content
        let newline_still_exists = curr_text.len() >= prev_text.len()
            && !prev_text.is_empty()
            && curr_text.chars().nth(prev_text.len() - 1) == Some('\n');
        if prev_tag_count == curr_tag_count
            && prev_text.len() == curr_text.len()
            && newline_still_exists
        {
            return OutputStateChange::NoChange;
        }

        // Old newline has been removed, it wasn't the end of the line after all
        if !newline_still_exists {
            return OutputStateChange::NewlineRemoved;
        }

        // Tag added - definitely the start of a new line
        if curr_tag_count > prev_tag_count {
            return OutputStateChange::ExtendedBeyondNewline;
        }

        // There must be new content - check whether it's just whitespace
        for c in curr_text.chars().skip(prev_text.len()) {
            if c != ' ' && c != '\t' {
                return OutputStateChange::ExtendedBeyondNewline;
            }
        }

        // There's new text but it's just spaces and tabs, so there's still the
        // potential
        // for glue to kill the newline.
        OutputStateChange::NoChange
    }

    pub(crate) fn visit_container(&mut self, container: &Rc<Container>, at_start: bool) {
        if !container.counting_at_start_only || at_start {
            if container.visits_should_be_counted {
                self.get_state_mut()
                    .increment_visit_count_for_container(container);
            }

            if container.turn_index_should_be_counted {
                self.get_state_mut()
                    .record_turn_index_visit_to_container(container);
            }
        }
    }

    /// The vector of [`Choice`](crate::choice::Choice) objects available at
    /// the current point in the `Story`. This vector will be
    /// populated as the `Story` is stepped through with the
    /// [`cont`](Story::cont) method.
    /// Once [`can_continue`](Story::can_continue) becomes `false`, this
    /// vector will be populated, and is usually (but not always) on the
    /// final [`cont`](Story::cont) step.
    pub fn get_current_choices(&self) -> Vec<Rc<Choice>> {
        // Don't include invisible choices for external usage.
        let mut choices = Vec::new();

        if let Some(current_choices) = self.get_state().get_current_choices() {
            for c in current_choices {
                if !c.is_invisible_default {
                    c.index.replace(choices.len());
                    choices.push(c.clone());
                }
            }
        }

        choices
    }

    /// The string of output text available at the current point in
    /// the `Story`. This string will be built as the `Story` is stepped
    /// through with the [`cont`](Story::cont) method.
    pub fn get_current_text(&mut self) -> Result<String, StoryError> {
        self.if_async_we_cant("call currentText since it's a work in progress")?;
        Ok(self.get_state_mut().get_current_text())
    }
}
