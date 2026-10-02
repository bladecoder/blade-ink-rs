//! Ink commands and evaluation stack.

use super::*;

impl Runtime {
    pub(super) fn control(&mut self, command: CommandType) -> Result<(), StoryError> {
        match command {
            CommandType::EvalStart => {
                self.callstack
                    .borrow_mut()
                    .current_element_mut()
                    .in_expression_evaluation = true;
            }
            CommandType::EvalEnd => {
                self.callstack
                    .borrow_mut()
                    .current_element_mut()
                    .in_expression_evaluation = false;
            }
            CommandType::EvalOutput => {
                let value = self.pop_value()?;
                if !value.as_any().is::<Void>() {
                    self.append_text(&value.to_string());
                }
            }
            CommandType::BeginString => {
                if !self
                    .callstack
                    .borrow()
                    .current_element()
                    .in_expression_evaluation
                {
                    return Err(StoryError::InvalidStoryState(
                        "Expected expression evaluation when beginning a string".to_owned(),
                    ));
                }
                self.string_stack.push(String::new());
                self.callstack
                    .borrow_mut()
                    .current_element_mut()
                    .in_expression_evaluation = false;
            }
            CommandType::EndString => {
                let text = self.string_stack.pop().ok_or_else(|| {
                    StoryError::InvalidStoryState("string evaluation was not started".to_owned())
                })?;
                self.callstack
                    .borrow_mut()
                    .current_element_mut()
                    .in_expression_evaluation = true;
                self.evaluation_stack
                    .push(Rc::new(Value::new(text.as_str())));
            }
            CommandType::BeginTag => self.tag_stack.push(String::new()),
            CommandType::EndTag => {
                // Ink can emit an EndTag without a matching BeginTag when a
                // sequence branches across a continuation boundary. The
                // original output stream accepts that marker as well.
                if let Some(tag) = self.tag_stack.pop() {
                    let tag = clean_output_whitespace(&tag);
                    if !self.string_stack.is_empty() {
                        self.evaluation_stack.push(Rc::new(Tag::new(&tag)));
                    } else if !tag.is_empty() {
                        self.current_tags.push(tag);
                    }
                }
            }
            CommandType::Duplicate => {
                let value = self.evaluation_stack.last().cloned().ok_or_else(|| {
                    StoryError::InvalidStoryState("evaluation stack is empty".to_owned())
                })?;
                self.evaluation_stack.push(value);
            }
            CommandType::PopEvaluatedValue => {
                self.pop_value()?;
            }
            CommandType::Done => {
                if self.callstack.borrow().can_pop_thread() {
                    self.callstack.borrow_mut().pop_thread()?;
                } else {
                    self.did_safe_exit = true;
                    self.callstack
                        .borrow_mut()
                        .set_current_pointer(ContentPointer::NULL);
                }
            }
            CommandType::End => {
                self.did_safe_exit = true;
                self.callstack
                    .borrow_mut()
                    .set_current_pointer(ContentPointer::NULL);
            }
            CommandType::NoOp => {}
            CommandType::ChoiceCount => self.push_int(self.choices.len() as i32),
            CommandType::Turns => self.push_int(self.current_turn_index + 1),
            CommandType::TurnsSince | CommandType::ReadCount => {
                let target = self.pop_value()?;
                let path = Value::get_value::<&Path>(target.as_ref()).ok_or_else(|| {
                    StoryError::InvalidStoryState(format!(
                        "TURNS_SINCE expected a divert target (knot, stitch, label name), but saw {target} "
                    ))
                })?;
                let count = match self
                    .data
                    .resolve_path(self.data.root(), path)
                    .and_then(|id| self.data.container_id(id))
                {
                    Some(container) if command == CommandType::ReadCount => {
                        self.counters.visit_count(container)
                    }
                    Some(container) => self
                        .counters
                        .turn_index(container)
                        .map_or(-1, |turn| self.current_turn_index - turn),
                    None if command == CommandType::ReadCount => 0,
                    None => -1,
                };
                self.push_int(count);
            }
            CommandType::VisitIndex => {
                let container = self.callstack.borrow().current_pointer().container.unwrap();
                self.push_int(self.counters.visit_count(container) - 1);
            }
            CommandType::SeedRandom => {
                self.story_seed = self.pop_int("Invalid value passed to SEED_RANDOM")?;
                self.previous_random = 0;
                self.evaluation_stack.push(Rc::new(Void::new()));
            }
            CommandType::Random => {
                let max =
                    self.pop_int("Invalid value for the maximum parameter of RANDOM(min, max)")?;
                let min =
                    self.pop_int("Invalid value for the minimum parameter of RANDOM(min, max)")?;
                let range = max - min + 1;
                if range <= 0 {
                    return Err(StoryError::InvalidStoryState(format!(
                        "RANDOM was called with minimum as {min} and maximum as {max}. The maximum must be larger"
                    )));
                }
                let mut rng =
                    StdRng::seed_from_u64((self.story_seed + self.previous_random) as u64);
                self.push_int((rng.random::<u32>() % range as u32) as i32 + min);
                self.previous_random += 1;
            }
            CommandType::SequenceShuffleIndex => {
                let elements =
                    self.pop_int("Expected number of elements in sequence for shuffle index")?;
                let count = self.pop_int("Expected sequence count value for shuffle index")?;
                if elements <= 0 {
                    return Err(StoryError::InvalidStoryState(
                        "Expected a positive number of elements in sequence for shuffle index"
                            .to_owned(),
                    ));
                }
                let container = self.callstack.borrow().current_pointer().container.unwrap();
                let path = self.data.canonical_path_text(container.node()).unwrap();
                let path_hash: i32 = path.chars().map(|character| character as i32).sum();
                let loop_index = count / elements;
                let iteration_index = count % elements;
                let mut rng =
                    StdRng::seed_from_u64((path_hash + loop_index + self.story_seed) as u64);
                let mut unpicked: Vec<i32> = (0..elements).collect();
                let mut picked = 0;
                for _ in 0..=iteration_index {
                    let index = rng.random::<i32>().rem_euclid(unpicked.len() as i32) as usize;
                    picked = unpicked.remove(index);
                }
                self.push_int(picked);
            }
            CommandType::ListFromInt => {
                let value = self.pop_int(
                    "Passed non-integer when creating a list element from a numerical value.",
                )?;
                let name = self.pop_value()?;
                let name = Value::get_value::<&StringValue>(name.as_ref())
                    .ok_or_else(|| {
                        StoryError::InvalidStoryState(
                            "Expected list name for LIST_FROM_INT".to_owned(),
                        )
                    })?
                    .string
                    .clone();
                let definition = self.data.list_definition_index(&name).ok_or_else(|| {
                    StoryError::InvalidStoryState(format!("Failed to find List called {name}"))
                })?;
                let list = (0..self
                    .data
                    .list_definition_item_count(definition)
                    .unwrap_or(0))
                    .filter_map(|index| self.data.list_definition_item_at(definition, index))
                    .find(|(_, item_value)| *item_value == value)
                    .map_or_else(InkList::new, |(item_name, _)| {
                        InkList::from_single_element((
                            InkListItem::new(Some(name.clone()), item_name.to_owned()),
                            value,
                        ))
                    });
                self.push_evaluation(Rc::new(Value::new(list)));
            }
            CommandType::ListRange => {
                let max = self.pop_value()?;
                let min = self.pop_value()?;
                let list = self.pop_value()?;
                let (Some(max), Some(min), Some(list)) = (
                    max.as_any().downcast_ref::<Value>(),
                    min.as_any().downcast_ref::<Value>(),
                    Value::get_value::<&InkList>(list.as_ref()),
                ) else {
                    return Err(StoryError::InvalidStoryState(
                        "Expected List, minimum and maximum for LIST_RANGE".to_owned(),
                    ));
                };
                self.push_evaluation(Rc::new(Value::new(
                    list.list_with_sub_range(&min.value, &max.value),
                )));
            }
            CommandType::ListRandom => {
                let source = self.pop_value()?;
                let list = Value::get_value::<&InkList>(source.as_ref()).ok_or_else(|| {
                    StoryError::InvalidStoryState("Expected list for LIST_RANDOM".to_owned())
                })?;
                let selected = if list.items.is_empty() {
                    InkList::new()
                } else {
                    let mut rng =
                        StdRng::seed_from_u64((self.story_seed + self.previous_random) as u64);
                    let random = rng.random::<u32>();
                    let mut sorted: Vec<_> = list.items.iter().collect();
                    sorted.sort_by(|left, right| right.1.cmp(left.1));
                    let (item, value) = sorted[random as usize % sorted.len()];
                    self.previous_random = random as i32;
                    InkList::from_single_element((item.clone(), *value))
                };
                self.push_evaluation(Rc::new(Value::new(selected)));
            }
            CommandType::StartThread => {}
            CommandType::PopFunction | CommandType::PopTunnel => {
                let kind = if command == CommandType::PopFunction {
                    PushPopType::Function
                } else {
                    PushPopType::Tunnel
                };
                if self.callstack.borrow().current_element().push_pop_type
                    == PushPopType::FunctionEvaluationFromGame
                {
                    self.callstack
                        .borrow_mut()
                        .set_current_pointer(ContentPointer::NULL);
                    self.did_safe_exit = true;
                    return Ok(());
                }
                let override_target = if kind == PushPopType::Tunnel {
                    let returned = self.pop_value()?;
                    if let Some(path) = Value::get_value::<&Path>(returned.as_ref()) {
                        Some(self.data.pointer_at_path(path).ok_or_else(|| {
                            StoryError::InvalidStoryState(
                                "tunnel return target does not resolve".to_owned(),
                            )
                        })?)
                    } else if returned.as_any().is::<Void>() {
                        None
                    } else {
                        return Err(StoryError::InvalidStoryState(
                            "Expected void if ->-> doesn't override target".to_owned(),
                        ));
                    }
                } else {
                    None
                };
                if kind == PushPopType::Function {
                    self.trim_function_output();
                }
                self.callstack.borrow_mut().pop(Some(kind))?;
                if let Some(target) = override_target {
                    self.diverted = target;
                }
            }
        }
        Ok(())
    }

