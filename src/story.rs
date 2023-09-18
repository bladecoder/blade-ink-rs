#![allow(unused_variables, dead_code)]

use std::{rc::Rc, time::Instant, collections::VecDeque};

use crate::{
    container::Container,
    error::ErrorType,
    json_serialization,
    push_pop::PushPopType,
    story_state::StoryState, pointer::{Pointer, self}, object::{RTObject, Object}, void::Void, path::Path, control_command::{ControlCommand, CommandType}, choice::Choice, value::Value, tag::Tag, divert::Divert, choice_point::ChoicePoint,
};

const INK_VERSION_CURRENT: i32 = 21;
const INK_VERSION_MINIMUM_COMPATIBLE: i32 = 18;

#[derive(PartialEq)]
enum OutputStateChange {
    NoChange,
    ExtendedBeyondNewline,
    NewlineRemoved
}

pub struct Story {
    main_content_container: Rc<Container>,
    state: Option<StoryState>,
    temporaty_evaluation_container: Option<Rc<Container>>,
    recursive_continue_count: usize,
    async_continue_active: bool,
    async_saving: bool,
    saw_lookahead_unsafe_function_after_new_line: bool,
    state_snapshot_at_last_new_line: Option<StoryState>,
    on_error: Option<fn(message: &str, error_type: ErrorType)>,
}

impl Story {
    pub fn new(json_string: &str) -> Result<Self, String> {
        let json: serde_json::Value = match serde_json::from_str(json_string) {
            Ok(value) => value,
            Err(_) => return Err("Story not in JSON format.".to_string()),
        };

        let version_opt = json.get("inkVersion");

        if version_opt.is_none() || !version_opt.unwrap().is_number() {
            return Err(
                "ink version number not found. Are you sure it's a valid .ink.json file?"
                    .to_string(),
            );
        }

        let version: i32 = version_opt.unwrap().as_i64().unwrap().try_into().unwrap();

        if version > INK_VERSION_CURRENT {
            return Err("Version of ink used to build story was newer than the current version of the engine".to_string());
        } else if version < INK_VERSION_MINIMUM_COMPATIBLE {
            return Err("Version of ink used to build story is too old to be loaded by this version of the engine".to_string());
        } else if version != INK_VERSION_CURRENT {
            log::debug!("WARNING: Version of ink used to build story doesn't match current version of engine. Non-critical, but recommend synchronising.");
        }

        let root_token = match json.get("root") {
            Some(value) => value,
            None => {
                return Err(
                    "Root node for ink not found. Are you sure it's a valid .ink.json file?"
                        .to_string(),
                )
            }
        };

        //object listDefsObj;
        //if (rootObject.TryGetValue ("listDefs", out listDefsObj)) {
        //    _listDefinitions = Json.JTokenToListDefinitions (listDefsObj);
        //}

        let main_content_container = json_serialization::jtoken_to_runtime_object(root_token, None)?;

        let main_content_container = main_content_container.into_any().downcast::<Container>();

        if main_content_container.is_err() {
            return Err("Root node for ink is not a container?".to_string());
        };

        let mut story = Story {
            main_content_container: main_content_container.unwrap(),
            state: None,
            temporaty_evaluation_container: None,
            recursive_continue_count: 0,
            async_continue_active: false,
            async_saving: false,
            saw_lookahead_unsafe_function_after_new_line: false,
            state_snapshot_at_last_new_line: None,
            on_error: None,
        };

        story.reset_state();

        Ok(story)
    }

    fn reset_state(&mut self) {
        //TODO ifAsyncWeCant("ResetState");

        self.state = Some(StoryState::new(self.main_content_container.clone()));

        // TODO state.getVariablesState().setVariableChangedEvent(this);

        self.reset_globals();
    }

    fn reset_globals(&mut self) {
        // TODO
        // if (self.main_content_container.get_named_content().containsKey("global decl")) {
        //     let originalPointer = self.state.as_ref().unwrap().get_current_pointer().clone();

        //     self.choose_path(Path::new_with_components_string(Some("global decl".to_string())), false);

        //     // Continue, but without validating external bindings,
        //     // since we may be doing this reset at initialisation time.
        //     self.continue_internal();

        //     self.state.as_ref().unwrap().set_current_pointer(originalPointer);
        // }

        self.state.as_mut().unwrap().get_variables_state_mut().snapshot_default_globals();
    }

