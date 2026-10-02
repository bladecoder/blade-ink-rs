//! Execution loop and choices.

use super::*;

impl Runtime {
    #[cfg(test)]
    pub(crate) fn new(data: StoryData) -> Result<Self, StoryError> {
        Self::new_with_seed(data, 1)
    }

    pub(crate) fn new_with_seed(data: StoryData, seed: i32) -> Result<Self, StoryError> {
        Self::new_shared(Rc::new(StoryContent::Arena(data)), seed)
    }

    #[cfg(feature = "binary-image")]
    pub(crate) fn new_image(data: crate::image::ImageView, seed: i32) -> Result<Self, StoryError> {
        Self::new_shared(Rc::new(StoryContent::Image(data)), seed)
    }

    pub(super) fn new_shared(data: Rc<StoryContent>, seed: i32) -> Result<Self, StoryError> {
        let root = data
            .container_id(data.root())
            .ok_or_else(|| StoryError::BadJson("root is not a container".to_owned()))?;
        let callstack = Rc::new(RefCell::new(CallStack::new(root)));
        let variables = VariablesState::new(callstack.clone(), data.clone());
        let mut runtime = Self {
            data,
            callstack,
            variables,
            counters: StoryCounters::new(),
            evaluation_stack: Vec::new(),
            output: String::new(),
            string_stack: Vec::new(),
            tag_stack: Vec::new(),
            current_tags: Vec::new(),
            glue_active: false,
            diverted: ContentPointer::NULL,
            did_safe_exit: false,
            choices: Vec::new(),
            current_turn_index: -1,
            story_seed: seed,
            previous_random: 0,
            externals: Rc::new(RefCell::new(HashMap::new())),
            allow_external_fallbacks: false,
            lookahead_active: false,
            lookahead_unsafe_external: false,
            current_flow_name: "DEFAULT_FLOW".to_owned(),
            named_flows: HashMap::new(),
            changed_variables: HashMap::new(),
            previsited_container: None,
            continuation_active: false,
            newline_snapshot: None,
        };
        if let Some(global_decl) = runtime.data.named_child(runtime.data.root(), "global decl") {
            let global_decl = runtime.data.container_id(global_decl).ok_or_else(|| {
                StoryError::BadJson("global declarations are not a container".to_owned())
            })?;
            let original = runtime.callstack.borrow().current_pointer();
            runtime
                .callstack
                .borrow_mut()
                .set_current_pointer(ContentPointer::at_container(global_decl));
            while runtime.can_continue() {
                runtime.cont()?;
            }
            runtime.callstack.borrow_mut().set_current_pointer(original);
            runtime.output.clear();
            runtime.did_safe_exit = false;
        }
        runtime.variables.snapshot_default_globals();
        runtime.changed_variables.clear();
        Ok(runtime)
    }

    pub(crate) fn reset_state(&mut self, seed: i32) -> Result<(), StoryError> {
        let mut reset = Self::new_shared(self.data.clone(), seed)?;
        reset.externals = self.externals.clone();
        reset.allow_external_fallbacks = self.allow_external_fallbacks;
        *self = reset;
        Ok(())
    }

    pub(crate) fn can_continue(&self) -> bool {
        !self.callstack.borrow().current_pointer().is_null()
    }

    /// Executes until a newline or the end of the current flow.
    pub(crate) fn cont(&mut self) -> Result<String, StoryError> {
        Ok(self.cont_with_budget(|| Ok(false))?.unwrap())
    }

    pub(crate) fn is_async_continue_active(&self) -> bool {
        self.continuation_active
    }

    pub(crate) fn force_end(&mut self) {
        self.callstack
            .borrow_mut()
            .set_current_pointer(ContentPointer::NULL);
        self.choices.clear();
        self.continuation_active = false;
        self.newline_snapshot = None;
    }

    pub(crate) fn cont_with_budget(
        &mut self,
        mut should_yield: impl FnMut() -> Result<bool, StoryError>,
    ) -> Result<Option<String>, StoryError> {
        if !self.continuation_active {
            if !self.can_continue() {
                return Err(StoryError::InvalidStoryState(
                    "Can't continue - should check can_continue before calling Continue".to_owned(),
                ));
            }
            self.variables.start_variable_observation();
            self.output.clear();
            self.current_tags.clear();
            self.did_safe_exit = false;
            self.lookahead_active = false;
            self.lookahead_unsafe_external = false;
        }
        self.continuation_active = true;
        let result = self.cont_internal(&mut should_yield);
        if !matches!(result, Ok(None)) {
            self.continuation_active = false;
            self.changed_variables = self.variables.complete_variable_observation();
            self.newline_snapshot = None;
        }
        result
    }