    pub(super) fn advance(&mut self) -> Result<(), StoryError> {
        if self.callstack.borrow().current_pointer().is_null() {
            return Ok(());
        }
        let previous = self.callstack.borrow().current_pointer();
        self.callstack
            .borrow_mut()
            .current_thread_mut()
            .previous_pointer = previous;
        if !self.diverted.is_null() {
            let target = self.diverted;
            self.record_entered_ancestors(previous, target);
            self.callstack.borrow_mut().set_current_pointer(target);
            self.diverted = ContentPointer::NULL;
            return Ok(());
        }
        let pointer = self.callstack.borrow().current_pointer();
        if let Some(next) = pointer.increment(&self.data) {
            self.callstack.borrow_mut().set_current_pointer(next);
            return Ok(());
        }
        if self
            .callstack
            .borrow()
            .can_pop_type(Some(PushPopType::Function))
        {
            self.trim_function_output();
            self.callstack
                .borrow_mut()
                .pop(Some(PushPopType::Function))?;
            if self
                .callstack
                .borrow()
                .current_element()
                .in_expression_evaluation
            {
                self.evaluation_stack.push(Rc::new(Void::new()));
            }
            let parent = self.callstack.borrow().current_pointer();
            self.callstack
                .borrow_mut()
                .set_current_pointer(parent.increment(&self.data).unwrap_or(ContentPointer::NULL));
        } else if self.callstack.borrow().can_pop_thread() {
            self.callstack.borrow_mut().pop_thread()?;
            self.advance()?;
        } else if self.callstack.borrow().current_element().push_pop_type
            == PushPopType::FunctionEvaluationFromGame
        {
            self.callstack
                .borrow_mut()
                .set_current_pointer(ContentPointer::NULL);
            self.did_safe_exit = true;
        } else {
            self.callstack
                .borrow_mut()
                .set_current_pointer(ContentPointer::NULL);
        }
        Ok(())
    }

    pub(super) fn pop_value(&mut self) -> Result<Rc<dyn RTObject>, StoryError> {
        self.evaluation_stack
            .pop()
            .ok_or_else(|| StoryError::InvalidStoryState("evaluation stack is empty".to_owned()))
    }

    pub(super) fn pop_int(&mut self, error: &str) -> Result<i32, StoryError> {
        let value = self.pop_value()?;
        Value::get_value::<i32>(value.as_ref())
            .ok_or_else(|| StoryError::InvalidStoryState(error.to_owned()))
    }
}