    pub fn build_string_of_hierarchy(&self) -> String {
        let mut sb = String::new();

        let cp = self.state.as_ref().unwrap().get_current_pointer().resolve();

        let cp = match cp {
            Some(_) => Some(cp.as_ref().unwrap().as_ref()),
            None => None,
        };

        self.main_content_container
            .build_string_of_hierarchy(&mut sb, 0, cp);

        sb
    }

    pub fn can_continue(&self) -> bool {
        self.state.as_ref().unwrap().can_continue()
    }

    pub fn cont(&mut self) -> Result<String, String> {
        self.continue_async(0.0)?;
        Ok(self.get_current_text())
    }

    pub fn continue_async(&mut self, millisecs_limit_async: f32) -> Result<(), String> {
        // TODO: if (!hasValidatedExternals) validateExternalBindings();

        self.continue_internal(millisecs_limit_async)?;

        Ok(())
    }

    fn continue_internal(&mut self, millisecs_limit_async: f32) -> Result<(), String> {
        let is_async_time_limited = millisecs_limit_async > 0.0;

        self.recursive_continue_count += 1;

        // Doing either:
        // - full run through non-async (so not active and don't want to be)
        // - Starting async run-through
        if !self.async_continue_active {
            self.async_continue_active = is_async_time_limited;
            if (!self.can_continue()) {
                return Err(
                    "Can't continue - should check canContinue before calling Continue".to_string(),
                );
            }

            self.state.as_mut().unwrap().set_did_safe_exit(false);

            self.state.as_mut().unwrap().reset_output(None);

            // It's possible for ink to call game to call ink to call game etc
            // In this case, we only want to batch observe variable changes
            // for the outermost call.
            if self.recursive_continue_count == 1 {
                self.state
                    .as_mut()
                    .unwrap()
                    .get_variables_state_mut()
                    .set_batch_observing_variable_changes(true);
            }
        }

        // Start timing
        let duration_stopwatch = Instant::now();

        let mut output_stream_ends_in_newline = false;
        self.saw_lookahead_unsafe_function_after_new_line = false;

        loop {
            match self.continue_single_step() {
                Ok(r) => output_stream_ends_in_newline = r,
                Err(s) => {
                    //self.add_error(s, false, e.useEndLineNumber);
                    break;
                }
            }

            println!("{}", self.build_string_of_hierarchy());

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
                if self
                    .state
                    .as_ref()
                    .unwrap()
                    .get_callstack()
                    .borrow()
                    .can_pop_thread()
                {
                    self.add_error("Thread available to pop, threads should always be flat by the end of evaluation?");
                }

                if self
                    .state
                    .as_ref()
                    .unwrap()
                    .get_generated_choices()
                    .is_empty()
                    && !self.state.as_ref().unwrap().is_did_safe_exit()
                    && self.temporaty_evaluation_container.is_none()
                {
                    if self
                        .state
                        .as_ref()
                        .unwrap()
                        .get_callstack()
                        .borrow()
                        .can_pop_type(PushPopType::Tunnel)
                    {
                        self.add_error("unexpectedly reached end of content. Do you need a '->->' to return from a tunnel?");
                    } else if self
                        .state
                        .as_ref()
                        .unwrap()
                        .get_callstack()
                        .borrow()
                        .can_pop_type(PushPopType::Function)
                    {
                        self.add_error(
                            "unexpectedly reached end of content. Do you need a '~ return'?",
                        );
                    } else if !self.state.as_ref().unwrap().get_callstack().borrow().can_pop() {
                        self.add_error("ran out of content. Do you need a '-> DONE' or '-> END'?");
                    } else {
                        self.add_error("unexpectedly reached end of content for unknown reason. Please debug compiler!");
                    }
                }
            }
            self.state.as_mut().unwrap().set_did_safe_exit(false);
            self.saw_lookahead_unsafe_function_after_new_line = false;

            if self.recursive_continue_count == 1 {
                self.state
                    .as_mut()
                    .unwrap()
                    .get_variables_state_mut()
                    .set_batch_observing_variable_changes(false);
            }

            self.async_continue_active = false;
        }

        self.recursive_continue_count -= 1;