    pub(super) fn cont_internal(
        &mut self,
        should_yield: &mut impl FnMut() -> Result<bool, StoryError>,
    ) -> Result<Option<String>, StoryError> {
        let mut newline_snapshot = self.newline_snapshot.take();
        while self.can_continue() {
            self.step()?;
            if self.lookahead_unsafe_external {
                *self = *newline_snapshot.take().ok_or_else(|| {
                    StoryError::InvalidStoryState(
                        "unsafe external without newline snapshot".to_owned(),
                    )
                })?;
                break;
            }
            if !self.can_continue() {
                self.follow_default_invisible_choice()?;
            }
            if let Some(previous) = newline_snapshot.as_ref() {
                if !self.output.starts_with(&previous.output) {
                    // Glue removed the newline, so this line is still open.
                    newline_snapshot = None;
                    self.lookahead_active = false;
                } else if self.current_tags.len() > previous.current_tags.len()
                    || self.output[previous.output.len()..]
                        .chars()
                        .any(|character| character != ' ' && character != '\t')
                {
                    *self = *newline_snapshot.take().unwrap();
                    break;
                }
            }
            if newline_snapshot.is_none() && self.output.ends_with('\n') {
                if self.can_continue() {
                    newline_snapshot = Some(Box::new(self.snapshot()));
                    self.lookahead_active = true;
                } else {
                    break;
                }
            }
            if should_yield()? {
                self.newline_snapshot = newline_snapshot;
                return Ok(None);
            }
        }
        if !self.can_continue() && self.choices.is_empty() && !self.did_safe_exit {
            return Err(StoryError::InvalidStoryState(
                "Ink had 1 error. It is strongly suggested that you assign an error handler to story.onError. The first issue was: RUNTIME ERROR: ran out of content. Do you need a '-> DONE' or '-> END'?".to_owned(),
            ));
        }
        Ok(Some(clean_output_whitespace(&self.output)))
    }

    pub(crate) fn snapshot(&self) -> Self {
        let callstack = Rc::new(RefCell::new(self.callstack.borrow().clone()));
        let mut variables = self.variables.clone();
        variables.set_callstack(callstack.clone());
        Self {
            data: self.data.clone(),
            callstack,
            variables,
            counters: self.counters.clone(),
            evaluation_stack: self.evaluation_stack.clone(),
            output: self.output.clone(),
            string_stack: self.string_stack.clone(),
            tag_stack: self.tag_stack.clone(),
            current_tags: self.current_tags.clone(),
            glue_active: self.glue_active,
            diverted: self.diverted,
            did_safe_exit: self.did_safe_exit,
            choices: self.choices.clone(),
            current_turn_index: self.current_turn_index,
            story_seed: self.story_seed,
            previous_random: self.previous_random,
            externals: self.externals.clone(),
            allow_external_fallbacks: self.allow_external_fallbacks,
            lookahead_active: self.lookahead_active,
            lookahead_unsafe_external: self.lookahead_unsafe_external,
            current_flow_name: self.current_flow_name.clone(),
            named_flows: self
                .named_flows
                .iter()
                .map(|(name, flow)| (name.clone(), flow.snapshot()))
                .collect(),
            changed_variables: self.changed_variables.clone(),
            previsited_container: self.previsited_container,
            continuation_active: self.continuation_active,
            newline_snapshot: None,
        }
    }

