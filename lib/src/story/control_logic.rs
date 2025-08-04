use crate::{
    container::Container,
    control_command::{CommandType, ControlCommand},
    divert::Divert,
    ink_list::InkList,
    ink_list_item::InkListItem,
    native_function_call::NativeFunctionCall,
    object::RTObject,
    path::Path,
    pointer,
    push_pop::PushPopType,
    story::Story,
    story_error::StoryError,
    story_state::StoryState,
    tag::Tag,
    value::Value,
    value_type::{StringValue, ValueType},
    variable_assigment::VariableAssignment,
    variable_reference::VariableReference,
    void::Void,
};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
};

/// # Control and Logic
/// Methods for performing logic and flow control.
impl Story {
    pub(crate) fn perform_logic_and_flow_control(
        &mut self,
        content_obj: &Option<Rc<dyn RTObject>>,
    ) -> Result<bool, StoryError> {
        let content_obj = match content_obj {
            Some(content_obj) => content_obj.clone(),
            None => return Ok(false),
        }; // Divert
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
                    if let Some(target) = Value::get_value::<&Path>(var_contents.as_ref()) {
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
                        let output = self.get_state_mut().pop_evaluation_stack(); // Functions may evaluate to Void, in which case we skip
                                                                                  // output
                        if !output.as_ref().as_any().is::<Void>() {
                            // TODO: Should we really always blanket convert to
                            // string?
                            // It would be okay to have numbers in the output stream
                            // the
                            // only problem is when exporting text for viewing, it
                            // skips over numbers etc.
                            let text: Rc<dyn RTObject> =
                                Rc::new(Value::new::<&str>(&output.to_string()));
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
                    }; // Tunnel onwards is allowed to specify an optional override
                       // divert to go to immediately after returning: ->-> target
                    let mut override_tunnel_return_target = None;
                    if pop_type == PushPopType::Tunnel {
                        let popped = self.get_state_mut().pop_evaluation_stack();
                        if let Some(v) = Value::get_value::<&Path>(popped.as_ref()) {
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
                        self.get_state_mut().pop_callstack(None)?; // Does tunnel onwards override by diverting to a new ->->
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

                        if Value::get_value::<&StringValue>(obj.as_ref()).is_some() {
                            content_stack_for_string.push_back(obj.clone());
                        }
                    }

                    // Consume the content that was produced for this string
                    self.get_state_mut()
                        .pop_from_output_stream(output_count_consumed); // Rescue the tags that we want actually to keep on the output stack
                                                                        // rather than consume as part of the string we're building.
                                                                        // At the time of writing, this only applies to Tag objects generated
                                                                        // by choices, which are pushed to the stack during string generation.

                    while let Some(rescue_tag) = content_to_retain.pop_back() {
                        self.get_state_mut()
                            .push_to_output_stream(rescue_tag);
                    }

                    // Build string out of the content we collected
                    let mut sb = String::new();
                    while let Some(c) = content_stack_for_string.pop_back() {
                        sb.push_str(&c.to_string());
                    }

                    // Return to expression evaluation (from content mode)
                    self.get_state().set_in_expression_evaluation(true);
                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Value::new::<&str>(&sb)));
                }
                CommandType::NoOp => {}
                CommandType::ChoiceCount => {
                    let choice_count = self.get_state().get_generated_choices().len();
                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Value::new::<i32>(choice_count as i32)));
                }
                CommandType::Turns => {
                    let current_turn = self.get_state().current_turn_index;
                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Value::new::<i32>(current_turn + 1)));
                }
                CommandType::TurnsSince | CommandType::ReadCount => {
                    let target = self.get_state_mut().pop_evaluation_stack();
                    if Value::get_value::<&Path>(target.as_ref()).is_none() {
                        let mut extra_note = "".to_owned();
                        if Value::get_value::<i32>(target.as_ref()).is_some() {
                            extra_note = format!(". Did you accidentally pass a read count ('knot_name') instead of a target {}",
                                    "('-> knot_name')?").to_owned();
                        }

                        return Err(StoryError::InvalidStoryState(format!("TURNS_SINCE expected a divert target (knot, stitch, label name), but saw {} {}", target
                                , extra_note)));
                    }

                    let target = Value::get_value::<&Path>(target.as_ref()).unwrap();
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
                        .push_evaluation_stack(Rc::new(Value::new::<i32>(either_count)));
                }
                CommandType::Random => {
                    let mut max_int = None;
                    let o = self.get_state_mut().pop_evaluation_stack();
                    if let Some(v) = Value::get_value::<i32>(o.as_ref()) {
                        max_int = Some(v);
                    }

                    let o = self.get_state_mut().pop_evaluation_stack();
                    let mut min_int = None;
                    if let Some(v) = Value::get_value::<i32>(o.as_ref()) {
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
                        .push_evaluation_stack(Rc::new(Value::new::<i32>(chosen_value)));
                    self.get_state_mut().previous_random = self.get_state().previous_random + 1;
                }
                CommandType::SeedRandom => {
                    let mut seed: Option<i32> = None;
                    let o = self.get_state_mut().pop_evaluation_stack();
                    if let Some(v) = Value::get_value::<i32>(o.as_ref()) {
                        seed = Some(v);
                    }

                    if seed.is_none() {
                        return Err(StoryError::InvalidStoryState(
                            "Invalid value passed to SEED_RANDOM".to_owned(),
                        ));
                    }

                    // Story seed affects both RANDOM and shuffle behaviour
                    self.get_state_mut().story_seed = seed.unwrap();
                    self.get_state_mut().previous_random = 0; // SEED_RANDOM returns nothing.
                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Void::new()));
                }
                CommandType::VisitIndex => {
                    let cpc = self.get_state().get_current_pointer().container.unwrap();
                    let count = self.get_state_mut().visit_count_for_container(&cpc) - 1; // index
                                                                                          // not count
                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Value::new::<i32>(count)));
                }
                CommandType::SequenceShuffleIndex => {
                    let shuffle_index = self.next_sequence_shuffle_index()?;
                    let v = Rc::new(Value::new::<i32>(shuffle_index));
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
                        self.get_state_mut().set_did_safe_exit(true); // Stop flow in current thread
                        self.get_state().set_current_pointer(pointer::NULL.clone());
                    }
                }
                CommandType::End => self.get_state_mut().force_end(),
                CommandType::ListFromInt => {
                    let mut int_val: Option<i32> = None;
                    let mut list_name_val: Option<&String> = None;
                    let o = self.get_state_mut().pop_evaluation_stack();
                    if let Some(v) = Value::get_value::<i32>(o.as_ref()) {
                        int_val = Some(v);
                    }

                    let o = self.get_state_mut().pop_evaluation_stack();
                    if let Some(s) = Value::get_value::<&StringValue>(o.as_ref()) {
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
                            generated_list_value = Some(Value::new::<InkList>(l));
                        }
                    } else {
                        return Err(StoryError::InvalidStoryState(format!(
                            "Failed to find List called {}",
                            list_name_val.as_ref().unwrap()
                        )));
                    }

                    if generated_list_value.is_none() {
                        generated_list_value = Some(Value::new::<InkList>(InkList::new()));
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
                    let target_list = Value::get_value::<&InkList>(p.as_ref());
                    if target_list.is_none() || min.is_err() || max.is_err() {
                        return Err(StoryError::InvalidStoryState(
                            "Expected List, minimum and maximum for LIST_RANGE".to_owned(),
                        ));
                    }

                    let result = target_list
                        .unwrap()
                        .list_with_sub_range(&min.unwrap().value, &max.unwrap().value);
                    self.get_state_mut()
                        .push_evaluation_stack(Rc::new(Value::new::<InkList>(result)));
                }
                CommandType::ListRandom => {
                    let o = self.get_state_mut().pop_evaluation_stack();
                    let list = Value::get_value::<&InkList>(o.as_ref());
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
                            let list_item_index = (next_random as usize) % list.items.len(); // Iterate through to get the random element, sorted for
                                                                                             // predictibility
                            let mut sorted: Vec<(&InkListItem, &i32)> = list.items.iter().collect();
                            sorted.sort_by(|a, b| b.1.cmp(a.1));
                            let random_item = sorted[list_item_index]; // Origin list is simply the origin of the one element
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
                        .push_evaluation_stack(Rc::new(Value::new::<InkList>(new_list)));
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

                            if let Some(sv) = Value::get_value::<&StringValue>(obj.as_ref()) {
                                content_stack_for_tag.push(sv.string.clone());
                            }
                        }

                        // Consume the content that was produced for this string
                        self.get_state_mut()
                            .pop_from_output_stream(output_count_consumed);
                        let mut sb = String::new();
                        for str_val in content_stack_for_tag.iter().rev() {
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
            let assigned_val = self.get_state_mut().pop_evaluation_stack(); // When in temporary evaluation, don't create new variables purely
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
            let found_value: Rc<Value>; // Explicit read count value
            if var_ref.path_for_count.is_some() {
                let container = var_ref.get_container_for_count();
                let count = self
                    .get_state_mut()
                    .visit_count_for_container(container.as_ref().unwrap());
                found_value = Rc::new(Value::new::<i32>(count));
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
                        found_value = Rc::new(Value::new::<i32>(0));
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
}