        // Report any errors that occured during evaluation.
        // This may either have been StoryExceptions that were thrown
        // and caught during evaluation, or directly added with AddError.
        if self.state.as_ref().unwrap().has_error() || self.state.as_ref().unwrap().has_warning() {
            match self.on_error {
                Some(on_err) => {
                    if self.state.as_ref().unwrap().has_error() {
                        for err in self.state.as_ref().unwrap().get_current_errors() {
                            (on_err)(&err, ErrorType::Error);
                        }
                    }

                    if self.state.as_ref().unwrap().has_warning() {
                        for err in self.state.as_ref().unwrap().get_current_warnings() {
                            (on_err)(&err, ErrorType::Warning);
                        }
                    }

                    self.reset_errors();
                }
                // Throw an exception since there's no error handler
                None => {
                    let mut sb = String::new();
                    sb.push_str("Ink had ");

                    if self.state.as_ref().unwrap().has_error() {
                        sb.push_str(&self.state.as_ref().unwrap().get_current_errors().len().to_string());

                        if self.state.as_ref().unwrap().get_current_errors().len() == 1 {
                            sb.push_str(" error");
                        } else {
                            sb.push_str(" errors");
                        }

                        if self.state.as_ref().unwrap().has_warning() {
                            sb.push_str(" and ");
                        }
                    }

                    if self.state.as_ref().unwrap().has_warning() {
                        sb.push_str(self.state.as_ref().unwrap().get_current_warnings().len().to_string().as_str());
                        if self.state.as_ref().unwrap().get_current_errors().len() == 1 {
                            sb.push_str(" warning");
                        } else {
                            sb.push_str(" warnings");
                        }
                    }

                    sb.push_str(". It is strongly suggested that you assign an error handler to story.onError. The first issue was: ");

                    if self.state.as_ref().unwrap().has_error() {
                        sb.push_str(self.state.as_ref().unwrap().get_current_errors()[0].as_str());
                    } else {
                        sb.push_str(self.state.as_ref().unwrap().get_current_warnings()[0].to_string().as_str());
                    }

                    return Err(sb);
                }
            }
        }