    pub(super) fn step(&mut self) -> Result<(), StoryError> {
        let data = self.data.clone();
        let mut pointer = self.callstack.borrow().current_pointer();
        let previous = self.callstack.borrow().current_thread().previous_pointer;
        let mut id = pointer.resolve(&data).ok_or_else(|| {
            StoryError::InvalidStoryState("story pointer does not resolve".to_owned())
        })?;

        while let Some(container) = data.container_id(id) {
            let NodeKindView::Container { count_flags, .. } = data.node_view(id).unwrap().kind
            else {
                unreachable!()
            };
            let already_open = previous.resolve(&data).is_some_and(|previous_id| {
                let mut ancestor = Some(previous_id);
                while let Some(current) = ancestor {
                    if current == id {
                        return true;
                    }
                    ancestor = data.node_view(current).and_then(|node| node.parent);
                }
                false
            });
            let record_visit = (!already_open || count_flags & 4 != 0)
                && self.previsited_container != Some(container);
            if self.previsited_container == Some(container) {
                self.previsited_container = None;
            }
            if record_visit && count_flags & 1 != 0 {
                self.counters.record_visit(container);
            }
            if record_visit && count_flags & 2 != 0 {
                self.counters
                    .record_turn(container, self.current_turn_index);
            }
            if data.child_count(id) == Some(0) {
                self.advance()?;
                return Ok(());
            }
            pointer = ContentPointer::start_of(container);
            id = pointer.resolve(&data).unwrap();
        }
        self.callstack.borrow_mut().set_current_pointer(pointer);
        let node = data.node_view(id).unwrap();
        match node.kind {
            NodeKindView::Value(value) => {
                let value = match value {
                    ValueView::VariablePointer {
                        name,
                        context_index: -1,
                    } => {
                        let context = self.callstack.borrow().context_for_variable_named(name);
                        Value::new_variable_pointer(name, context as i32)
                    }
                    _ => Value::new_value_type(value.to_runtime()),
                };
                if self
                    .callstack
                    .borrow()
                    .current_element()
                    .in_expression_evaluation
                {
                    self.push_evaluation(Rc::new(value));
                } else if let ValueType::String(text) = value.value {
                    self.append_text(&text.string);
                }
            }
            NodeKindView::Divert {
                path,
                target,
                variable_name,
                external_args,
                conditional,
                external,
                pushes_to_stack,
                stack_push_type,
            } => {
                if conditional {
                    let condition = self.pop_value()?;
                    if !Self::truthy(condition.as_ref())? {
                        self.advance()?;
                        return Ok(());
                    }
                }
                if external {
                    let name = path
                        .map(|path| path.to_runtime().to_string())
                        .ok_or_else(|| {
                            StoryError::InvalidStoryState(
                                "external function has no name".to_owned(),
                            )
                        })?;
                    self.call_external(&name, external_args)?;
                    self.advance()?;
                    return Ok(());
                }
                let target = if let Some(name) = variable_name {
                    let value = self
                        .variables
                        .get_variable_with_name(name, -1)
                        .ok_or_else(|| {
                            StoryError::InvalidStoryState(format!(
                                "Tried to divert using a target from a variable that could not be found ({name})"
                            ))
                        })?;
                    let ValueType::DivertTarget(path) = &value.value else {
                        return Err(StoryError::InvalidStoryState(format!(
                            "Variable ({name}) did not contain a divert target"
                        )));
                    };
                    data.pointer_at_path(path)
                } else {
                    target.and_then(|id| data.pointer_for(id))
                };
                self.diverted = target.ok_or_else(|| {
                    StoryError::InvalidStoryState("divert target does not resolve".to_owned())
                })?;
                if pushes_to_stack {
                    self.callstack.borrow_mut().push(
                        stack_push_type,
                        self.evaluation_stack.len(),
                        self.string_stack.last().unwrap_or(&self.output).len() as i32,
                    );
                }
            }
            NodeKindView::ControlCommand(command) => self.control(command)?,
            NodeKindView::NativeFunction(op) => {
                let function = NativeFunctionCall::new(op);
                let count = function.get_number_of_parameters();
                if self.evaluation_stack.len() < count {
                    return Err(StoryError::InvalidStoryState(
                        "Not enough operands for native function".to_owned(),
                    ));
                }
                let params = self
                    .evaluation_stack
                    .drain(self.evaluation_stack.len() - count..)
                    .collect();
                self.push_evaluation(function.call(params)?);
            }
            NodeKindView::VariableAssignment {
                name,
                global,
                new_declaration,
            } => {
                let value = self.pop_value()?;
                let value = value.into_any().downcast::<Value>().map_err(|_| {
                    StoryError::InvalidStoryState("assignment expected a value".to_owned())
                })?;
                let assignment = VariableAssignment::new(name, new_declaration, global);
                self.variables.assign(&assignment, value)?;
            }
            NodeKindView::VariableReference { name, count_target } => {
                let value: Rc<dyn RTObject> = if let Some(container) = count_target {
                    Rc::new(Value::new(self.counters.visit_count(container)))
                } else {
                    self.variables
                        .get_variable_with_name(name, -1)
                        .ok_or_else(|| {
                            StoryError::InvalidStoryState(format!(
                                "Variable '{}' was not found",
                                name
                            ))
                        })?
                };
                self.push_evaluation(value);
            }
            NodeKindView::Glue => {
                let stream = self.string_stack.last_mut().unwrap_or(&mut self.output);
                let trailing_start = stream
                    .rfind(|character: char| !character.is_whitespace())
                    .map_or(0, |index| index + 1);
                if let Some(relative_newline) = stream[trailing_start..].find('\n') {
                    stream.truncate(trailing_start + relative_newline);
                }
                self.glue_active = true;
            }
            NodeKindView::Void => self.evaluation_stack.push(Rc::new(Void::new())),
            NodeKindView::Tag(text) => {
                if self
                    .callstack
                    .borrow()
                    .current_element()
                    .in_expression_evaluation
                {
                    self.evaluation_stack.push(Rc::new(Tag::new(text)));
                } else {
                    self.current_tags.push(text.to_owned());
                }
            }
            NodeKindView::ChoicePoint { flags, target } => {
                self.process_choice(id, flags, target)?
            }
            NodeKindView::Container { .. } => unreachable!("containers were descended above"),
        }
        if matches!(
            node.kind,
            NodeKindView::ControlCommand(CommandType::StartThread)
        ) {
            self.advance()?;
            self.callstack.borrow_mut().push_thread();
            return Ok(());
        }
        self.advance()
    }

