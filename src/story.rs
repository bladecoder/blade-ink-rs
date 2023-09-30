#![allow(unused_variables, dead_code)]

use std::{rc::Rc, time::Instant, collections::{VecDeque, HashMap}};

use rand::{Rng, rngs::StdRng, SeedableRng};

use crate::{
    container::Container,
    error::ErrorType,
    json_read,
    push_pop::PushPopType,
    story_state::StoryState, pointer::{Pointer, self}, object::{RTObject, Object}, void::Void, path::Path, control_command::{ControlCommand, CommandType}, choice::Choice, value::Value, tag::Tag, divert::Divert, choice_point::ChoicePoint, search_result::SearchResult, variable_assigment::VariableAssignment, native_function_call::NativeFunctionCall, variable_reference::VariableReference, list_definitions_origin::ListDefinitionsOrigin, ink_list::InkList, ink_list_item::InkListItem, variables_state::VariablesState,
};

pub const INK_VERSION_CURRENT: i32 = 21;
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
    prev_containers: Vec<Rc<Container>>,
    list_definitions: Rc<ListDefinitionsOrigin>,
}

impl Story {
    pub fn new(json_string: &str) -> Result<Self, String> {
        let json: serde_json::Value = match serde_json::from_str(json_string) {
            Ok(value) => value,
            Err(_) => return Err("Story not in JSON format.".to_owned()),
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
            return Err("Version of ink used to build story was newer than the current version of the engine".to_owned());
        } else if version < INK_VERSION_MINIMUM_COMPATIBLE {
            return Err("Version of ink used to build story is too old to be loaded by this version of the engine".to_owned());
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

        let list_definitions = match json.get("listDefs") {
            Some(def) => Rc::new(json_read::jtoken_to_list_definitions(def)?),
            None => {
                return Err(
                    "List Definitions node for ink not found. Are you sure it's a valid .ink.json file?"
                        .to_string(),
                )
            }
        };

        let main_content_container = json_read::jtoken_to_runtime_object(root_token, None)?;

        let main_content_container = main_content_container.into_any().downcast::<Container>();

        if main_content_container.is_err() {
            return Err("Root node for ink is not a container?".to_owned());
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
            prev_containers: Vec::new(),
            list_definitions,
        };

        story.reset_state();

        Ok(story)
    }

    pub fn get_state(&self) -> &StoryState {
        self.state.as_ref().unwrap()
    }

    fn get_state_mut(&mut self) -> &mut StoryState {
        self.state.as_mut().unwrap()
    }

    fn reset_state(&mut self) {
        self.if_async_we_cant("ResetState");

        self.state = Some(StoryState::new(self.main_content_container.clone(), self.list_definitions.clone()));

        // TODO self.get_state_mut().get_variables_state().setVariableChangedEvent(this);

        self.reset_globals();
    }

    fn reset_globals(&mut self) {
        if self.main_content_container.named_content.contains_key("global decl") {
            let original_pointer = self.get_state().get_current_pointer().clone();

            self.choose_path(&Path::new_with_components_string(Some("global decl")), false);

            // Continue, but without validating external bindings,
            // since we may be doing this reset at initialisation time.
            self.continue_internal(0.0);

            self.get_state().set_current_pointer(original_pointer);
        }

        self.get_state_mut().get_variables_state_mut().snapshot_default_globals();
    }

    pub fn build_string_of_hierarchy(&self) -> String {
        let mut sb = String::new();

        let cp = self.get_state().get_current_pointer().resolve();

        let cp = match cp {
            Some(_) => Some(cp.as_ref().unwrap().as_ref()),
            None => None,
        };

        self.main_content_container
            .build_string_of_hierarchy(&mut sb, 0, cp);

        sb
    }

    pub fn can_continue(&self) -> bool {
        self.get_state().can_continue()
    }

    pub fn cont(&mut self) -> Result<String, String> {
        self.continue_async(0.0)?;
        Ok(self.get_current_text())
    }

    pub fn continue_maximally(&mut self) -> Result<String, String> {
        self.if_async_we_cant("continue_maximally");

        let mut sb = String::new();

        while self.can_continue() {
            sb.push_str(&self.cont()?);
        }

        Ok(sb)
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
            if !self.can_continue() {
                return Err(
                    "Can't continue - should check can_continue before calling Continue".to_owned(),
                );
            }

            self.get_state_mut().set_did_safe_exit(false);

            self.get_state_mut().reset_output(None);

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

            //println!("{}", self.build_string_of_hierarchy());

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
                    && !self.get_state().is_did_safe_exit()
                    && self.temporaty_evaluation_container.is_none()
                {
                    if self
                        .state
                        .as_ref()
                        .unwrap()
                        .get_callstack()
                        .borrow()
                        .can_pop_type(Some(PushPopType::Tunnel))
                    {
                        self.add_error("unexpectedly reached end of content. Do you need a '->->' to return from a tunnel?");
                    } else if self
                        .state
                        .as_ref()
                        .unwrap()
                        .get_callstack()
                        .borrow()
                        .can_pop_type(Some(PushPopType::Function))
                    {
                        self.add_error(
                            "unexpectedly reached end of content. Do you need a '~ return'?",
                        );
                    } else if !self.get_state().get_callstack().borrow().can_pop() {
                        self.add_error("ran out of content. Do you need a '-> DONE' or '-> END'?");
                    } else {
                        self.add_error("unexpectedly reached end of content for unknown reason. Please debug compiler!");
                    }
                }
            }
            self.get_state_mut().set_did_safe_exit(false);
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
        if self.get_state().has_error() || self.get_state().has_warning() {
            match self.on_error {
                Some(on_err) => {
                    if self.get_state().has_error() {
                        for err in self.get_state().get_current_errors() {
                            (on_err)(&err, ErrorType::Error);
                        }
                    }

                    if self.get_state().has_warning() {
                        for err in self.get_state().get_current_warnings() {
                            (on_err)(&err, ErrorType::Warning);
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
                        sb.push_str(self.get_state().get_current_warnings().len().to_string().as_str());
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
                        sb.push_str(self.get_state().get_current_warnings()[0].to_string().as_str());
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
        if !self.can_continue() && !self.get_state().get_callstack().borrow().element_is_evaluate_from_game() {
            self.try_follow_default_invisible_choice();
        }

        // Don't save/rewind during string evaluation, which is e.g. used for choices
        if !self.get_state().in_string_evaluation() {

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
            if self.get_state().output_stream_ends_in_newline() {

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

        Ok(false)
    }

    pub fn get_current_text(&mut self) -> String {
        self.if_async_we_cant("call currentText since it's a work in progress");
        self.get_state_mut().get_current_text()
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
            self.get_state_mut().apply_any_patch();
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
        let mut pointer = self.get_state().get_current_pointer().clone();

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

        self.get_state_mut().set_current_pointer(pointer.clone());

        // Is the current content Object:
        // - Normal content
        // - Or a logic/flow statement - if so, do it
        // Stop flow if we hit a stack pop when we're unable to pop (e.g.
        // return/done statement in knot
        // that was diverted to rather than called as a function)
        let mut current_content_obj = pointer.resolve();

        let is_logic_or_flow_control = self.perform_logic_and_flow_control(&current_content_obj);

        // Has flow been forced to end by flow control above?
        if self.get_state().get_current_pointer().is_null() {
            return;
        }

        if is_logic_or_flow_control {
            should_add_to_stream = false;
        }

        // Choice with condition?
        if current_content_obj.is_some() {
                if let Ok(choice_point) =  current_content_obj.clone().unwrap().into_any().downcast::<ChoicePoint>() {

                let choice = self.process_choice(&choice_point);
                if choice.is_some() {
                    self.get_state_mut().get_generated_choices_mut().push(choice.unwrap());
                }

                current_content_obj = None;
                should_add_to_stream = false;
            }
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
            let var_pointer =
                Value::get_variable_pointer_value(current_content_obj.as_ref().unwrap().as_ref());

            if var_pointer.is_some() && var_pointer.unwrap().context_index == -1 {

                // Create new Object so we're not overwriting the story's own
                // data
                let context_idx = self.get_state().get_callstack().borrow().context_for_variable_named(&var_pointer.unwrap().variable_name);
                current_content_obj = Some(Rc::new(Value::new_variable_pointer(&var_pointer.unwrap().variable_name, context_idx as i32)));
            }

            // Expression evaluation content
            if self.get_state().get_in_expression_evaluation() {
                self.get_state_mut().push_evaluation_stack(current_content_obj.as_ref().unwrap().clone());
            }
            // Output stream content (i.e. not expression evaluation)
            else {
                self.get_state_mut().push_to_output_stream(current_content_obj.as_ref().unwrap().clone());
            }
        }

        // Increment the content pointer, following diverts if necessary
        self.next_content();

        // Starting a thread should be done after the increment to the content
        // pointer,
        // so that when returning from the thread, it returns to the content
        // after this instruction.
        if current_content_obj.is_some() {
            if let Some(control_cmd) = current_content_obj.as_ref().unwrap().as_any().downcast_ref::<ControlCommand>() {
                if control_cmd.command_type == CommandType::StartThread {
                    self.get_state().get_callstack().borrow_mut().push_thread();
                }
            }
        }

    }

    fn try_follow_default_invisible_choice(&mut self) {
        let all_choices = match self.get_state().get_current_choices() {
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
        self.get_state().get_callstack().as_ref().borrow_mut().set_current_thread(choice.get_thread_at_generation().unwrap().copy());

        // If there's a chance that this state will be rolled back to before
        // the invisible choice then make sure that the choice thread is
        // left intact, and it isn't re-entered in an old state.
        if self.state_snapshot_at_last_new_line.is_some() {
            let fork_thread = self.get_state().get_callstack().as_ref().borrow_mut().fork_thread();
            self.get_state().get_callstack().as_ref().borrow_mut().set_current_thread(fork_thread);
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
        
        if !self.async_saving {
            self.get_state_mut().apply_any_patch();
        }

        // No longer need the snapshot.
        self.state_snapshot_at_last_new_line = None;    
    }

    fn visit_container(&mut self, container: &Rc<Container>, at_start: bool) {
        if !container.counting_at_start_only || at_start {
            if container.visits_should_be_counted {
                self.get_state_mut().increment_visit_count_for_container(container);
            }

            if container.turn_index_should_be_counted {
                self.get_state_mut().record_turn_index_visit_to_container(container);
            }
        }
    }

    pub fn get_variables_state(&self) -> &VariablesState {
        self.get_state().get_variables_state()
    }

    pub fn get_variables_state_mut(&mut self) -> &mut VariablesState {
        self.get_state_mut().get_variables_state_mut()
    }

    fn perform_logic_and_flow_control(&mut self, content_obj: &Option<Rc<dyn RTObject>>) -> bool {
        let content_obj = match content_obj {
            Some(content_obj) => {
                content_obj.clone()
            },
            None => return false,
        };

        // Divert
        if let Ok(current_divert) = content_obj.clone().into_any().downcast::<Divert>() {
            if current_divert.is_conditional {
                let o = self.get_state_mut().pop_evaluation_stack();
                if !self.is_truthy(o) {
                    return true;
                }
            }

            if current_divert.has_variable_target() {
                let var_name = &current_divert.variable_divert_name;
                if let Some(var_contents) = self.get_state().get_variables_state().get_variable_with_name(var_name.as_ref().unwrap(), -1) {
                    if let Some(target) = Value::get_divert_target_value(var_contents.as_ref()) {
                        let p = Self::pointer_at_path(&self.main_content_container, target);
                        self.get_state_mut().set_diverted_pointer(p);
                    } else {
                        // TODO
                        // let int_content = var_contents.downcast_ref::<IntValue>();
                        // let error_message = format!(
                        //     "Tried to divert to a target from a variable, but the variable ({}) didn't contain a divert target, it ",
                        //     var_name
                        // );
                        // let error_message = if let Some(int_content) = int_content {
                        //     if int_content.value == 0 {
                        //         format!("{}was empty/null (the value 0).", error_message)
                        //     } else {
                        //         format!("{}contained '{}'.", error_message, var_contents)
                        //     }
                        // } else {
                        //     error_message
                        // };

                        // error(error_message);
                        panic!();
                    }
                } else {
                    // TODO
                    // error(format!(
                    //     "Tried to divert using a target from a variable that could not be found ({})",
                    //     var_name
                    // ));
                    panic!();
                }
            } else if current_divert.is_external {
                //call_external_function(&current_divert.get_target_path_string(), current_divert.get_external_args());
                return true;
            } else {
                self.get_state_mut().set_diverted_pointer(current_divert.get_target_pointer());
            }

            if current_divert.pushes_to_stack {
                self.get_state()
                    .get_callstack().borrow_mut()
                    .push(current_divert.stack_push_type, 0, self.get_state().get_output_stream().len() as i32);
            }

            if self.get_state().diverted_pointer.is_null() && !current_divert.is_external {
                //     error(format!("Divert resolution failed: {:?}", current_divert));
            }

            return true;
        }

        if let Some(eval_command) = content_obj.as_ref().as_any().downcast_ref::<ControlCommand>() {
            match eval_command.command_type {
                CommandType::EvalStart => {
                    assert!(!self.get_state().get_in_expression_evaluation(), "Already in expression evaluation?");
                    self.get_state().set_in_expression_evaluation(true);
                },
                CommandType::EvalOutput => {
                    // If the expression turned out to be empty, there may not be
                    // anything on the stack
                    if self.get_state().evaluation_stack.len() > 0 {

                        let output = self.get_state_mut().pop_evaluation_stack();

                        // Functions may evaluate to Void, in which case we skip
                        // output
                        if let None = output.as_ref().as_any().downcast_ref::<Void>() {
                            // TODO: Should we really always blanket convert to
                            // string?
                            // It would be okay to have numbers in the output stream
                            // the
                            // only problem is when exporting text for viewing, it
                            // skips over numbers etc.
                            let text:Rc<dyn RTObject> = Rc::new(Value::new_string(&output.to_string()));

                            self.get_state_mut().push_to_output_stream(text);
                        }
                    }
                },
                CommandType::EvalEnd => {
                    assert!(self.get_state().get_in_expression_evaluation(), "Not in expression evaluation mode");
                    self.get_state().set_in_expression_evaluation(false);
                },
                CommandType::Duplicate => {
                    let obj = self.get_state().peek_evaluation_stack().unwrap().clone();
                    self.get_state_mut().push_evaluation_stack(obj);
                },
                CommandType::PopEvaluatedValue => {
                    self.get_state_mut().pop_evaluation_stack();
                },
                CommandType::PopFunction | CommandType::PopTunnel=> {
                    let pop_type = if CommandType::PopFunction == eval_command.command_type  {
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

                        if override_tunnel_return_target.is_none() {
                            assert!(popped.as_ref().as_any().downcast_ref::<Void>().is_some(), "Expected void if ->-> doesn't override target");
                        }
                    }

                    if self.get_state_mut().try_exit_function_evaluation_from_game() {
                        return true;
                    } else if self.get_state().get_callstack().borrow().get_current_element().push_pop_type != pop_type
                            || !self.get_state().get_callstack().borrow().can_pop() {

                        let mut names: HashMap<PushPopType, String>   = HashMap::new();
                        names.insert(PushPopType::Function, "function return statement (~ return)".to_owned());
                        names.insert(PushPopType::Tunnel, "tunnel onwards statement (->->)".to_owned());

                        let mut expected = names.get(&self.get_state().get_callstack().borrow().get_current_element().push_pop_type).cloned();
                        if !self.get_state().get_callstack().borrow().can_pop() {
                            expected = Some("end of flow (-> END or choice)".to_owned());
                        }

                        panic!("Found {}, when expected {}", names.get(&pop_type).unwrap(), expected.unwrap());
                        //TODO error(errorMsg);
                    } else {
                        self.get_state_mut().pop_callstack(None);

                        // Does tunnel onwards override by diverting to a new ->->
                        // target?
                        if let Some(override_tunnel_return_target) = override_tunnel_return_target {
                            let p = Self::pointer_at_path(&self.main_content_container, &override_tunnel_return_target);
                            self.get_state_mut().set_diverted_pointer(p);
                        }
                    }                    
                },
                CommandType::BeginString => {
                    self.get_state_mut().push_to_output_stream(content_obj.clone());

                    assert!(self.get_state().get_in_expression_evaluation(),
                            "Expected to be in an expression when evaluating a string");
                            self.get_state().set_in_expression_evaluation(false);
                },
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
                    self.get_state_mut().pop_from_output_stream(output_count_consumed);

                    // Rescue the tags that we want actually to keep on the output stack
                    // rather than consume as part of the string we're building.
                    // At the time of writing, this only applies to Tag objects generated
                    // by choices, which are pushed to the stack during string generation.
                    for rescued_tag in content_to_retain.iter() {
                        self.get_state_mut().push_to_output_stream(rescued_tag.clone());
                    }

                    // Build string out of the content we collected
                    let mut sb = String::new();

                    while let Some(c) = content_stack_for_string.pop_back() {
                        sb.push_str(&c.to_string());
                    }

                    // Return to expression evaluation (from content mode)
                    self.get_state().set_in_expression_evaluation(true);
                    self.get_state_mut().push_evaluation_stack(Rc::new(Value::new_string(&sb)));
                },
                CommandType::NoOp => {},
                CommandType::ChoiceCount => {
                    let choice_count = self.get_state().get_generated_choices().len();
                    self.get_state_mut().push_evaluation_stack(Rc::new(Value::new_int(choice_count as i32)));
                },
                CommandType::Turns => {
                    let current_turn = self.get_state().current_turn_index;
                    self.get_state_mut().push_evaluation_stack(Rc::new(Value::new_int( current_turn + 1)));
                },
                CommandType::TurnsSince |  CommandType::ReadCount => {
                    let target = self.get_state_mut().pop_evaluation_stack();
                    if Value::get_divert_target_value(target.as_ref()).is_none() {
                        let extra_note = "";
                        if Value::get_int_value(target.as_ref()).is_some() {
                        //     extraNote = ". Did you accidentally pass a read count ('knot_name') instead of a target "
                        //             + "('-> knot_name')?";
                        }
                        // error("TURNS_SINCE expected a divert target (knot, stitch, label name), but saw " + target
                        //         + extra_note);
                        panic!();
                    }

                    let target = Value::get_divert_target_value(target.as_ref()).unwrap();

                    let otmp = self.content_at_path(target).correct_obj();
                    let container = match &otmp {
                        Some(o) => o.clone().into_any().downcast::<Container>().ok(),
                        None => None,
                    };

                    let mut either_count = 0;

                    match container {
                        Some(container) => {
                            if eval_command.command_type == CommandType::TurnsSince {
                                either_count = self.get_state().turns_since_for_container(container.as_ref());
                            } else {either_count = self.get_state_mut().visit_count_for_container(&container) as i32;}
                        },
                        None => {
                            if eval_command.command_type == CommandType::TurnsSince {
                                either_count = -1; // turn count, default to never/unknown
                            } else { either_count = 0; } // visit count, assume 0 to default to allowing entry

                            // warning("Failed to find container for " + evalCommand.toString() + " lookup at "
                            //         + divertTarget.getTargetPath().toString());
                            panic!()
                        }
                    }

                    self.get_state_mut().push_evaluation_stack(Rc::new(Value::new_int(either_count)));
                },
                CommandType::Random => {
                    let mut max_int = None;
                    let o  = self.get_state_mut().pop_evaluation_stack();

                    if let Some(v) = Value::get_int_value(o.as_ref()) {
                        max_int = Some(v);
                    }

                    let o  = self.get_state_mut().pop_evaluation_stack();

                    let mut min_int = None;
                    if let Some(v) = Value::get_int_value(o.as_ref()) {
                        min_int = Some(v);
                    }

                    if min_int.is_none() {
                        panic!("Invalid value for the minimum parameter of RANDOM(min, max)");
                    }

                    if max_int.is_none() {
                        panic!("Invalid value for the maximum parameter of RANDOM(min, max)");
                    }

                    let min_value = min_int.unwrap();
                    let max_value = max_int.unwrap();

                    let random_range = max_value - min_value + 1;

                    if random_range <= 0 {
                        // TODO
                        // panic!(format!(
                        //     "RANDOM was called with minimum as {} and maximum as {}. The maximum must be larger",
                        //     min_value.to_string(), max_value.to_string()
                        // ));

                        panic!();
                    }

                    let result_seed = self.get_state().story_seed + self.get_state().previous_random;

                    let mut rng = StdRng::seed_from_u64(result_seed as u64);
                    let next_random = rng.gen::<u32>();

                    let chosen_value = (next_random % random_range as u32) as i32 + min_value;
                    
                    self.get_state_mut().push_evaluation_stack(Rc::new(Value::new_int(chosen_value)));

                    self.get_state_mut().previous_random = self.get_state().previous_random + 1;
                },
                CommandType::SeedRandom => {
                    let mut seed: Option<i32> = None;

                    let o = self.get_state_mut().pop_evaluation_stack();

                    if let Some(v) = Value::get_int_value(o.as_ref()) {seed = Some(v);}

                    if seed.is_none() {
                        panic!("Invalid value passed to SEED_RANDOM");
                    }

                    // Story seed affects both RANDOM and shuffle behaviour
                    self.get_state_mut().story_seed = seed.unwrap();
                    self.get_state_mut().previous_random = 0;

                    // SEED_RANDOM returns nothing.
                    self.get_state_mut().push_evaluation_stack(Rc::new(Void::new()));
                },
                CommandType::VisitIndex => {
                    let cpc = self.get_state().get_current_pointer().container.unwrap();
                    let count = self.get_state_mut().visit_count_for_container(&cpc) - 1; // index
                    // not
                    // count
                    self.get_state_mut().push_evaluation_stack(Rc::new(Value::new_int(count as i32)));
                },
                CommandType::SequenceShuffleIndex => {
                    let shuffle_index = self.next_sequence_shuffle_index();
                    let v = Rc::new(Value::new_int(shuffle_index));
                    self.get_state_mut().push_evaluation_stack(v);
                },
                CommandType::StartThread => {
                    // Handled in main step function
                },
                CommandType::Done => {
                   // We may exist in the context of the initial
                    // act of creating the thread, or in the context of
                    // evaluating the content.
                    if self.get_state().get_callstack().borrow().can_pop_thread() {
                        self.get_state().get_callstack().as_ref().borrow_mut().pop_thread();
                    }

                    // In normal flow - allow safe exit without warning
                    else {
                        self.get_state_mut().set_did_safe_exit(true);

                        // Stop flow in current thread
                        self.get_state().set_current_pointer(pointer::NULL.clone());
                    } 
                },
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
                        panic!("Passed non-integer when creating a list element from a numerical value.");
                    }
                
                    let mut generated_list_value: Option<Value> = None;
                
                    if let Some(found_list_def) = self.list_definitions.as_ref().get_list_definition(&list_name_val.as_ref().unwrap()) {
                        if let Some(found_item) = found_list_def.get_item_with_value(int_val.unwrap()) {
                            let l = InkList::from_single_element((found_item.clone(), int_val.unwrap()));
                            generated_list_value = Some(Value::new_list(l));
                        }
                    } else {
                        //panic!(format!("Failed to find List called {}", list_name_val.as_ref().unwrap()));
                        panic!();
                    }
                
                    if generated_list_value.is_none() {
                        generated_list_value = Some(Value::new_list(InkList::new()));
                    }
                
                    self.get_state_mut().push_evaluation_stack(Rc::new(generated_list_value.unwrap()));
                
                },
                CommandType::ListRange => {
                    let mut p = self.get_state_mut().pop_evaluation_stack();
                    let max = p.into_any().downcast::<Value>();

                    p = self.get_state_mut().pop_evaluation_stack();
                    let min = p.into_any().downcast::<Value>();

                    p = self.get_state_mut().pop_evaluation_stack();
                    let target_list = Value::get_list_value(p.as_ref());

                    if target_list.is_none() || min.is_err() || max.is_err()
                        {panic!("Expected List, minimum and maximum for LIST_RANGE");}

                    let result = target_list.unwrap().list_with_sub_range(&min.unwrap().value, &max.unwrap().value);

                    self.get_state_mut().push_evaluation_stack(Rc::new(Value::new_list(result)));
                },
                CommandType::ListRandom => {
                    let o = self.get_state_mut().pop_evaluation_stack();
                    let list = Value::get_list_value(o.as_ref());

                    if list.is_none() {panic!("Expected list for LIST_RANDOM");}

                    let list = list.unwrap();

                    let new_list = {
                        // List was empty: return empty list
                        if list.items.is_empty() {
                            InkList::new()
                        }
                        // Non-empty source list
                        else {
                            // Generate a random index for the element to take
                            let result_seed = self.get_state().story_seed + self.get_state().previous_random;
                            let mut rng = StdRng::seed_from_u64(result_seed as u64);
                            let next_random = rng.gen::<u32>();
                            let list_item_index = (next_random as usize) % list.items.len();

                            // Iterate through to get the random element, sorted for predictibility
                            let mut sorted: Vec<(&InkListItem, &i32)> = list.items.iter().collect();
                            sorted.sort_by(|a, b| b.1.cmp(a.1));
                            let random_item = sorted[list_item_index];

                            // Origin list is simply the origin of the one element
                            let mut new_list = InkList::from_single_origin(random_item.0.get_origin_name().unwrap().clone(), self.list_definitions.as_ref());
                            new_list.items.insert(random_item.0.clone(), *random_item.1);
                            
                            self.get_state_mut().previous_random = next_random as i32;

                            new_list
                        }
                    };

                    self.get_state_mut().push_evaluation_stack(Rc::new(Value::new_list(new_list)));
                },
                CommandType::BeginTag => self.get_state_mut().push_to_output_stream(content_obj.clone()),
                CommandType::EndTag =>  {

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
                    //  - First, to generate the choice text. String evaluation
                    //    will be on, and the final string will be pushed to the
                    //    evaluation stack, ready to be popped to make a Choice
                    //    object.
                    //  - Second, when ink generates text after choosing the choice.
                    //    On this ocassion, it's not in string evaluation mode.
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

                            if let Some(command) = obj.as_ref().as_any().downcast_ref::<ControlCommand>() {
                                if command.command_type == CommandType::BeginTag {
                                    break;
                                } else {
                                    panic!("Unexpected ControlCommand while extracting tag from choice");
                                    //break;
                                }
                            }

                            if let Some(sv) = Value::get_string_value(obj.as_ref()) {
                                content_stack_for_tag.push(sv.string.clone());
                            }
                        }

                        // Consume the content that was produced for this string
                        self.get_state_mut().pop_from_output_stream(output_count_consumed);

                        let mut sb = String::new();
                        for str_val in &content_stack_for_tag {
                            sb.push_str(&str_val);
                        }

                        let choice_tag = Rc::new(Tag::new(&StoryState::clean_output_whitespace(&sb)));
                        // Pushing to the evaluation stack means it gets picked up
                        // when a Choice is generated from the next Choice Point.
                        self.get_state_mut().push_evaluation_stack(choice_tag);
                    }

                    // Otherwise! Simply push EndTag, so that in the output stream we
                    // have a structure of: [BeginTag, "the tag content", EndTag]
                    else {
                        self.get_state_mut().push_to_output_stream(content_obj.clone());
                    }
                },
            }

            return true;
        }

        // Variable assignment
        if let Some(var_ass) = content_obj.as_ref().as_any().downcast_ref::<VariableAssignment>() {
            let assigned_val = self.get_state_mut().pop_evaluation_stack();

            // When in temporary evaluation, don't create new variables purely
            // within
            // the temporary context, but attempt to create them globally
            // var prioritiseHigherInCallStack = _temporaryEvaluationContainer
            // != null;
            let assigned_val = assigned_val.into_any().downcast::<Value>().unwrap();
            self.get_state_mut().get_variables_state_mut().assign( var_ass, assigned_val);

            return true;
        }

        // Variable reference
        if let Ok(var_ref) = content_obj.clone().into_any().downcast::<VariableReference>() {
            let mut found_value: Option<Rc<Value>> = None;

            // Explicit read count value
            if let Some(p) = &var_ref.path_for_count {
                let container = var_ref.get_container_for_count();
                let count = self.get_state_mut().visit_count_for_container(container.as_ref().unwrap());
                found_value = Some(Rc::new(Value::new_int(count as i32)));
            }

            // Normal variable reference
            else {

                found_value = self.get_state().get_variables_state().get_variable_with_name(&var_ref.name, -1);

                if let None = found_value {
                    // TODO
                    // self.warning("Variable not found: '" + varRef.getName()
                    //         + "'. Using default value of 0 (false). This can happen with temporary variables if the "
                    //         + "declaration hasn't yet been hit. Globals are always given a default value on load if a "
                    //         + "value doesn't exist in the save state.");
                    
                    found_value = Some(Rc::new(Value::new_int(0)));
                }
            }

            self.get_state_mut().push_evaluation_stack(found_value.unwrap());

            return true;
        }

        // Native function call
        if let Some(func) = content_obj.as_ref().as_any().downcast_ref::<NativeFunctionCall>() {
            let func_params = self.get_state_mut().pop_evaluation_stack_multiple(func.get_number_of_parameters());

            let result = func.call(func_params);
            self.get_state_mut().push_evaluation_stack(result);

            return true;
        }
        

        false
    }

    fn next_content(&mut self) {
        // Setting previousContentObject is critical for
        // VisitChangedContainersDueToDivert
        let cp = self.get_state().get_current_pointer();
        self.get_state_mut().set_previous_pointer(cp);

        // Divert step?
        if !self.get_state().diverted_pointer.is_null() {
            let dp = self.get_state().diverted_pointer.clone();
            self.get_state_mut().set_current_pointer(dp);
            self.get_state_mut().set_diverted_pointer(pointer::NULL.clone());

            // Internally uses state.previousContentObject and
            // state.currentContentObject
            self.visit_changed_containers_due_to_divert();

            // Diverted location has valid content?
            if !self.get_state().get_current_pointer().is_null() {
                return;
            }

            // Otherwise, if diverted location doesn't have valid content,
            // drop down and attempt to increment.
            // This can happen if the diverted path is intentionally jumping
            // to the end of a container - e.g. a Conditional that's re-joining
        }

        let successful_pointer_increment = self.increment_content_pointer();

        // Ran out of content? Try to auto-exit from a function,
        // or finish evaluating the content of a thread
        if !successful_pointer_increment {

            let mut didPop = false;

            let can_pop_type = self.get_state().get_callstack().as_ref().borrow().can_pop_type(Some(PushPopType::Function));
            if can_pop_type {

                // Pop from the call stack
                self.get_state_mut().pop_callstack(Some(PushPopType::Function));

                // This pop was due to dropping off the end of a function that
                // didn't return anything,
                // so in this case, we make sure that the evaluator has
                // something to chomp on if it needs it
                if self.get_state().get_in_expression_evaluation() {
                    self.get_state_mut().push_evaluation_stack(Rc::new(Void::new()));
                }

                didPop = true;
            } else if self.get_state().get_callstack().as_ref().borrow().can_pop_thread() {
                self.get_state().get_callstack().as_ref().borrow_mut().pop_thread();

                didPop = true;
            } else {
                self.get_state_mut().try_exit_function_evaluation_from_game();
            }

            // Step past the point where we last called out
            if didPop && !self.get_state().get_current_pointer().is_null() {
                self.next_content();
            }
        }   
    }

    fn increment_content_pointer(&self) -> bool {
        let mut successful_increment = true;

        let mut pointer = self.get_state().get_callstack().as_ref().borrow().get_current_element().current_pointer.clone();
        pointer.index += 1;

        let mut container= pointer.container.as_ref().unwrap().clone();

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
            let index_in_ancestor = next_ancestor.as_ref().unwrap().content.iter().position(|s| Rc::ptr_eq(s, &rto));
            if index_in_ancestor.is_none() {
                break;
            }

            pointer = Pointer::new(next_ancestor, index_in_ancestor.unwrap() as i32);
            container= pointer.container.as_ref().unwrap().clone();

            // Increment to next content in outer container
            pointer.index += 1;

            successful_increment = true;
        }

        if !successful_increment {
            pointer = pointer::NULL.clone();
        }

        self.get_state().get_callstack().as_ref().borrow_mut().get_current_element_mut().current_pointer = pointer;

        return successful_increment;
    }

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

    pub fn has_error(&self) -> bool {
        self.get_state().has_error()
    }

    pub fn get_current_errors(&self) -> &Vec<String> {
        self.get_state().get_current_errors()
    }

    pub fn choose_choice_index(&mut self, choice_index: usize) {
        let choices = self.get_current_choices();
        //assert!(choice_index < choices.len(), "choice out of range");

        // Replace callstack with the one from the thread at the choosing point,
        // so that we can jump into the right place in the flow.
        // This is important in case the flow was forked by a new thread, which
        // can create multiple leading edges for the story, each of
        // which has its own context.
        let choice_to_choose = choices.get(choice_index).unwrap();
        self.get_state().get_callstack().borrow_mut().set_current_thread(choice_to_choose.get_thread_at_generation().unwrap());

        self.choose_path(&choice_to_choose.target_path, true);
    }

    fn choose_path(&mut self, p: &Path, incrementing_turn_index: bool) {
        self.get_state_mut().set_chosen_path( &p,  incrementing_turn_index);

        // Take a note of newly visited containers for read counts etc
        self.visit_changed_containers_due_to_divert();
    }

    fn is_truthy(&self, obj: Rc<dyn RTObject>) -> bool {
        let truthy = false;

        if let Some(val) = obj.as_ref().as_any().downcast_ref::<Value>() {
        
            if let Some(_) = Value::get_divert_target_value(obj.as_ref()) {
                // self.error("Shouldn't use a divert target (to " + divTarget.getTargetPath()
                //         + ") as a conditional value. Did you intend a function call 'likeThis()' or a read count "
                //         + "check 'likeThis'? (no arrows)");
                return false;
            }

            return val.is_truthy();
        }

        return truthy;
    }

    fn process_choice(&mut self, choice_point: &Rc<ChoicePoint>) -> Option<Rc<Choice>> {
        let mut show_choice = true;

        // Don't create choice if choice point doesn't pass conditional
        if choice_point.has_condition() {
            let condition_value = self.get_state_mut().pop_evaluation_stack();
            if !self.is_truthy(condition_value) {
                show_choice = false;
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
            let visit_count = self.get_state_mut().visit_count_for_container(choice_point.get_choice_target().as_ref().unwrap());
            if visit_count > 0 {
                show_choice = false;
            }
        }

        // We go through the full process of creating the choice above so
        // that we consume the content for it, since otherwise it'll
        // be shown on the output stream.
        if !show_choice {
            return None;
        }

        start_text.push_str(&choice_only_text);

        let choice = Rc::new(Choice::new(choice_point.get_path_on_choice(), Object::get_path(choice_point.as_ref()).to_string(), choice_point.is_invisible_default(), tags, self.get_state().get_callstack().borrow_mut().fork_thread(), start_text.trim().to_string(), 0, 0));

        Some(choice)
    }

    fn pop_choice_string_and_tags(&mut self, tags: &mut Vec<String>) -> String {
        let obj = self.get_state_mut().pop_evaluation_stack();
        let choice_only_str_val = Value::get_string_value(obj.as_ref()).unwrap();

        while self.get_state().evaluation_stack.len() > 0 && self.get_state().peek_evaluation_stack().unwrap().as_any().is::<Tag>() {
            let tag = self.get_state_mut().pop_evaluation_stack().into_any().downcast::<Tag>().unwrap();
            tags.insert(0, tag.get_text().clone()); // popped in reverse order
        }

        return choice_only_str_val.string.to_string();
    }

    pub fn pointer_at_path(main_content_container: &Rc<Container>, path: &Path) -> Pointer {
        if path.len() == 0 {
            return pointer::NULL.clone();
        }
    
        let mut p = Pointer::default();
        let mut path_length_to_use = path.len() as i32;
        
        
        let result: SearchResult = 
            if path.get_last_component().unwrap().is_index() {
                path_length_to_use -= 1;
                let result = SearchResult::from_search_result(&main_content_container.content_at_path(path, 0, path_length_to_use));
                p.container = result.get_container();
                p.index = path.get_last_component().unwrap().index.unwrap() as i32;

                result
            } else {
                let result = SearchResult::from_search_result(&main_content_container.content_at_path(path, 0, -1));
                p.container = result.get_container();
                p.index = -1;

                result
            };

        let main_container: Rc<dyn RTObject> = main_content_container.clone();
    
        if Rc::ptr_eq(&result.obj, &main_container) && path_length_to_use > 0 {
            // self.error(format!(
            //     "Failed to find content at path '{}', and no approximation of it was possible.",
            //     path
            // ));
            panic!()
        } else if result.approximate {
            // warning(format!(
            //     "Failed to find content at path '{}', so it was approximated to: '{}'.",
            //     path,
            //     result.obj.unwrap().get_path()
            // ));
        }
    
        p
    }

    fn visit_changed_containers_due_to_divert(&mut self) {
        let previous_pointer = self.get_state().get_previous_pointer();
        let pointer = self.get_state().get_current_pointer();
    
        // Unless we're pointing *directly* at a piece of content, we don't do counting here.
        // Otherwise, the main stepping function will do the counting.
        if pointer.is_null() || pointer.index == -1 {
            return;
        }
    
        // First, find the previously open set of containers
        self.prev_containers.clear();
    
        if !previous_pointer.is_null() {
            let mut prev_ancestor = None;
    
            let resolved = previous_pointer.resolve();
            if resolved.is_some() && resolved.as_ref().unwrap().as_any().is::<Container>() {
                prev_ancestor = resolved.unwrap().into_any().downcast::<Container>().ok();
            } else if previous_pointer.container.is_some() {
                prev_ancestor = previous_pointer.container.clone();
            }
    
            while let Some(prev_anc) = prev_ancestor {
                self.prev_containers.push(prev_anc.clone());
                prev_ancestor = prev_anc.get_object().get_parent();
            }
        }
    
        // If the new Object is a container itself, it will be visited
        // automatically at the next actual content step. However, we need to walk up the new ancestry to see if there
        // are more new containers
        let current_child_of_container = pointer.resolve();
        
        if current_child_of_container.is_none() {
            return;
        }

        let mut current_child_of_container = current_child_of_container.unwrap();
    
        let mut current_container_ancestor = current_child_of_container
            .get_object().get_parent();
    
        let mut all_children_entered_at_start = true;
    
        while let Some(current_container) = current_container_ancestor {
            if !self.prev_containers.iter().any(|e| Rc::ptr_eq(e, &current_container))
                || current_container.counting_at_start_only
            {
                // Check whether this ancestor container is being entered at the start,
                // by checking whether the child Object is the first.
                let entering_at_start = current_container
                    .content
                    .first()
                    .map(|first_child| Rc::ptr_eq(first_child, &current_child_of_container) && all_children_entered_at_start)
                    .unwrap_or(false);
    
                // Don't count it as entering at start if we're entering randomly somewhere within
                // a container B that happens to be nested at index 0 of container A. It only
                // counts
                // if we're diverting directly to the first leaf node.
                if !entering_at_start {
                    all_children_entered_at_start = false;
                }
    
                // Mark a visit to this container
                self.visit_container(&current_container, entering_at_start);
    
                current_child_of_container = current_container.clone();
                current_container_ancestor = current_container.get_object().get_parent();
            } else {
                break;
            }
        }
    }

    // TODO: The result and the args should be an object not a String
    pub fn evaluate_function(&mut self, func_name: &str, args: Option<&Vec<String>>, text_output: &mut String) -> Result<Option<String>, String> {
        self.if_async_we_cant("evaluate a function");

        if func_name.trim().is_empty() {
            return Err("Function is empty or white space.".to_owned());
        }

        // Get the content that we need to run
        let func_container = self.knot_container_with_name(func_name);
        if func_container.is_none() {
            let mut e = "Function doesn't exist: '".to_owned();
            e.push_str(func_name);
            e.push('\'');

            return Err(e);
        }

        // Snapshot the output stream
        let output_stream_before = self.get_state().get_output_stream().clone();
        self.get_state_mut().reset_output(None);

        // State will temporarily replace the callstack in order to evaluate
        self.get_state_mut().start_function_evaluation_from_game(func_container.unwrap(), args);

        // Evaluate the function, and collect the string output
        while self.can_continue() {
            let text = self.cont()?;

            text_output.push_str(&text);
        }

        // Restore the output stream in case this was called
        // during main story evaluation.
        self.get_state_mut().reset_output(Some(output_stream_before));

        // Finish evaluation, and see whether anything was produced
        let result = self.get_state_mut().complete_function_evaluation_from_game();

        return result;    
    }

    fn knot_container_with_name(&self, name: &str) -> Option<Rc<Container>> {
        let named_container = self.main_content_container.named_content.get(name);

        named_container.cloned()
    }

    fn next_sequence_shuffle_index(&mut self) -> i32 {
        let pop_evaluation_stack = self.get_state_mut().pop_evaluation_stack();
        let num_elements = if let Some(v) = Value::get_int_value(pop_evaluation_stack.as_ref()) {
            v
        } else {
            panic!("Expected number of elements in sequence for shuffle index");
        };
    
        let seq_container = self.get_state().get_current_pointer().container.unwrap();
    
        let seq_count = if let Some(v) = Value::get_int_value(pop_evaluation_stack.as_ref()) {
            v
        } else {
            panic!("Expected sequence count value for shuffle index");
        };
    
        let loop_index = seq_count / num_elements;
        let iteration_index = seq_count % num_elements;
    
        // Generate the same shuffle based on:
        // - The hash of this container, to make sure it's consistent
        //   each time the runtime returns to the sequence
        // - How many times the runtime has looped around this full shuffle
        let seq_path_str = Object::get_path(seq_container.as_ref()).to_string();
        let sequence_hash: i32 = seq_path_str.chars().map(|c| c as i32).sum();
        let random_seed = sequence_hash + loop_index + self.get_state().story_seed;
    
        let mut rng = StdRng::seed_from_u64(random_seed as u64);
    
        let mut unpicked_indices: Vec<i32> = (0..num_elements).collect();
    
        for i in 0..=iteration_index {
            let chosen = rng.gen::<i32>().rem_euclid(unpicked_indices.len() as i32);
            let chosen_index = unpicked_indices[chosen as usize];
            unpicked_indices.retain(|&x| x != chosen_index);
    
            if i == iteration_index {
                return chosen_index;
            }
        }
    
        panic!("Should never reach here");
    }

    pub fn get_global_tags(&self) -> Result<Vec<String>, String> {
        self.tags_at_start_of_flow_container_with_path_string("")
    }

    pub fn tags_for_content_at_path(&self, path: &str) -> Result<Vec<String>, String> {
        self.tags_at_start_of_flow_container_with_path_string(path)
    }

    fn tags_at_start_of_flow_container_with_path_string(&self, path_string: &str) -> Result<Vec<String>, String> {
        let path = Path::new_with_components_string(Some(path_string));

        // Expected to be global story, knot, or stitch
        let mut flow_container = self.content_at_path(&path).container().unwrap();
    
        while let Some(first_content) = flow_container.content.get(0) {
            if let Ok(container) = first_content.clone().into_any().downcast::<Container>() {
                flow_container = container;
            } else {
                break;
            }
        }
    
        // Any initial tag objects count as the "main tags" associated with that
        // story/knot/stitch
        let mut in_tag = false;
        let mut tags = Vec::new();
        
        for content in &flow_container.content {        
            match content.as_ref().as_any().downcast_ref::<ControlCommand>() {
                Some(command) => {
                    match command.command_type {
                        CommandType::BeginTag => in_tag = true,
                        CommandType::EndTag => in_tag = false,
                        _ => {}
                    }
                }
                _ => {
                    if in_tag {
                        if let Some(string_value) = Value::get_string_value(content.as_ref()) {
                            tags.push(string_value.string.clone());
                        } else {
                            return Err(
                                "Tag contained non-text content. Only plain text is allowed when using globalTags or TagsAtContentPath. If you want to evaluate dynamic content, you need to use story.Continue()".to_owned(),
                            );
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    
        Ok(tags)    
    }

    fn content_at_path(&self, path: &Path) -> SearchResult {
        self.main_content_container.content_at_path(path, 0, -1)
    }

    pub fn get_current_tags(&mut self) -> Vec<String> {
        self.if_async_we_cant("call currentTags since it's a work in progress");
        return self.get_state_mut().get_current_tags();
    }

    pub fn choose_path_string(&mut self, path: &str, reset_call_stack: bool, args: Option<&Vec<String>>) -> Result<(), String> {
        self.if_async_we_cant("call ChoosePathString right now");

        if reset_call_stack {
            self.reset_callstack();
        } else {
            // ChoosePathString is potentially dangerous since you can call it when the
            // stack is
            // pretty much in any state. Let's catch one of the worst offenders.
            if self.get_state().get_callstack().borrow().get_current_element().push_pop_type == PushPopType::Function {
                let mut func_detail = "".to_owned();
                let container = self.get_state().get_callstack().borrow().get_current_element().current_pointer.container.clone();
                if let Some(container) = container {
                    func_detail = format!("({})", Object::get_path(container.as_ref()).to_string());
                }
                
                // Err("Story was running a function " + funcDetail + "when you called ChoosePathString("
                //         + path + ") - this is almost certainly not not what you want! Full stack trace: \n"
                //         + state.getCallStack().getCallStackTrace());
                return Err("Story was running a function".to_owned());
            }
        }

        self.get_state_mut().pass_arguments_to_evaluation_stack(args)?;
        self.choose_path(&Path::new_with_components_string(Some(path)), true);

        Ok(())
    }

    fn reset_callstack(&mut self) {
        self.if_async_we_cant("ResetCallstack");

        self.get_state_mut().force_end();
    }

    pub fn switch_flow(&mut self, flow_name: &str) {
        self.if_async_we_cant("switch flow");

        if self.async_saving {
            panic!("Story is already in background saving mode, can't switch flow to {}", flow_name);
        }

        self.get_state_mut().switch_flow_internal(flow_name);
    }

    fn if_async_we_cant(&self, activity_str: &str) {
        if self.async_continue_active {
            panic!("Can't {}. Story is in the middle of a ContinueAsync(). Make more continue_async() calls or a single cont() call beforehand.", activity_str);
        }
    }
}