        Ok(())
    }

    fn continue_single_step(&mut self) -> Result<bool, String> {
        // Run main step function (walks through content)
        self.step();

        // Run out of content and we have a default invisible choice that we can follow?
        if !self.can_continue() && !self.state.as_ref().unwrap().get_callstack().borrow().element_is_evaluate_from_game() {
            self.try_follow_default_invisible_choice();
        }

        // Don't save/rewind during string evaluation, which is e.g. used for choices
        if !self.state.as_ref().unwrap().in_string_evaluation(){

            // We previously found a newline, but were we just double checking that
            // it wouldn't immediately be removed by glue?
            if let Some(state_snapshot_at_last_new_line) = self.state_snapshot_at_last_new_line.as_mut() {

                // Has proper text or a tag been added? Then we know that the newline
                // that was previously added is definitely the end of the line.
                let change = Story::calculate_newline_output_state_change(
                        &state_snapshot_at_last_new_line.get_current_text(), 
                        &self.state.as_mut().unwrap().get_current_text(),
                        state_snapshot_at_last_new_line.get_current_tags().len() as i32,
                        self.state.as_mut().unwrap().get_current_tags().len() as i32);

                // The last time we saw a newline, it was definitely the end of the line, so we
                // want to rewind to that point.
                if change == OutputStateChange::ExtendedBeyondNewline || self.saw_lookahead_unsafe_function_after_new_line {
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
            if self.state.as_ref().unwrap().output_stream_ends_in_newline() {

                // If we can continue evaluation for a bit:
                // Create a snapshot in case we need to rewind.
                // We're going to continue stepping in case we see glue or some
                // non-text content such as choices.
                if self.can_continue() {

                    // Don't bother to record the state beyond the current newline.
                    // e.g.:
                    // Hello world\n // record state at the end of here
                    // ~ complexCalculation() // don't actually need this unless it generates text
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

        // outputStreamEndsInNewline = false
        return Ok(false);    
    }

    pub fn get_current_text(&mut self) -> String {
        //TODO ifAsyncWeCant("call currentText since it's a work in progress");
        self.state.as_mut().unwrap().get_current_text()
    }

    pub(crate) fn get_main_content_container(&self) -> Rc<Container> {
        match self.temporaty_evaluation_container.as_ref() {
            Some(c) => c.clone(),
            None => self.main_content_container.clone(),
        }
    }

    fn restore_state_snapshot(&mut self) {
        // Patched state had temporarily hijacked our
        // VariablesState and set its own callstack on it,
        // so we need to restore that.
        // If we're in the middle of saving, we may also
        // need to give the VariablesState the old patch.
        self.state_snapshot_at_last_new_line.as_mut().unwrap().restore_after_patch();

        self.state = self.state_snapshot_at_last_new_line.take();

        // If save completed while the above snapshot was
        // active, we need to apply any changes made since
        // the save was started but before the snapshot was made.
        if !self.async_saving {
            self.state.as_mut().unwrap().apply_any_patch();
        }
    }

    fn add_error(&self, arg: &str) {
        todo!()
    }

    fn reset_errors(&self) {
        todo!()
    }

    fn step(&mut self) {
        let mut should_add_to_stream = true;

        // Get current content
        let mut pointer = self.state.as_ref().unwrap().get_current_pointer().clone();

        if pointer.is_null() {
            return;
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

        self.state.as_mut().unwrap().set_current_pointer(pointer.clone());

        // Is the current content Object:
        // - Normal content
        // - Or a logic/flow statement - if so, do it
        // Stop flow if we hit a stack pop when we're unable to pop (e.g.
        // return/done statement in knot
        // that was diverted to rather than called as a function)
        let mut current_content_obj = pointer.resolve();
        let is_logic_or_flow_control = self.perform_logic_and_flow_control(&current_content_obj);

        // Has flow been forced to end by flow control above?
        if self.state.as_ref().unwrap().get_current_pointer().is_null() {
            return;
        }

        if is_logic_or_flow_control {
            should_add_to_stream = false;
        }

        // Choice with condition?
        if let Ok(choicePoint) =  current_content_obj.clone().unwrap().into_any().downcast::<ChoicePoint>() {

            let choice = self.process_choice(&choicePoint);
            if choice.is_some() {
                self.state.as_mut().unwrap().get_generated_choices().push(choice.unwrap());
            }

            current_content_obj = None;
            should_add_to_stream = false;
        }

        // If the container has no content, then it will be
        // the "content" itself, but we skip over it.
        if current_content_obj.is_some() && current_content_obj.as_ref().unwrap().as_any().is::<Container>() {
            should_add_to_stream = false;
        }

        // Content to add to evaluation stack or the output stream
        if should_add_to_stream {

            // If we're pushing a variable pointer onto the evaluation stack,
            // ensure that it's specific
            // to our current (possibly temporary) context index. And make a
            // copy of the pointer
            // so that we're not editing the original runtime Object.
            
            // TODO
            // VariablePointerValue varPointer =
            //         currentContentObj instanceof VariablePointerValue ? (VariablePointerValue) currentContentObj : null;

            // if (varPointer != null && varPointer.getContextIndex() == -1) {

            //     // Create new Object so we're not overwriting the story's own
            //     // data
            //     int contextIdx = state.getCallStack().contextForVariableNamed(varPointer.getVariableName());
            //     currentContentObj = new VariablePointerValue(varPointer.getVariableName(), contextIdx);
            // }

            // Expression evaluation content
            if self.state.as_ref().unwrap().get_in_expression_evaluation() {
                self.state.as_mut().unwrap().push_evaluation_stack(current_content_obj.unwrap());
            }
            // Output stream content (i.e. not expression evaluation)
            else {
                self.state.as_mut().unwrap().push_to_output_stream(current_content_obj);
            }
        }

        // Increment the content pointer, following diverts if necessary
        self.next_content();

        // Starting a thread should be done after the increment to the content
        // pointer,
        // so that when returning from the thread, it returns to the content
        // after this instruction.
        
        // TODO
        // let controlCmd =
        //         currentContentObj instanceof ControlCommand ? (ControlCommand) currentContentObj : null;
        // if (controlCmd != null && controlCmd.getCommandType() == ControlCommand.CommandType.StartThread) {
        //     state.getCallStack().pushThread();
        // }
    }

    fn try_follow_default_invisible_choice(&self) {
        let all_choices = match self.state.as_ref().unwrap().get_current_choices() {
            Some(c) => c,
            None => return,
        };

        // Is a default invisible choice the ONLY choice?
        // var invisibleChoices = allChoices.Where (c =>
        // c.choicePoint.isInvisibleDefault).ToList();
        let mut invisible_choices:Vec<Rc<Choice>>  = Vec::new();
        for c in all_choices {
            if c.is_invisible_default {
                invisible_choices.push(c.clone());
            }
        }

        if invisible_choices.len() == 0 || all_choices.len() > invisible_choices.len() {
            return;
        }

        let choice = &invisible_choices[0];

        // Invisible choice may have been generated on a different thread,
        // in which case we need to restore it before we continue
        self.state.as_ref().unwrap().get_callstack().as_ref().borrow_mut().set_current_thread(choice.thread_at_generation.borrow().as_ref().unwrap().copy());

        // If there's a chance that this state will be rolled back to before
        // the invisible choice then make sure that the choice thread is
        // left intact, and it isn't re-entered in an old state.
        if self.state_snapshot_at_last_new_line.is_some() {
            let fork_thread = self.state.as_ref().unwrap().get_callstack().as_ref().borrow_mut().fork_thread();
            self.state.as_ref().unwrap().get_callstack().as_ref().borrow_mut().set_current_thread(fork_thread);
        }

        self.choose_path(&choice.target_path, false);
    }

    fn calculate_newline_output_state_change(
        prev_text: &str,
        curr_text: &str,
        prev_tag_count: i32,
        curr_tag_count: i32,
    ) -> OutputStateChange {
        // Simple case: nothing's changed, and we still have a newline
        // at the end of the current content
        let newline_still_exists = curr_text.len() >= prev_text.len()
            && prev_text.len() > 0
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

    fn state_snapshot(&mut self) {
        self.state_snapshot_at_last_new_line = self.state.take();
        self.state = Some(self.state_snapshot_at_last_new_line.as_ref().unwrap().copy_and_start_patching());
    }

    fn discard_snapshot(&mut self) {
        // Normally we want to integrate the patch
        // into the main global/counts dictionaries.
        // However, if we're in the middle of async
        // saving, we simply stay in a "patching" state,
        // albeit with the newer cloned patch.
        
        // TODO
        //if (!asyncSaving) state.applyAnyPatch();

        // No longer need the snapshot.
        self.state_snapshot_at_last_new_line = None;    
    }

    fn visit_container(&mut self, container: &Container, at_start: bool) {
        if !container.counting_at_start_only || at_start {
            if container.visits_should_be_counted {
                self.state.as_mut().unwrap().increment_visit_count_for_container(container);
            }

            if container.turn_index_should_be_counted {
                self.state.as_mut().unwrap().record_turn_index_visit_to_container(container);
            }
        }
    }

    fn perform_logic_and_flow_control(&mut self, content_obj: &Option<Rc<dyn RTObject>>) -> bool {
        let content_obj = match content_obj {
            Some(content_obj) => {
                content_obj.clone()
            },
            None => return false,
        };

        // Divert
        if let Some(current_divert) = content_obj.as_ref().as_any().downcast_ref::<Divert>() {
            if current_divert.is_conditional {
                let o = self.state.as_mut().unwrap().pop_evaluation_stack();
                if !self.is_truthy(o) {
                    return true;
                }
            }

            if current_divert.has_variable_target() {
                // let var_name = current_divert.variable_divert_name;
                // if let Some(var_contents) = self.state.as_ref().unwrap().get_variables_state().get_variable_with_name(var_name) {
                //     if let Some(target) = var_contents.downcast_ref::<DivertTargetValue>() {
                //         self.state.as_ref().unwrap().set_diverted_pointer(pointer_at_path(&target.get_target_path()));
                //     } else {
                //         let int_content = var_contents.downcast_ref::<IntValue>();
                //         let error_message = format!(
                //             "Tried to divert to a target from a variable, but the variable ({}) didn't contain a divert target, it ",
                //             var_name
                //         );
                //         let error_message = if let Some(int_content) = int_content {
                //             if int_content.value == 0 {
                //                 format!("{}was empty/null (the value 0).", error_message)
                //             } else {
                //                 format!("{}contained '{}'.", error_message, var_contents)
                //             }
                //         } else {
                //             error_message
                //         };

                //         error(error_message);
                //     }
                // } else {
                //     error(format!(
                //         "Tried to divert using a target from a variable that could not be found ({})",
                //         var_name
                //     ));
                // }
            } else if current_divert.is_external {
                //call_external_function(&current_divert.get_target_path_string(), current_divert.get_external_args());
                return true;
            } else {
                self.state.as_mut().unwrap().set_diverted_pointer(current_divert.target_pointer.clone());
            }

            if current_divert.pushes_to_stack {
            //     self.state.as_ref().unwrap()
            //         .get_call_stack()
            //         .push(current_divert.stack_push_type, 0, state.get_output_stream().len());
            // 
            }

            if self.state.as_ref().unwrap().diverted_pointer.is_null() && !current_divert.is_external {
                // if let Some(source_name) = &current_divert.get_debug_metadata().source_name {
                //     error(format!("Divert target doesn't exist: {}", source_name));
                // } else {
                //     error(format!("Divert resolution failed: {:?}", current_divert));
                // }
            }

            return true;
        }

        if let Some(eval_command) = content_obj.as_ref().as_any().downcast_ref::<ControlCommand>() {
            match eval_command.command_type {
                crate::control_command::CommandType::NotSet => todo!(),
                crate::control_command::CommandType::EvalStart => {
                    assert!(!self.state.as_ref().unwrap().get_in_expression_evaluation(), "Already in expression evaluation?");
                    self.state.as_ref().unwrap().set_in_expression_evaluation(true);
                },
                crate::control_command::CommandType::EvalOutput => todo!(),
                crate::control_command::CommandType::EvalEnd => {
                    assert!(self.state.as_ref().unwrap().get_in_expression_evaluation(), "Not in expression evaluation mode");
                    self.state.as_ref().unwrap().set_in_expression_evaluation(false);
                },
                crate::control_command::CommandType::Duplicate => todo!(),
                crate::control_command::CommandType::PopEvaluatedValue => todo!(),
                crate::control_command::CommandType::PopFunction => todo!(),
                crate::control_command::CommandType::PopTunnel => todo!(),
                crate::control_command::CommandType::BeginString => {
                    self.state.as_mut().unwrap().push_to_output_stream(Some(content_obj.clone()));

                    assert!(self.state.as_ref().unwrap().get_in_expression_evaluation(),
                            "Expected to be in an expression when evaluating a string");
                            self.state.as_ref().unwrap().set_in_expression_evaluation(false);
                },
                crate::control_command::CommandType::EndString => {

                    // Since we're iterating backward through the content,
                    // build a stack so that when we build the string,
                    // it's in the right order
                    let mut content_stack_for_string: VecDeque<Rc<dyn RTObject>> = VecDeque::new();
                    let mut content_to_retain: VecDeque<Rc<dyn RTObject>> = VecDeque::new();

                    let mut output_count_consumed = 0;

                    for i in (0..self.state.as_ref().unwrap().get_output_stream().len()).rev() {
                        let obj = &self.state.as_ref().unwrap().get_output_stream()[i];
                        output_count_consumed += 1;

                        if let Some(command) = obj.as_ref().as_any().downcast_ref::<ControlCommand>() {
                            if command.command_type == CommandType::BeginString {
                                break;
                            }
                        }

                        if let Some(tag) = obj.as_ref().as_any().downcast_ref::<Tag>() {
                            content_to_retain.push_back(obj.clone());
                        }

                        if let Some(sv) = Value::get_string_value(obj.as_ref()) {
                            content_stack_for_string.push_back(obj.clone());
                        }
                    }

                    // Consume the content that was produced for this string
                    self.state.as_mut().unwrap().pop_from_output_stream(output_count_consumed);

                    // Rescue the tags that we want actually to keep on the output stack
                    // rather than consume as part of the string we're building.
                    // At the time of writing, this only applies to Tag objects generated
                    // by choices, which are pushed to the stack during string generation.
                    for rescued_tag in content_to_retain.iter() {
                        self.state.as_mut().unwrap().push_to_output_stream(Some(rescued_tag.clone()));
                    }

                    // Build string out of the content we collected
                    let mut sb = String::new();

                    while let Some(c) = content_stack_for_string.pop_back() {
                        sb.push_str(&c.to_string());
                    }

                    // Return to expression evaluation (from content mode)
                    self.state.as_ref().unwrap().set_in_expression_evaluation(true);
                    self.state.as_mut().unwrap().push_evaluation_stack(Rc::new(Value::new_string(&sb)));
                },
                crate::control_command::CommandType::NoOp => todo!(),
                crate::control_command::CommandType::ChoiceCount => todo!(),
                crate::control_command::CommandType::Turns => todo!(),
                crate::control_command::CommandType::TurnsSince => todo!(),
                crate::control_command::CommandType::ReadCount => todo!(),
                crate::control_command::CommandType::Random => todo!(),
                crate::control_command::CommandType::SeedRandom => todo!(),
                crate::control_command::CommandType::VisitIndex => todo!(),
                crate::control_command::CommandType::SequenceShuffleIndex => todo!(),
                crate::control_command::CommandType::StartThread => todo!(),
                crate::control_command::CommandType::Done => {
                   // We may exist in the context of the initial
                    // act of creating the thread, or in the context of
                    // evaluating the content.
                    if self.state.as_ref().unwrap().get_callstack().borrow().can_pop_thread() {
                        self.state.as_ref().unwrap().get_callstack().as_ref().borrow_mut().pop_thread();
                    }

                    // In normal flow - allow safe exit without warning
                    else {
                        self.state.as_mut().unwrap().set_did_safe_exit(true);

                        // Stop flow in current thread
                        self.state.as_ref().unwrap().set_current_pointer(pointer::NULL);
                    } 
                },
                crate::control_command::CommandType::End => todo!(),
                crate::control_command::CommandType::ListFromInt => todo!(),
                crate::control_command::CommandType::ListRange => todo!(),
                crate::control_command::CommandType::ListRandom => todo!(),
                crate::control_command::CommandType::BeginTag => todo!(),
                crate::control_command::CommandType::EndTag => todo!(),
            }
        }

        false
    }

    fn next_content(&mut self) {
        // Setting previousContentObject is critical for
        // VisitChangedContainersDueToDivert
        let cp = self.state.as_ref().unwrap().get_current_pointer();
        self.state.as_mut().unwrap().set_previous_pointer(cp);

        // Divert step?

        // TODO
        // if !self.state.as_ref().unwrap().get_diverted_pointer().is_null() {

        //     self.state.as_mut().unwrap().setCurrentPointer(state.getDivertedPointer());
        //     self.state.as_mut().unwrap().setDivertedPointer(Pointer.Null);

        //     // Internally uses state.previousContentObject and
        //     // state.currentContentObject
        //     self.visitChangedContainersDueToDivert();

        //     // Diverted location has valid content?
        //     if !self.state.as_ref().unwrap().get_current_pointer().is_null() {
        //         return;
        //     }

        //     // Otherwise, if diverted location doesn't have valid content,
        //     // drop down and attempt to increment.
        //     // This can happen if the diverted path is intentionally jumping
        //     // to the end of a container - e.g. a Conditional that's re-joining
        // }

        let successful_pointer_increment = self.increment_content_pointer();

        // Ran out of content? Try to auto-exit from a function,
        // or finish evaluating the content of a thread
        if !successful_pointer_increment {

            let mut didPop = false;

            if self.state.as_ref().unwrap().get_callstack().as_ref().borrow().can_pop_type(PushPopType::Function) {

                // Pop from the call stack
                self.state.as_mut().unwrap().pop_callstack(PushPopType::Function);

                // This pop was due to dropping off the end of a function that
                // didn't return anything,
                // so in this case, we make sure that the evaluator has
                // something to chomp on if it needs it
                if self.state.as_ref().unwrap().get_in_expression_evaluation() {
                    self.state.as_mut().unwrap().push_evaluation_stack(Void::new());
                }

                didPop = true;
            } else if (self.state.as_ref().unwrap().get_callstack().as_ref().borrow().can_pop_thread()) {
                self.state.as_ref().unwrap().get_callstack().as_ref().borrow_mut().pop_thread();

                didPop = true;
            } else {
                self.state.as_mut().unwrap().try_exit_function_evaluation_from_game();
            }

            // Step past the point where we last called out
            if didPop && !self.state.as_ref().unwrap().get_current_pointer().is_null() {
                self.next_content();
            }
        }     
    }

    fn increment_content_pointer(&self) -> bool {
        let mut successful_increment = true;

        let mut pointer = self.state.as_ref().unwrap().get_callstack().as_ref().borrow().get_current_element().current_pointer.clone();
        pointer.index += 1;

        let container= pointer.container.as_ref().unwrap().clone();

        // Each time we step off the end, we fall out to the next container, all
        // the
        // while we're in indexed rather than named content
        while pointer.index >= container.content.len() as i32 {

            successful_increment = false;

            let next_ancestor = container.get_object().get_parent();

            if next_ancestor.is_none() {
                break;
            }

            let container: Rc<dyn RTObject> = container.clone();
            let index_in_ancestor = next_ancestor.as_ref().unwrap().content.iter().position(|s| Rc::ptr_eq(s, &container));
            if index_in_ancestor.is_none() {
                break;
            }

            pointer = Pointer::new(next_ancestor, index_in_ancestor.unwrap() as i32);

            // Increment to next content in outer container
            pointer.index += 1;

            successful_increment = true;
        }

        if !successful_increment {
            pointer = pointer::NULL.clone();
        }

        self.state.as_ref().unwrap().get_callstack().as_ref().borrow_mut().get_current_element_mut().current_pointer = pointer;

        return successful_increment;
    }

    fn choose_path(&self, target_path: &Path, arg: bool) {
        todo!()
    }

    pub fn get_current_choices(&self) -> Vec<Rc<Choice>> {
        // Don't include invisible choices for external usage.
        let mut choices = Vec::new();

        if let Some(current_choices) = self.state.as_ref().unwrap().get_current_choices() {
            for c in current_choices {
                if !c.is_invisible_default {
                    c.index.replace(choices.len());
                    choices.push(c.clone());
                }
            }
        }

        choices
    }

    pub fn has_error(&self) -> bool {
        self.state.as_ref().unwrap().has_error()
    }

    pub fn get_current_errors(&self) -> &Vec<String> {
        self.state.as_ref().unwrap().get_current_errors()
    }

    pub fn choose_choice_index(&self, choice_list_index: usize) {
        todo!()
    }

    fn is_truthy(&self, obj: Rc<dyn RTObject>) -> bool {
        let truthy = false;
        
        if let Ok(val) = obj.into_any().downcast::<Value>() {
            // TODO

            // if (val instanceof DivertTargetValue) {
            //     DivertTargetValue divTarget = (DivertTargetValue) val;
            //     error("Shouldn't use a divert target (to " + divTarget.getTargetPath()
            //             + ") as a conditional value. Did you intend a function call 'likeThis()' or a read count "
            //             + "check 'likeThis'? (no arrows)");
            //     return false;
            // }

            return val.is_truthy();
        }

        return truthy;
    }

    fn process_choice(&mut self, choice_point: &Rc<ChoicePoint>) -> Option<Rc<Choice>> {
        let mut showChoice = true;

        // Don't create choice if choice point doesn't pass conditional
        if choice_point.has_condition() {
            let condition_value = self.state.as_mut().unwrap().pop_evaluation_stack();
            if !self.is_truthy(condition_value) {
                showChoice = false;
            }
        }

        let mut start_text = String::new();
        let mut choice_only_text = String::new();
        let mut tags: Vec<String> = Vec::with_capacity(0);

        if choice_point.has_choice_only_content() {
            choice_only_text = self.pop_choice_string_and_tags(&mut tags);
        }

        if choice_point.has_start_content() {
            start_text = self.pop_choice_string_and_tags(&mut tags);
        }

        // Don't create choice if player has already read this content
        if choice_point.once_only() {
            //TODO
            // let visitCount = state.visitCountForContainer(choicePoint.getChoiceTarget());
            // if (visitCount > 0) {
            //     showChoice = false;
            // }
        }

        // We go through the full process of creating the choice above so
        // that we consume the content for it, since otherwise it'll
        // be shown on the output stream.
        if !showChoice {
            return None;
        }

        start_text.push_str(&choice_only_text);

        let choice = Rc::new(Choice::new(choice_point.get_path_on_choice(), Object::get_path(choice_point.clone()).to_string(), choice_point.is_invisible_default(), tags, self.state.as_ref().unwrap().get_callstack().borrow_mut().fork_thread(), start_text.trim().to_string(), 0, 0));

        Some(choice)
    }

    fn pop_choice_string_and_tags(&mut self, tags: &[String]) -> String {
        let obj = self.state.as_mut().unwrap().pop_evaluation_stack();
        let choiceOnlyStrVal = Value::get_string_value(obj.as_ref()).unwrap();

        // TODO

        // while (self.state.as_ref().unwrap().evaluation_stack.len() > 0 && self.state.as_ref().unwrap().peek_evaluation_stack() instanceof Tag) {
        //     Tag tag = (Tag) state.popEvaluationStack();
        //     tags.add(0, tag.getText()); // popped in reverse order
        // }

        return choiceOnlyStrVal.string.to_string();
    }
}

