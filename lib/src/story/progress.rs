use crate::{
    choice::Choice,
    choice_point::ChoicePoint,
    container::Container,
    control_command::{CommandType, ControlCommand},
    divert::Divert,
    ink_list::InkList,
    ink_list_item::InkListItem,
    native_function_call::NativeFunctionCall,
    object::RTObject,
    pointer::{self, Pointer},
    push_pop::PushPopType,
    story::{OutputStateChange, Story},
    story_callbacks::ErrorType,
    story_error::StoryError,
    story_state::StoryState,
    tag::Tag,
    value::Value,
    value_type::ValueType,
    variable_assigment::VariableAssignment,
    variable_reference::VariableReference,
    void::Void,
};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
    time::Instant,
};

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

        // Start timing
        let duration_stopwatch = Instant::now();

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
                && duration_stopwatch.elapsed().as_millis() as f32 > millisecs_limit_async
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

    pub(crate) fn perform_logic_and_flow_control(
        &mut self,
        content_obj: &Option<Rc<dyn RTObject>>,
    ) -> Result<bool, StoryError> {
        let content_obj = match content_obj {
            Some(content_obj) => content_obj.clone(),
            None => return Ok(false),
        };

        // Divert
        if let Ok(current_divert) = content_obj.clone().into_any().downcast::<Divert>() {
            if current_divert.is_conditional {
                let o = self.get_state_mut().pop_evaluation_stack();
                if !self.is_truthy(o)? {
                    return Ok(true);
                }
            }

            if current_divert.has_variable_target() {
                let var_name = &current_divert.variable_divert_name;
                if let Some(var_contents) = self
                    .get_state()
                    .variables_state
                    .get_variable_with_name(var_name.as_ref().unwrap(), -1)
                {
                    if let Some(target) = Value::get_divert_target_value(var_contents.as_ref()) {
                        let p = Self::pointer_at_path(&self.main_content_container, target)?;
                        self.get_state_mut().set_diverted_pointer(p);
                    } else {
                        let error_message = format!(
                            "Tried to divert to a target from a variable, but the variable ({}) didn't contain a divert target, it ",
                            var_name.as_ref().unwrap()
                        );

                        let error_message = if let ValueType::Int(int_content) = var_contents.value
                        {
                            if int_content == 0 {
                                format!("{}was empty/null (the value 0).", error_message)
                            } else {
                                format!("{}contained '{}'.", error_message, var_contents)
                            }
                        } else {
                            error_message
                        };

                        return Err(StoryError::InvalidStoryState(error_message));
                    }
                } else {
                    return Err(StoryError::InvalidStoryState(format!("Tried to divert using a target from a variable that could not be found ({})", var_name.as_ref().unwrap())));
                }
            } else if current_divert.is_external {
                self.call_external_function(
                    &current_divert.get_target_path_string().unwrap(),
                    current_divert.external_args,
                )?;
                return Ok(true);
            } else {
                self.get_state_mut()
                    .set_diverted_pointer(current_divert.get_target_pointer());
            }

            if current_divert.pushes_to_stack {
                self.get_state().get_callstack().borrow_mut().push(
                    current_divert.stack_push_type,
                    0,
                    self.get_state().get_output_stream().len() as i32,
                );
            }

            if self.get_state().diverted_pointer.is_null() && !current_divert.is_external {
                //     error(format!("Divert resolution failed: {:?}",
                // current_divert));
            }

            return Ok(true);
        }

        if let Some(eval_command) = content_obj
            .as_ref()
            .as_any()
            .downcast_ref::<ControlCommand>()
        {
            match eval_command.command_type {
                CommandType::EvalStart => {
                    if self.get_state().get_in_expression_evaluation() {
                        return Err(StoryError::InvalidStoryState(
                            "Already in expression evaluation?".to_owned(),
                        ));
                    }

                    self.get_state().set_in_expression_evaluation(true);
                }
                CommandType::EvalOutput => {
                    // If the expression turned out to be empty, there may not be
                    // anything on the stack
                    if !self.get_state().evaluation_stack.is_empty() {
                        let output = self.get_state_mut().pop_evaluation_stack();

                        // Functions may evaluate to Void, in which case we skip
                        // output
                        if !output.as_ref().as_any().is::<Void>() {
                            // TODO: Should we really always blanket convert to
                            // string?
                            // It would be okay to have numbers in the output stream
                            // the
                            // only problem is when exporting text for viewing, it
                            // skips over numbers etc.
                            let text: Rc<dyn RTObject> =
                                Rc::new(Value::new_string(&output.to_string()));

                            self.get_state_mut().push_to_output_stream(text);
                        }
                    }
                }
                CommandType::EvalEnd => {
                    if !self.get_state().get_in_expression_evaluation() {
                        return Err(StoryError::InvalidStoryState(
                            "Not in expression evaluation mode".to_owned(),
                        ));
                    }
                    self.get_state().set_in_expression_evaluation(false);
                }
                CommandType::Duplicate => {
                    let obj = self.get_state().peek_evaluation_stack().unwrap().clone();
                    self.get_state_mut().push_evaluation_stack(obj);
                }
                CommandType::PopEvaluatedValue => {
                    self.get_state_mut().pop_evaluation_stack();
                }
                CommandType::PopFunction | CommandType::PopTunnel => {
                    let pop_type = if CommandType::PopFunction == eval_command.command_type {
                        PushPopType::Function
                    } else {
                        PushPopType::Tunnel
                    };

                    // Tunnel onwards is allowed to specify an optional override
                    // divert to go to immediately after returning: ->-> target
                    let mut override_tunnel_return_target = None;
                    if pop_type == PushPopType::Tunnel {
                        let popped = self.get_state_mut().pop_evaluation_stack();

                        if let Some(v) = Value::get_divert_target_value(popped.as_ref()) {
                            override_tunnel_return_target = Some(v.clone());
                        }

                        if override_tunnel_return_target.is_none()
                            && !popped.as_ref().as_any().is::<Void>()
                        {
                            return Err(StoryError::InvalidStoryState(
                                "Expected void if ->-> doesn't override target".to_owned(),
                            ));
                        }
                    }

                    if self
                        .get_state_mut()
                        .try_exit_function_evaluation_from_game()
                    {
                        return Ok(true);
                    } else if self
                        .get_state()
                        .get_callstack()
                        .borrow()
                        .get_current_element()
                        .push_pop_type
                        != pop_type
                        || !self.get_state().get_callstack().borrow().can_pop()
                    {
                        let mut names: HashMap<PushPopType, String> = HashMap::new();
                        names.insert(
                            PushPopType::Function,
                            "function return statement (~ return)".to_owned(),
                        );
                        names.insert(
                            PushPopType::Tunnel,
                            "tunnel onwards statement (->->)".to_owned(),
                        );

                        let mut expected = names
                            .get(
                                &self
                                    .get_state()
                                    .get_callstack()
                                    .borrow()
                                    .get_current_element()
                                    .push_pop_type,
                            )
                            .cloned();
                        if !self.get_state().get_callstack().borrow().can_pop() {
                            expected = Some("end of flow (-> END or choice)".to_owned());
                        }

                        return Err(StoryError::InvalidStoryState(format!(
                            "Found {}, when expected {}",
                            names.get(&pop_type).unwrap(),
                            expected.unwrap()
                        )));
                    } else {
                        self.get_state_mut().pop_callstack(None)?;

                        // Does tunnel onwards override by diverting to a new ->->
                        // target?
                        if let Some(override_tunnel_return_target) = override_tunnel_return_target {
                            let p = Self::pointer_at_path(
                                &self.main_content_container,
                                &override_tunnel_return_target,
                            )?;
                            self.get_state_mut().set_diverted_pointer(p);
                        }
                    }
                }
                CommandType::BeginString => {
                    self.get_state_mut()
                        .push_to_output_stream(content_obj.clone());

                    if !self.get_state().get_in_expression_evaluation() {
                        return Err(StoryError::InvalidStoryState(
                            "Expected to be in an expression when evaluating a string".to_owned(),
                        ));
                    }

                    self.get_state().set_in_expression_evaluation(false);
                }
                CommandType::EndString => {
                    // Since we're iterating backward through the content,
                    // build a stack so that when we build the string,
                    // it's in the right order
                    let mut content_stack_for_string: VecDeque<Rc<dyn RTObject>> = VecDeque::new();
                    let mut content_to_retain: VecDeque<Rc<dyn RTObject>> = VecDeque::new();

                    let mut output_count_consumed = 0;

                    for i in (0..self.get_state().get_output_stream().len()).rev() {
                        let obj = &self.get_state().get_output_stream()[i];
                        output_count_consumed += 1;

                        if let Some(command) =
                            obj.as_ref().as_any().downcast_ref::<ControlCommand>()
                        {
                            if command.command_type == CommandType::BeginString {
                                break;
                            }
                        }

                        if obj.as_ref().as_any().downcast_ref::<Tag>().is_some() {
                            content_to_retain.push_back(obj.clone());
                        }

                        if Value::get_string_value(obj.as_ref()).is_some() {
                            content_stack_for_string.push_back(obj.clone());
                        }
                    }

                    // Consume the content that was produced for this string
                    self.get_state_mut()
                        .pop_from_output_stream(output_count_consumed);

                    // Rescue the tags that we want actually to keep on the output stack
                    // rather than consume as part of the string we're building.
                    // At the time of writing, this only applies to Tag objects generated
                    // by choices, which are pushed to the stack during string generation.
                    for rescued_tag in content_to_retain.iter() {
                        self.get_state_mut()
                            .push_to_output_stream(rescued_tag.clone());
                    }

                    // Build string out of the content we collected
                    let mut sb = String::new();

                    while let Some(c) = content_stack_for_string.pop_back() {
                        sb.push_str(&c.to_string());
                    }

                    // Return to expression evaluation (from content mode)
                    self.get_state().set_in_expression_evaluation(true);
                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Value::new_string(&sb)));
                }
                CommandType::NoOp => {}
                CommandType::ChoiceCount => {
                    let choice_count = self.get_state().get_generated_choices().len();
                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Value::new_int(choice_count as i32)));
                }
                CommandType::Turns => {
                    let current_turn = self.get_state().current_turn_index;
                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Value::new_int(current_turn + 1)));
                }
                CommandType::TurnsSince | CommandType::ReadCount => {
                    let target = self.get_state_mut().pop_evaluation_stack();
                    if Value::get_divert_target_value(target.as_ref()).is_none() {
                        let mut extra_note = "".to_owned();
                        if Value::get_int_value(target.as_ref()).is_some() {
                            extra_note = format!(". Did you accidentally pass a read count ('knot_name') instead of a target {}",
                                    "('-> knot_name')?").to_owned();
                        }

                        return Err(StoryError::InvalidStoryState(format!("TURNS_SINCE expected a divert target (knot, stitch, label name), but saw {} {}", target
                                , extra_note)));
                    }

                    let target = Value::get_divert_target_value(target.as_ref()).unwrap();

                    let otmp = self.content_at_path(target).correct_obj();
                    let container = match &otmp {
                        Some(o) => o.clone().into_any().downcast::<Container>().ok(),
                        None => None,
                    };

                    let either_count: i32;

                    match container {
                        Some(container) => {
                            if eval_command.command_type == CommandType::TurnsSince {
                                either_count = self
                                    .get_state()
                                    .turns_since_for_container(container.as_ref())?;
                            } else {
                                either_count =
                                    self.get_state_mut().visit_count_for_container(&container);
                            }
                        }
                        None => {
                            if eval_command.command_type == CommandType::TurnsSince {
                                either_count = -1; // turn count, default to
                                                   // never/unknown
                            } else {
                                either_count = 0;
                            } // visit count, assume 0 to default to allowing entry

                            self.add_error(
                                &format!(
                                    "Failed to find container for {} lookup at {}",
                                    eval_command, target
                                ),
                                true,
                            );
                        }
                    }

                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Value::new_int(either_count)));
                }
                CommandType::Random => {
                    let mut max_int = None;
                    let o = self.get_state_mut().pop_evaluation_stack();

                    if let Some(v) = Value::get_int_value(o.as_ref()) {
                        max_int = Some(v);
                    }

                    let o = self.get_state_mut().pop_evaluation_stack();

                    let mut min_int = None;
                    if let Some(v) = Value::get_int_value(o.as_ref()) {
                        min_int = Some(v);
                    }

                    if min_int.is_none() {
                        return Err(StoryError::InvalidStoryState(
                            "Invalid value for the minimum parameter of RANDOM(min, max)"
                                .to_owned(),
                        ));
                    }

                    if max_int.is_none() {
                        return Err(StoryError::InvalidStoryState(
                            "Invalid value for the maximum parameter of RANDOM(min, max)"
                                .to_owned(),
                        ));
                    }

                    let min_value = min_int.unwrap();
                    let max_value = max_int.unwrap();

                    let random_range = max_value - min_value + 1;

                    if random_range <= 0 {
                        return Err(StoryError::InvalidStoryState(format!(
                            "RANDOM was called with minimum as {} and maximum as {}. The maximum must be larger",
                            min_value, max_value
                        )));
                    }

                    let result_seed =
                        self.get_state().story_seed + self.get_state().previous_random;

                    let mut rng = StdRng::seed_from_u64(result_seed as u64);
                    let next_random = rng.gen::<u32>();

                    let chosen_value = (next_random % random_range as u32) as i32 + min_value;

                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Value::new_int(chosen_value)));

                    self.get_state_mut().previous_random = self.get_state().previous_random + 1;
                }
                CommandType::SeedRandom => {
                    let mut seed: Option<i32> = None;

                    let o = self.get_state_mut().pop_evaluation_stack();

                    if let Some(v) = Value::get_int_value(o.as_ref()) {
                        seed = Some(v);
                    }

                    if seed.is_none() {
                        return Err(StoryError::InvalidStoryState(
                            "Invalid value passed to SEED_RANDOM".to_owned(),
                        ));
                    }

                    // Story seed affects both RANDOM and shuffle behaviour
                    self.get_state_mut().story_seed = seed.unwrap();
                    self.get_state_mut().previous_random = 0;

                    // SEED_RANDOM returns nothing.
                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Void::new()));
                }
                CommandType::VisitIndex => {
                    let cpc = self.get_state().get_current_pointer().container.unwrap();
                    let count = self.get_state_mut().visit_count_for_container(&cpc) - 1; // index
                                                                                          // not count
                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Value::new_int(count)));
                }
                CommandType::SequenceShuffleIndex => {
                    let shuffle_index = self.next_sequence_shuffle_index()?;
                    let v = Rc::new(Value::new_int(shuffle_index));
                    self.get_state_mut().push_evaluation_stack(v);
                }
                CommandType::StartThread => {
                    // Handled in main step function
                }
                CommandType::Done => {
                    // We may exist in the context of the initial
                    // act of creating the thread, or in the context of
                    // evaluating the content.
                    if self.get_state().get_callstack().borrow().can_pop_thread() {
                        self.get_state()
                            .get_callstack()
                            .as_ref()
                            .borrow_mut()
                            .pop_thread()?;
                    }
                    // In normal flow - allow safe exit without warning
                    else {
                        self.get_state_mut().set_did_safe_exit(true);

                        // Stop flow in current thread
                        self.get_state().set_current_pointer(pointer::NULL.clone());
                    }
                }
                CommandType::End => self.get_state_mut().force_end(),
                CommandType::ListFromInt => {
                    let mut int_val: Option<i32> = None;
                    let mut list_name_val: Option<&String> = None;

                    let o = self.get_state_mut().pop_evaluation_stack();

                    if let Some(v) = Value::get_int_value(o.as_ref()) {
                        int_val = Some(v);
                    }

                    let o = self.get_state_mut().pop_evaluation_stack();

                    if let Some(s) = Value::get_string_value(o.as_ref()) {
                        list_name_val = Some(&s.string);
                    }

                    if int_val.is_none() {
                        return Err(StoryError::InvalidStoryState("Passed non-integer when creating a list element from a numerical value.".to_owned()));
                    }

                    let mut generated_list_value: Option<Value> = None;

                    if let Some(found_list_def) = self
                        .list_definitions
                        .as_ref()
                        .get_list_definition(list_name_val.as_ref().unwrap())
                    {
                        if let Some(found_item) =
                            found_list_def.get_item_with_value(int_val.unwrap())
                        {
                            let l = InkList::from_single_element((
                                found_item.clone(),
                                int_val.unwrap(),
                            ));
                            generated_list_value = Some(Value::new_list(l));
                        }
                    } else {
                        return Err(StoryError::InvalidStoryState(format!(
                            "Failed to find List called {}",
                            list_name_val.as_ref().unwrap()
                        )));
                    }

                    if generated_list_value.is_none() {
                        generated_list_value = Some(Value::new_list(InkList::new()));
                    }

                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(generated_list_value.unwrap()));
                }
                CommandType::ListRange => {
                    let mut p = self.get_state_mut().pop_evaluation_stack();
                    let max = p.into_any().downcast::<Value>();

                    p = self.get_state_mut().pop_evaluation_stack();
                    let min = p.into_any().downcast::<Value>();

                    p = self.get_state_mut().pop_evaluation_stack();
                    let target_list = Value::get_list_value(p.as_ref());

                    if target_list.is_none() || min.is_err() || max.is_err() {
                        return Err(StoryError::InvalidStoryState(
                            "Expected List, minimum and maximum for LIST_RANGE".to_owned(),
                        ));
                    }

                    let result = target_list
                        .unwrap()
                        .list_with_sub_range(&min.unwrap().value, &max.unwrap().value);

                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Value::new_list(result)));
                }
                CommandType::ListRandom => {
                    let o = self.get_state_mut().pop_evaluation_stack();
                    let list = Value::get_list_value(o.as_ref());

                    if list.is_none() {
                        return Err(StoryError::InvalidStoryState(
                            "Expected list for LIST_RANDOM".to_owned(),
                        ));
                    }

                    let list = list.unwrap();

                    let new_list = {
                        // List was empty: return empty list
                        if list.items.is_empty() {
                            InkList::new()
                        }
                        // Non-empty source list
                        else {
                            // Generate a random index for the element to take
                            let result_seed =
                                self.get_state().story_seed + self.get_state().previous_random;
                            let mut rng = StdRng::seed_from_u64(result_seed as u64);
                            let next_random = rng.gen::<u32>();
                            let list_item_index = (next_random as usize) % list.items.len();

                            // Iterate through to get the random element, sorted for
                            // predictibility
                            let mut sorted: Vec<(&InkListItem, &i32)> = list.items.iter().collect();
                            sorted.sort_by(|a, b| b.1.cmp(a.1));
                            let random_item = sorted[list_item_index];

                            // Origin list is simply the origin of the one element
                            let mut new_list = InkList::from_single_origin(
                                random_item.0.get_origin_name().unwrap().clone(),
                                self.list_definitions.as_ref(),
                            )?;
                            new_list.items.insert(random_item.0.clone(), *random_item.1);

                            self.get_state_mut().previous_random = next_random as i32;

                            new_list
                        }
                    };

                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Value::new_list(new_list)));
                }
                CommandType::BeginTag => self
                    .get_state_mut()
                    .push_to_output_stream(content_obj.clone()),
                CommandType::EndTag => {
                    // EndTag has 2 modes:
                    //  - When in string evaluation (for choices)
                    //  - Normal
                    //
                    // The only way you could have an EndTag in the middle of
                    // string evaluation is if we're currently generating text for a
                    // choice, such as:
                    //
                    //   + choice # tag
                    //
                    // In the above case, the ink will be run twice:
                    //  - First, to generate the choice text. String evaluation will be on, and the
                    //    final string will be pushed to the evaluation stack, ready to be popped to
                    //    make a Choice object.
                    //  - Second, when ink generates text after choosing the choice. On this
                    //    ocassion, it's not in string evaluation mode.
                    //
                    // On the writing side, we disallow manually putting tags within
                    // strings like this:
                    //
                    //   {"hello # world"}
                    //
                    // So we know that the tag must be being generated as part of
                    // choice content. Therefore, when the tag has been generated,
                    // we push it onto the evaluation stack in the exact same way
                    // as the string for the choice content.
                    if self.get_state().in_string_evaluation() {
                        let mut content_stack_for_tag: Vec<String> = Vec::new();
                        let mut output_count_consumed = 0;

                        for i in (0..self.get_state().get_output_stream().len()).rev() {
                            let obj = &self.get_state().get_output_stream()[i];

                            output_count_consumed += 1;

                            if let Some(command) =
                                obj.as_ref().as_any().downcast_ref::<ControlCommand>()
                            {
                                if command.command_type == CommandType::BeginTag {
                                    break;
                                } else {
                                    return Err(StoryError::InvalidStoryState("Unexpected ControlCommand while extracting tag from choice".to_owned()));
                                }
                            }

                            if let Some(sv) = Value::get_string_value(obj.as_ref()) {
                                content_stack_for_tag.push(sv.string.clone());
                            }
                        }

                        // Consume the content that was produced for this string
                        self.get_state_mut()
                            .pop_from_output_stream(output_count_consumed);

                        let mut sb = String::new();
                        for str_val in &content_stack_for_tag {
                            sb.push_str(str_val);
                        }

                        let choice_tag =
                            Rc::new(Tag::new(&StoryState::clean_output_whitespace(&sb)));
                        // Pushing to the evaluation stack means it gets picked up
                        // when a Choice is generated from the next Choice Point.
                        self.get_state_mut().push_evaluation_stack(choice_tag);
                    }
                    // Otherwise! Simply push EndTag, so that in the output stream we
                    // have a structure of: [BeginTag, "the tag content", EndTag]
                    else {
                        self.get_state_mut()
                            .push_to_output_stream(content_obj.clone());
                    }
                }
            }

            return Ok(true);
        }

        // Variable assignment
        if let Some(var_ass) = content_obj
            .as_ref()
            .as_any()
            .downcast_ref::<VariableAssignment>()
        {
            let assigned_val = self.get_state_mut().pop_evaluation_stack();

            // When in temporary evaluation, don't create new variables purely
            // within
            // the temporary context, but attempt to create them globally
            // var prioritiseHigherInCallStack = _temporaryEvaluationContainer
            // != null;
            let assigned_val = assigned_val.into_any().downcast::<Value>().unwrap();
            self.get_state_mut()
                .variables_state
                .assign(var_ass, assigned_val)?;

            return Ok(true);
        }

        // Variable reference
        if let Ok(var_ref) = content_obj
            .clone()
            .into_any()
            .downcast::<VariableReference>()
        {
            let found_value: Rc<Value>;

            // Explicit read count value
            if var_ref.path_for_count.is_some() {
                let container = var_ref.get_container_for_count();
                let count = self
                    .get_state_mut()
                    .visit_count_for_container(container.as_ref().unwrap());
                found_value = Rc::new(Value::new_int(count));
            }
            // Normal variable reference
            else {
                match self
                    .get_state()
                    .variables_state
                    .get_variable_with_name(&var_ref.name, -1)
                {
                    Some(v) => found_value = v,
                    None => {
                        self.add_error(&format!("Variable not found: '{}'. Using default value of 0 (false). This can happen with temporary variables if the declaration hasn't yet been hit. Globals are always given a default value on load if a value doesn't exist in the save state.", var_ref.name), true);

                        found_value = Rc::new(Value::new_int(0));
                    }
                }
            }

            self.get_state_mut().push_evaluation_stack(found_value);

            return Ok(true);
        }

        // Native function call
        if let Some(func) = content_obj
            .as_ref()
            .as_any()
            .downcast_ref::<NativeFunctionCall>()
        {
            let func_params = self
                .get_state_mut()
                .pop_evaluation_stack_multiple(func.get_number_of_parameters());

            let result = func.call(func_params)?;
            self.get_state_mut().push_evaluation_stack(result);

            return Ok(true);
        }

        Ok(false)
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
