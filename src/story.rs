#![allow(unused_variables, dead_code)]

use std::{rc::Rc, time::Instant, borrow::BorrowMut};

use crate::{
    container::{Container},
    error::{ErrorType},
    json_serialization,
    push_pop::PushPopType,
    story_state::StoryState, pointer::{Pointer, self}, object::RTObject, void::Void,
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

        let main_content_container = json_serialization::jtoken_to_runtime_object(root_token)?;

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

        self.state = Some(StoryState::new(self));

        // TODO state.getVariablesState().setVariableChangedEvent(this);

        self.reset_globals();
    }

    fn reset_globals(&self) {
        /* TODO
        if (mainContentContainer.getNamedContent().containsKey("global decl")) {
            final Pointer originalPointer = new Pointer(state.getCurrentPointer());

            choosePath(new Path("global decl"), false);

            // Continue, but without validating external bindings,
            // since we may be doing this reset at initialisation time.
            continueInternal();

            state.setCurrentPointer(originalPointer);
        }

        state.getVariablesState().snapshotDefaultGlobals();
        */
    }

    pub fn build_string_of_hierarchy(&self) -> String {
        let mut sb = String::new();

        self.main_content_container
            .build_string_of_hierarchy(&mut sb, 0, None); // TODO state.getCurrentPointer().resolve());

        sb
    }

    pub fn can_continue(&self) -> bool {
        self.state.as_ref().unwrap().can_continue()
    }

    pub fn cont(&mut self) -> String {
        self.continue_async(0.0);
        self.get_current_text()
    }

    pub fn continue_async(&mut self, millisecs_limit_async: f32) {
        // TODO: if (!hasValidatedExternals) validateExternalBindings();

        self.continue_internal(millisecs_limit_async);
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
            if let Some(state_snapshot_at_last_new_line) = self.state_snapshot_at_last_new_line.as_ref() {

                // Has proper text or a tag been added? Then we know that the newline
                // that was previously added is definitely the end of the line.
                let change = self.calculate_newline_output_state_change(
                        state_snapshot_at_last_new_line.get_current_text(), self.state.as_ref().unwrap().get_current_text(),
                        state_snapshot_at_last_new_line.get_current_tags().len(),
                        self.state.as_ref().unwrap().get_current_tags().len());

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
                    if self.state_snapshot_at_last_new_line.is_some() {
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

    pub fn get_current_text(&self) -> String {
        todo!()
    }

    pub(crate) fn get_main_content_container(&self) -> Rc<Container> {
        match self.temporaty_evaluation_container.as_ref() {
            Some(c) => c.clone(),
            None => self.main_content_container.clone(),
        }
    }

    fn restore_state_snapshot(&self) {
        todo!()
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
        let current_content_obj = pointer.resolve();
        let is_logic_or_flow_control = self.perform_logic_and_flow_control(&current_content_obj);

        // Has flow been forced to end by flow control above?
        if self.state.as_ref().unwrap().get_current_pointer().is_null() {
            return;
        }

        if is_logic_or_flow_control {
            should_add_to_stream = false;
        }

        // Choice with condition?
        // TODO
        // ChoicePoint choicePoint = currentContentObj instanceof ChoicePoint ? (ChoicePoint) currentContentObj : null;

        // if (choicePoint != null) {
        //     Choice choice = processChoice(choicePoint);
        //     if (choice != null) {
        //         state.getGeneratedChoices().add(choice);
        //     }

        //     currentContentObj = null;
        //     should_add_to_stream = false;
        // }

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
                self.state.as_mut().unwrap().push_evaluation_stack(current_content_obj);
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
        todo!()
    }

    fn calculate_newline_output_state_change(&self, get_current_text_1: String, get_current_text_2: String, len_1: usize, len_2: usize) -> OutputStateChange {
        todo!()
    }

    fn state_snapshot(&self) {
        todo!()
    }

    fn discard_snapshot(&self) {
        todo!()
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

    fn perform_logic_and_flow_control(&self, current_content_obj: &Option<Rc<dyn RTObject>>) -> bool {
        match current_content_obj {
            Some(current_content_obj) => {
                // TODO
                return false;
            },
            None => return false,
        }
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
                    self.state.as_mut().unwrap().push_evaluation_stack(Some(Void::new()));
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
}


#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn oneline_test() {
        let json_string =
            fs::read_to_string("examples/inkfiles/basictext/oneline.ink.json").unwrap();
        let mut story = Story::new(&json_string).unwrap();
        println!("{}", story.build_string_of_hierarchy());

        assert!(story.can_continue());
        let line = story.cont();
        println!("{}", line);
        assert_eq!("Line.", line);
        assert!(!story.can_continue());
    }

    #[test]
    fn twolines_test() {
        let json_string =
            fs::read_to_string("examples/inkfiles/basictext/twolines.ink.json").unwrap();
        let story = Story::new(&json_string).unwrap();
        println!("{}", story.build_string_of_hierarchy());
    }

    fn next_all(story: &mut Story, text: &mut Vec<String>) {
        while story.can_continue() {
            let line = story.cont();
            print!("{line}");

            if !line.trim().is_empty() {
                text.push(line.trim().to_string());
            }
        }

        /* TODO
        if story.has_error() {
            fail(TestUtils.joinText(story.getCurrentErrors()));
        }
        */
    }
}