    pub(super) fn process_choice(
        &mut self,
        source: NodeId,
        flags: i32,
        target: Option<ContainerId>,
    ) -> Result<(), StoryError> {
        let target = target.ok_or_else(|| {
            StoryError::InvalidStoryState("choice target does not resolve".to_owned())
        })?;
        let mut show = true;
        if flags & 1 != 0 {
            let condition = self.pop_value()?;
            show = Self::truthy(condition.as_ref())?;
        }
        let mut tags = Vec::new();
        let choice_only = if flags & 4 != 0 {
            self.pop_choice_string_and_tags(&mut tags)?
        } else {
            String::new()
        };
        let mut start = if flags & 2 != 0 {
            self.pop_choice_string_and_tags(&mut tags)?
        } else {
            String::new()
        };
        if flags & 16 != 0 && self.counters.visit_count(target) > 0 {
            show = false;
        }
        if show {
            start.push_str(&choice_only);
            self.choices.push(RuntimeChoice {
                target,
                source,
                is_invisible_default: flags & 8 != 0,
                text: start.trim().to_owned(),
                tags,
                thread: self.callstack.borrow_mut().fork_thread(),
            });
        }
        Ok(())
    }

    pub(super) fn pop_choice_string_and_tags(
        &mut self,
        tags: &mut Vec<String>,
    ) -> Result<String, StoryError> {
        let value = self.pop_value()?;
        let Some(text) = Value::get_value::<&StringValue>(value.as_ref()) else {
            return Err(StoryError::InvalidStoryState(
                "choice text was not a string".to_owned(),
            ));
        };
        let text = text.string.clone();
        while self
            .evaluation_stack
            .last()
            .is_some_and(|value| value.as_any().is::<Tag>())
        {
            let tag = self
                .pop_value()?
                .into_any()
                .downcast::<Tag>()
                .map_err(|_| StoryError::InvalidStoryState("choice tag invalid".to_owned()))?;
            tags.insert(0, tag.get_text().clone());
        }
        Ok(text)
    }

    pub(super) fn follow_default_invisible_choice(&mut self) -> Result<(), StoryError> {
        if self.choices.is_empty()
            || self
                .choices
                .iter()
                .any(|choice| !choice.is_invisible_default)
        {
            return Ok(());
        }
        let choice = self.choices.remove(0);
        self.callstack
            .borrow_mut()
            .set_current_thread(choice.thread);
        self.callstack
            .borrow_mut()
            .set_current_pointer(ContentPointer::at_container(choice.target));
        self.choices.clear();
        Ok(())
    }

    pub(crate) fn choices(&self) -> &[RuntimeChoice] {
        &self.choices
    }

    pub(crate) fn choose_choice_index(&mut self, index: usize) -> Result<(), StoryError> {
        let visible: Vec<_> = self
            .choices
            .iter()
            .enumerate()
            .filter(|(_, choice)| !choice.is_invisible_default)
            .map(|(index, _)| index)
            .collect();
        let index = *visible
            .get(index)
            .ok_or_else(|| StoryError::BadArgument("choice out of range".to_owned()))?;
        let choice = self.choices.swap_remove(index);
        self.callstack
            .borrow_mut()
            .set_current_thread(choice.thread);
        self.callstack
            .borrow_mut()
            .set_current_pointer(ContentPointer::at_container(choice.target));
        self.choices.clear();
        self.current_turn_index += 1;
        Ok(())
    }
}
