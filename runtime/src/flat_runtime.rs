//! Interpreter transition path that reads static instructions by arena ID.
//! Dynamic values, counters, variables and call frames belong to one run.

#[allow(unused_imports)]
use crate::prelude::*;

use crate::{
    choice::Choice,
    compat::{cell::RefCell, collections::HashMap, rc::Rc},
    control_command::CommandType,
    flat_callstack::{FlatCallStack, FlatThread},
    flat_story::{
        ContainerId, FlatCounters, FlatPointer, FlatStoryData, NodeId, NodeKindView,
        StaticStoryView, ValueView,
    },
    ink_list::InkList,
    ink_list_item::InkListItem,
    list_definition::ListDefinition,
    native_function_call::NativeFunctionCall,
    object::RTObject,
    path::Path,
    push_pop::PushPopType,
    story::external_functions::{ExternalFunction, ExternalFunctionResult},
    story_error::StoryError,
    story_state::StoryState,
    tag::Tag,
    value::Value,
    value_type::{StringValue, ValueType},
    variable_assigment::VariableAssignment,
    variables_state::VariablesState,
    void::Void,
};
use rand::{RngExt, SeedableRng, rngs::StdRng};

#[cfg(feature = "stream-json-parser")]
mod state_stream;

#[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
use serde_json::{Map, json};

#[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
use crate::{
    control_command::ControlCommand,
    json::{json_read, json_write},
    story_state::INK_SAVE_STATE_VERSION,
};

#[derive(Clone)]
pub(crate) struct FlatChoice {
    pub(crate) target: ContainerId,
    pub(crate) source: NodeId,
    pub(crate) is_invisible_default: bool,
    pub(crate) text: String,
    pub(crate) tags: Vec<String>,
    pub(crate) thread: FlatThread,
}

pub(crate) struct FlatRuntime {
    data: Rc<FlatStoryData>,
    callstack: Rc<RefCell<FlatCallStack>>,
    variables: VariablesState,
    counters: FlatCounters,
    evaluation_stack: Vec<Rc<dyn RTObject>>,
    output: String,
    string_stack: Vec<String>,
    tag_stack: Vec<String>,
    current_tags: Vec<String>,
    glue_active: bool,
    diverted: FlatPointer,
    did_safe_exit: bool,
    choices: Vec<FlatChoice>,
    current_turn_index: i32,
    story_seed: i32,
    previous_random: i32,
    externals: Rc<RefCell<HashMap<String, BoundExternal>>>,
    allow_external_fallbacks: bool,
    lookahead_active: bool,
    lookahead_unsafe_external: bool,
    current_flow_name: String,
    named_flows: HashMap<String, FlatFlow>,
    changed_variables: HashMap<String, ValueType>,
    previsited_container: Option<ContainerId>,
    continuation_active: bool,
    newline_snapshot: Option<Box<FlatRuntime>>,
}

struct BoundExternal {
    function: Box<dyn ExternalFunction>,
    lookahead_safe: bool,
}

struct FlatFlow {
    callstack: Rc<RefCell<FlatCallStack>>,
    output: String,
    choices: Vec<FlatChoice>,
    current_tags: Vec<String>,
}

impl FlatFlow {
    fn new(root: ContainerId) -> Self {
        Self {
            callstack: Rc::new(RefCell::new(FlatCallStack::new(root))),
            output: String::new(),
            choices: Vec::new(),
            current_tags: Vec::new(),
        }
    }

    fn snapshot(&self) -> Self {
        Self {
            callstack: Rc::new(RefCell::new(self.callstack.borrow().clone())),
            output: self.output.clone(),
            choices: self.choices.clone(),
            current_tags: self.current_tags.clone(),
        }
    }
}

impl FlatRuntime {
    pub(crate) fn new(data: FlatStoryData) -> Result<Self, StoryError> {
        Self::new_with_seed(data, 1)
    }

    pub(crate) fn new_with_seed(data: FlatStoryData, seed: i32) -> Result<Self, StoryError> {
        Self::new_shared(Rc::new(data), seed)
    }

    fn new_shared(data: Rc<FlatStoryData>, seed: i32) -> Result<Self, StoryError> {
        let root = data
            .container_id(data.root())
            .ok_or_else(|| StoryError::BadJson("root is not a container".to_owned()))?;
        let callstack = Rc::new(RefCell::new(FlatCallStack::new(root)));
        let variables = VariablesState::new_flat(callstack.clone(), data.clone());
        let mut runtime = Self {
            data,
            callstack,
            variables,
            counters: FlatCounters::new(),
            evaluation_stack: Vec::new(),
            output: String::new(),
            string_stack: Vec::new(),
            tag_stack: Vec::new(),
            current_tags: Vec::new(),
            glue_active: false,
            diverted: FlatPointer::NULL,
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
                .set_current_pointer(FlatPointer::at_container(global_decl));
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
            .set_current_pointer(FlatPointer::NULL);
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

    fn cont_internal(
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
        Ok(Some(StoryState::clean_output_whitespace(&self.output)))
    }

    fn snapshot(&self) -> Self {
        let callstack = Rc::new(RefCell::new(self.callstack.borrow().clone()));
        let mut variables = self.variables.clone();
        variables.set_flat_callstack(callstack.clone());
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

    fn step(&mut self) -> Result<(), StoryError> {
        let data = self.data.clone();
        let mut pointer = self.callstack.borrow().current_pointer();
        let previous = self.callstack.borrow().current_thread().previous_pointer;
        let mut id = pointer.resolve(&data).ok_or_else(|| {
            StoryError::InvalidStoryState("story pointer does not resolve".to_owned())
        })?;

        while let Some(container) = data.container_id(id) {
            let NodeKindView::Container { count_flags } = data.node_view(id).unwrap().kind else {
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
            pointer = FlatPointer::start_of(container);
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

    fn process_choice(
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
            self.choices.push(FlatChoice {
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

    fn pop_choice_string_and_tags(&mut self, tags: &mut Vec<String>) -> Result<String, StoryError> {
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

    fn follow_default_invisible_choice(&mut self) -> Result<(), StoryError> {
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
            .set_current_pointer(FlatPointer::at_container(choice.target));
        self.choices.clear();
        Ok(())
    }

    pub(crate) fn choices(&self) -> &[FlatChoice] {
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
            .set_current_pointer(FlatPointer::at_container(choice.target));
        self.choices.clear();
        self.current_turn_index += 1;
        Ok(())
    }

    fn control(&mut self, command: CommandType) -> Result<(), StoryError> {
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
                    let tag = StoryState::clean_output_whitespace(&tag);
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
                        .set_current_pointer(FlatPointer::NULL);
                }
            }
            CommandType::End => {
                self.did_safe_exit = true;
                self.callstack
                    .borrow_mut()
                    .set_current_pointer(FlatPointer::NULL);
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
                let definition = self
                    .data
                    .list_definitions
                    .iter()
                    .find(|definition| definition.name == name)
                    .ok_or_else(|| {
                        StoryError::InvalidStoryState(format!("Failed to find List called {name}"))
                    })?;
                let list = definition
                    .items
                    .iter()
                    .find(|(_, item_value)| *item_value == value)
                    .map_or_else(InkList::new, |(item_name, _)| {
                        InkList::from_single_element((
                            InkListItem::new(Some(name.clone()), item_name.clone()),
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
                        .set_current_pointer(FlatPointer::NULL);
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

    fn advance(&mut self) -> Result<(), StoryError> {
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
            self.diverted = FlatPointer::NULL;
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
                .set_current_pointer(parent.increment(&self.data).unwrap_or(FlatPointer::NULL));
        } else if self.callstack.borrow().can_pop_thread() {
            self.callstack.borrow_mut().pop_thread()?;
            self.advance()?;
        } else if self.callstack.borrow().current_element().push_pop_type
            == PushPopType::FunctionEvaluationFromGame
        {
            self.callstack
                .borrow_mut()
                .set_current_pointer(FlatPointer::NULL);
            self.did_safe_exit = true;
        } else {
            self.callstack
                .borrow_mut()
                .set_current_pointer(FlatPointer::NULL);
        }
        Ok(())
    }

    fn pop_value(&mut self) -> Result<Rc<dyn RTObject>, StoryError> {
        self.evaluation_stack
            .pop()
            .ok_or_else(|| StoryError::InvalidStoryState("evaluation stack is empty".to_owned()))
    }

    fn pop_int(&mut self, error: &str) -> Result<i32, StoryError> {
        let value = self.pop_value()?;
        Value::get_value::<i32>(value.as_ref())
            .ok_or_else(|| StoryError::InvalidStoryState(error.to_owned()))
    }

    pub(crate) fn bind_external_function<F>(
        &mut self,
        name: &str,
        function: F,
        lookahead_safe: bool,
    ) -> Result<(), StoryError>
    where
        F: ExternalFunction + 'static,
    {
        let mut externals = self.externals.borrow_mut();
        if externals.contains_key(name) {
            return Err(StoryError::BadArgument(format!(
                "Function '{name}' has already been bound."
            )));
        }
        externals.insert(
            name.to_owned(),
            BoundExternal {
                function: Box::new(function),
                lookahead_safe,
            },
        );
        Ok(())
    }

    pub(crate) fn set_allow_external_fallbacks(&mut self, allowed: bool) {
        self.allow_external_fallbacks = allowed;
    }

    pub(crate) fn unbind_external_function(&mut self, name: &str) -> Result<(), StoryError> {
        if self.externals.borrow_mut().remove(name).is_none() {
            return Err(StoryError::BadArgument(format!(
                "Function '{name}' has not been bound."
            )));
        }
        Ok(())
    }

    pub(crate) fn validate_external_bindings(&self) -> Result<(), StoryError> {
        let bound = self.externals.borrow();
        let mut missing = crate::compat::collections::HashSet::new();
        for index in 0..self.data.node_count() {
            let Some(node_id) = u32::try_from(index).ok().map(NodeId) else {
                break;
            };
            let Some(node) = self.data.node_view(node_id) else {
                continue;
            };
            let NodeKindView::Divert { path, external, .. } = node.kind else {
                continue;
            };
            if !external {
                continue;
            }
            let Some(path) = path else {
                continue;
            };
            let name = path.to_runtime().to_string();
            let has_fallback = self.allow_external_fallbacks
                && self
                    .data
                    .find_named_child(self.data.root(), &name)
                    .is_some();
            if !bound.contains_key(&name) && !has_fallback {
                missing.insert(name);
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        let count = missing.len();
        let mut names: Vec<_> = missing.into_iter().collect();
        names.sort();
        Err(StoryError::InvalidStoryState(format!(
            "ERROR: Missing function binding for external{}: '{}' {}",
            if count > 1 { "s" } else { "" },
            names.join(", "),
            if self.allow_external_fallbacks {
                ", and no fallback ink function found."
            } else {
                " (ink fallbacks disabled)"
            }
        )))
    }

    fn call_external(&mut self, name: &str, count: usize) -> Result<(), StoryError> {
        if self.lookahead_active
            && self
                .externals
                .borrow()
                .get(name)
                .is_some_and(|def| !def.lookahead_safe)
        {
            self.lookahead_unsafe_external = true;
            return Ok(());
        }
        if !self.externals.borrow().contains_key(name) {
            if self.allow_external_fallbacks {
                if let Some(target) = self
                    .data
                    .named_child(self.data.root(), name)
                    .and_then(|id| self.data.container_id(id))
                {
                    self.callstack.borrow_mut().push(
                        PushPopType::Function,
                        self.evaluation_stack.len(),
                        self.string_stack.last().unwrap_or(&self.output).len() as i32,
                    );
                    self.diverted = FlatPointer::start_of(target);
                    return Ok(());
                }
                return Err(StoryError::InvalidStoryState(format!(
                    "Trying to call EXTERNAL function '{name}' which has not been bound, and fallback ink function could not be found."
                )));
            }
            return Err(StoryError::InvalidStoryState(format!(
                "Trying to call EXTERNAL function '{name}' which has not been bound (and ink fallbacks disabled)."
            )));
        }
        let mut args = Vec::with_capacity(count);
        for _ in 0..count {
            let popped = self.pop_value()?;
            let value = popped.as_any().downcast_ref::<Value>().ok_or_else(|| {
                StoryError::InvalidStoryState(format!(
                    "Trying to call EXTERNAL function '{name}' with arguments which are not values."
                ))
            })?;
            args.push(value.value.clone());
        }
        args.reverse();
        let result: ExternalFunctionResult = self
            .externals
            .borrow_mut()
            .get_mut(name)
            .unwrap()
            .function
            .call(name, &args);
        let result = result.map_err(|error| StoryError::ExternalFunctionFailed {
            function_name: name.to_owned(),
            error,
        })?;
        match result {
            Some(value) => self.push_evaluation(Rc::new(Value::new_value_type(value))),
            None => self.evaluation_stack.push(Rc::new(Void::new())),
        }
        Ok(())
    }

    fn push_int(&mut self, value: i32) {
        self.evaluation_stack.push(Rc::new(Value::new(value)));
    }

    fn push_evaluation(&mut self, value: Rc<dyn RTObject>) {
        if let Some(list) = Value::get_value::<&InkList>(value.as_ref()) {
            let names = list.get_origin_names();
            let mut origins = list.origins.borrow_mut();
            origins.clear();
            for name in names {
                if let Some(definition) = self
                    .data
                    .list_definitions
                    .iter()
                    .find(|item| item.name == name)
                {
                    let items: HashMap<_, _> = definition.items.iter().cloned().collect();
                    origins.push(ListDefinition::new(name, items));
                }
            }
        }
        self.evaluation_stack.push(value);
    }

    fn append_text(&mut self, text: &str) {
        if let Some(tag) = self.tag_stack.last_mut() {
            tag.push_str(text);
            return;
        }
        if self.string_stack.is_empty()
            && text == "\n"
            && (self.output.is_empty() || self.output.ends_with('\n'))
        {
            return;
        }
        if self.glue_active {
            if text == "\n" {
                return;
            }
            if text.chars().any(|character| !character.is_whitespace()) {
                self.glue_active = false;
            }
        }
        let leading_function_whitespace = {
            let stack = self.callstack.borrow();
            let element = stack.current_element();
            element.push_pop_type == PushPopType::Function && !element.function_output_started
        };
        if leading_function_whitespace {
            if text.chars().all(char::is_whitespace) {
                return;
            }
            self.callstack
                .borrow_mut()
                .current_element_mut()
                .function_output_started = true;
        }
        self.string_stack
            .last_mut()
            .unwrap_or(&mut self.output)
            .push_str(text);
    }

    fn trim_function_output(&mut self) {
        let (start, started) = {
            let stack = self.callstack.borrow();
            let element = stack.current_element();
            (
                element.function_start_in_output_stream.max(0) as usize,
                element.function_output_started,
            )
        };
        if started {
            let stream = self.string_stack.last_mut().unwrap_or(&mut self.output);
            while stream.len() > start && stream.ends_with(char::is_whitespace) {
                stream.pop();
            }
        }
    }

    fn truthy(value: &dyn RTObject) -> Result<bool, StoryError> {
        let Some(value) = value.as_any().downcast_ref::<Value>() else {
            return Err(StoryError::InvalidStoryState(
                "condition expected a value".to_owned(),
            ));
        };
        match &value.value {
            ValueType::Bool(value) => Ok(*value),
            ValueType::Int(value) => Ok(*value != 0),
            ValueType::Float(value) => Ok(*value != 0.0),
            ValueType::String(StringValue { string, .. }) => Ok(!string.is_empty()),
            ValueType::List(value) => Ok(!value.items.is_empty()),
            ValueType::DivertTarget(_) | ValueType::VariablePointer(_) => Err(
                StoryError::InvalidStoryState("cannot use a target as a condition".to_owned()),
            ),
        }
    }

    pub(crate) fn choose_path(&mut self, path: &Path) -> Result<(), StoryError> {
        let pointer = self.data.pointer_at_path(path).ok_or_else(|| {
            StoryError::BadArgument(format!("story path '{path}' does not resolve"))
        })?;
        let previous = self.callstack.borrow().current_pointer();
        self.record_entered_ancestors(previous, pointer);
        if let Some(container) = pointer.container.filter(|_| pointer.index < 0) {
            let flags = match self.data.node_view(container.node()).unwrap().kind {
                NodeKindView::Container { count_flags } => count_flags,
                _ => 0,
            };
            if flags & 1 != 0 {
                self.counters.record_visit(container);
            }
            if flags & 2 != 0 {
                self.counters
                    .record_turn(container, self.current_turn_index);
            }
            self.previsited_container = Some(container);
        }
        self.callstack.borrow_mut().set_current_pointer(pointer);
        Ok(())
    }

    fn record_entered_ancestors(&mut self, previous: FlatPointer, target: FlatPointer) {
        let Some(target_id) = target.resolve(&self.data) else {
            return;
        };
        let mut previous_ancestors = Vec::new();
        let mut old = previous.resolve(&self.data);
        while let Some(id) = old {
            previous_ancestors.push(id);
            old = self.data.node_view(id).and_then(|node| node.parent);
        }
        let mut child = target_id;
        let mut entered_at_start = true;
        while let Some(parent) = self.data.node_view(child).and_then(|node| node.parent) {
            let child_index = self.data.node_view(child).and_then(|node| node.child_index);
            entered_at_start &= child_index == Some(0);
            if previous_ancestors.contains(&parent) {
                break;
            }
            if let NodeKindView::Container { count_flags } =
                self.data.node_view(parent).unwrap().kind
                && (count_flags & 4 == 0 || entered_at_start)
            {
                let container = self.data.container_id(parent).unwrap();
                if count_flags & 1 != 0 {
                    self.counters.record_visit(container);
                }
                if count_flags & 2 != 0 {
                    self.counters
                        .record_turn(container, self.current_turn_index);
                }
            }
            child = parent;
        }
    }

    pub(crate) fn current_node(&self) -> Option<NodeId> {
        self.callstack
            .borrow()
            .current_pointer()
            .resolve(&self.data)
    }

    pub(crate) fn current_path_string(&self) -> Option<String> {
        self.callstack
            .borrow()
            .current_pointer()
            .path(&self.data)
            .map(|path| path.to_string())
    }

    pub(crate) fn visit_count_at_path_string(&self, path: &str) -> Result<i32, StoryError> {
        let path = Path::new_with_components_string(Some(path));
        let container = self
            .data
            .resolve_path(self.data.root(), &path)
            .and_then(|id| self.data.container_id(id))
            .ok_or_else(|| {
                StoryError::BadArgument(format!("story path '{path}' does not resolve"))
            })?;
        Ok(self.counters.visit_count(container))
    }

    pub(crate) fn current_tags(&self) -> &[String] {
        &self.current_tags
    }

    pub(crate) fn public_choices(&self) -> Vec<Rc<Choice>> {
        if self.can_continue() {
            return Vec::new();
        }
        self.choices
            .iter()
            .filter(|choice| !choice.is_invisible_default)
            .enumerate()
            .map(|(index, choice)| {
                let target = self
                    .data
                    .canonical_path_text(choice.target.node())
                    .unwrap_or_default();
                let source = self
                    .data
                    .canonical_path_text(choice.source)
                    .unwrap_or_default();
                Rc::new(Choice::new_from_json(
                    &target,
                    source,
                    &choice.text,
                    index,
                    choice.thread.thread_index,
                    choice.tags.clone(),
                ))
            })
            .collect()
    }

    pub(crate) fn build_string_of_hierarchy(&self) -> String {
        fn append(
            data: &impl StaticStoryView,
            id: NodeId,
            current: Option<NodeId>,
            depth: usize,
            text: &mut String,
        ) {
            let Some(node) = data.node_view(id) else {
                return;
            };
            for _ in 0..depth {
                text.push_str("  ");
            }
            text.push_str(&format!("{:?}", node.kind));
            if Some(id) == current {
                text.push_str(" <---");
            }
            text.push('\n');
            if let Some(count) = data.child_count(id) {
                for index in 0..count {
                    let child = data.child_at(id, index).unwrap();
                    append(data, child, current, depth + 1, text);
                }
                if let Some(named_count) = data.named_child_count(id) {
                    for index in 0..named_count {
                        let entry = data.named_child_at(id, index).unwrap();
                        if data
                            .node_view(entry.node)
                            .is_some_and(|child| child.child_index.is_none())
                        {
                            append(data, entry.node, current, depth + 1, text);
                        }
                    }
                }
            }
        }
        let mut text = String::new();
        let current = self
            .callstack
            .borrow()
            .current_pointer()
            .resolve(&self.data);
        append(&*self.data, self.data.root(), current, 0, &mut text);
        text
    }

    pub(crate) fn current_text(&self) -> String {
        StoryState::clean_output_whitespace(&self.output)
    }

    pub(crate) fn switch_flow(&mut self, name: &str) {
        if name == self.current_flow_name {
            return;
        }
        let root = self.data.container_id(self.data.root()).unwrap();
        let next = self
            .named_flows
            .remove(name)
            .unwrap_or_else(|| FlatFlow::new(root));
        let previous = FlatFlow {
            callstack: core::mem::replace(&mut self.callstack, next.callstack),
            output: core::mem::replace(&mut self.output, next.output),
            choices: core::mem::replace(&mut self.choices, next.choices),
            current_tags: core::mem::replace(&mut self.current_tags, next.current_tags),
        };
        self.variables.set_flat_callstack(self.callstack.clone());
        let previous_name = core::mem::replace(&mut self.current_flow_name, name.to_owned());
        self.named_flows.insert(previous_name, previous);
    }

    pub(crate) fn remove_flow(&mut self, name: &str) -> Result<(), StoryError> {
        if name == "DEFAULT_FLOW" {
            return Err(StoryError::BadArgument(
                "Cannot destroy default flow".to_owned(),
            ));
        }
        if name == self.current_flow_name {
            self.switch_to_default_flow();
        }
        self.named_flows.remove(name);
        Ok(())
    }

    pub(crate) fn switch_to_default_flow(&mut self) {
        self.switch_flow("DEFAULT_FLOW");
    }

    pub(crate) fn choose_path_string(
        &mut self,
        path: &str,
        increment_turn: bool,
    ) -> Result<(), StoryError> {
        self.choose_path(&Path::new_with_components_string(Some(path)))?;
        self.choices.clear();
        if increment_turn {
            self.current_turn_index += 1;
        }
        Ok(())
    }

    pub(crate) fn choose_path_string_with_arguments(
        &mut self,
        path: &str,
        reset_call_stack: bool,
        args: Option<&Vec<ValueType>>,
    ) -> Result<(), StoryError> {
        if reset_call_stack {
            self.callstack.borrow_mut().reset();
            self.choices.clear();
        } else if self.callstack.borrow().current_element().push_pop_type == PushPopType::Function {
            return Err(StoryError::InvalidStoryState(format!(
                "Story was running a function when you called ChoosePathString({path})"
            )));
        }
        if let Some(args) = args {
            for value in args {
                if matches!(
                    value,
                    ValueType::DivertTarget(_) | ValueType::VariablePointer(_)
                ) {
                    return Err(StoryError::InvalidStoryState(
                        "ink arguments when calling EvaluateFunction / ChoosePathStringWithParameters must be int, float, string, bool or InkList.".to_owned()
                    ));
                }
                self.push_evaluation(Rc::new(Value::new_value_type(value.clone())));
            }
        }
        self.choose_path_string(path, true)
    }

    pub(crate) fn get_variable(&self, name: &str) -> Option<ValueType> {
        self.variables.get(name)
    }

    pub(crate) fn global_variable_exists(&self, name: &str) -> bool {
        self.variables.global_variable_exists_with_name(name)
    }

    pub(crate) fn list_from_origin(&self, origin_name: &str) -> Result<InkList, StoryError> {
        let definition = self
            .data
            .list_definitions
            .iter()
            .find(|item| item.name == origin_name)
            .ok_or_else(|| {
                StoryError::BadArgument(format!("List origin '{origin_name}' does not exist."))
            })?;
        let list = InkList::new();
        list.set_initial_origin_names(vec![origin_name.to_owned()]);
        let values: HashMap<_, _> = definition.items.iter().cloned().collect();
        list.origins
            .borrow_mut()
            .push(ListDefinition::new(origin_name.to_owned(), values));
        Ok(list)
    }

    pub(crate) fn list_from_item(&self, full_item_name: &str) -> Result<InkList, StoryError> {
        let item = InkListItem::from_full_name(full_item_name);
        let origin_name = item.get_origin_name().ok_or_else(|| {
            StoryError::BadArgument(format!(
                "List item '{full_item_name}' must use the 'origin.item' format."
            ))
        })?;
        let mut list = self.list_from_origin(origin_name)?;
        let value = self
            .data
            .list_definitions
            .iter()
            .find(|def| def.name == *origin_name)
            .and_then(|def| {
                def.items
                    .iter()
                    .find(|(name, _)| name == item.get_item_name())
            })
            .map(|(_, value)| *value)
            .ok_or_else(|| {
                StoryError::BadArgument(format!("List item '{full_item_name}' does not exist."))
            })?;
        list.items.insert(item, value);
        Ok(list)
    }

    pub(crate) fn tags_for_content_at_path(&self, path: &str) -> Result<Vec<String>, StoryError> {
        let path = Path::new_with_components_string(Some(path));
        let mut container = self
            .data
            .resolve_path(self.data.root(), &path)
            .ok_or_else(|| {
                StoryError::BadArgument(format!("story path '{path}' does not resolve"))
            })?;
        if self.data.container_id(container).is_none() {
            return Err(StoryError::BadArgument(format!(
                "story path '{path}' is not a container"
            )));
        }
        while let Some(first) = self.data.child_at(container, 0) {
            if self.data.container_id(first).is_some() {
                container = first;
            } else {
                break;
            }
        }
        let mut tags = Vec::new();
        let mut in_tag = false;
        for index in 0..self.data.child_count(container).unwrap_or(0) {
            let id = self.data.child_at(container, index).unwrap();
            match self.data.node_view(id).unwrap().kind {
                NodeKindView::ControlCommand(CommandType::BeginTag) => in_tag = true,
                NodeKindView::ControlCommand(CommandType::EndTag) => in_tag = false,
                NodeKindView::Value(ValueView::String(text)) if in_tag => {
                    tags.push(text.to_owned())
                }
                NodeKindView::Value(_) if in_tag => return Err(StoryError::InvalidStoryState(
                    "Tag contained non-text content. Only plain text is allowed when using globalTags or TagsAtContentPath. If you want to evaluate dynamic content, you need to use story.Continue()".to_owned()
                )),
                _ if !in_tag => break,
                _ => {}
            }
        }
        Ok(tags)
    }

    pub(crate) fn evaluate_function(
        &mut self,
        name: &str,
        args: Option<&Vec<ValueType>>,
        text_output: &mut String,
    ) -> Result<Option<ValueType>, StoryError> {
        if name.trim().is_empty() {
            return Err(StoryError::InvalidStoryState(
                "Function is empty or white space.".to_owned(),
            ));
        }
        let target = self
            .data
            .named_child(self.data.root(), name)
            .and_then(|id| self.data.container_id(id))
            .ok_or_else(|| StoryError::BadArgument(format!("Function doesn't exist: '{name}'")))?;
        let output_before = core::mem::take(&mut self.output);
        let tags_before = core::mem::take(&mut self.current_tags);
        self.callstack.borrow_mut().push(
            PushPopType::FunctionEvaluationFromGame,
            self.evaluation_stack.len(),
            0,
        );
        self.callstack
            .borrow_mut()
            .set_current_pointer(FlatPointer::start_of(target));
        if let Some(args) = args {
            for value in args {
                if matches!(
                    value,
                    ValueType::DivertTarget(_) | ValueType::VariablePointer(_)
                ) {
                    return Err(StoryError::InvalidStoryState(
                        "ink arguments when calling EvaluateFunction / ChoosePathStringWithParameters must be int, float, string, bool or InkList.".to_owned()
                    ));
                }
                self.push_evaluation(Rc::new(Value::new_value_type(value.clone())));
            }
        }
        let mut changes = HashMap::new();
        while self.can_continue() {
            text_output.push_str(&self.cont()?);
            changes.extend(self.take_changed_variables());
        }
        self.changed_variables = changes;
        self.output = output_before;
        self.current_tags = tags_before;
        let height = self
            .callstack
            .borrow()
            .current_element()
            .evaluation_stack_height_when_pushed;
        let result = self.evaluation_stack.pop();
        self.evaluation_stack.truncate(height);
        self.callstack
            .borrow_mut()
            .pop(Some(PushPopType::FunctionEvaluationFromGame))?;
        Ok(result.and_then(|value| {
            value
                .as_any()
                .downcast_ref::<Value>()
                .map(|value| match &value.value {
                    ValueType::DivertTarget(path) => ValueType::new::<&str>(&path.to_string()),
                    other => other.clone(),
                })
        }))
    }

    pub(crate) fn set_variable(&mut self, name: &str, value: &ValueType) -> Result<(), StoryError> {
        if self.variables.set(name, value.clone())? {
            self.changed_variables
                .insert(name.to_owned(), value.clone());
        }
        Ok(())
    }

    pub(crate) fn take_changed_variables(&mut self) -> HashMap<String, ValueType> {
        core::mem::take(&mut self.changed_variables)
    }

    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    fn write_flow_json(
        &self,
        stack: &FlatCallStack,
        output: &str,
        tags: &[String],
        choices: &[FlatChoice],
    ) -> Result<serde_json::Value, StoryError> {
        let mut flow = Map::new();
        flow.insert("callstack".to_owned(), stack.write_json(&self.data)?);
        let mut output_stream: Vec<Rc<dyn RTObject>> = Vec::new();
        if !output.is_empty() {
            for segment in output.split_inclusive('\n') {
                let text = segment.strip_suffix('\n').unwrap_or(segment);
                if !text.is_empty() {
                    output_stream.push(Rc::new(Value::new(text)));
                }
                if segment.ends_with('\n') {
                    output_stream.push(Rc::new(Value::new("\n")));
                }
            }
        }
        for tag in tags {
            output_stream.push(Rc::new(ControlCommand::new(CommandType::BeginTag)));
            output_stream.push(Rc::new(Value::new(tag.as_str())));
            output_stream.push(Rc::new(ControlCommand::new(CommandType::EndTag)));
        }
        flow.insert(
            "outputStream".to_owned(),
            json_write::write_list_rt_objs(&output_stream)?,
        );
        let mut saved_choices = Vec::new();
        let mut choice_threads = Map::new();
        for (index, choice) in choices.iter().enumerate() {
            let mut saved = Map::new();
            saved.insert("text".to_owned(), json!(choice.text));
            saved.insert("index".to_owned(), json!(index));
            saved.insert(
                "originalChoicePath".to_owned(),
                json!(
                    self.data
                        .canonical_path_text(choice.source)
                        .ok_or_else(|| StoryError::InvalidStoryState(
                            "choice has invalid source".to_owned()
                        ))?
                ),
            );
            saved.insert(
                "originalThreadIndex".to_owned(),
                json!(choice.thread.thread_index),
            );
            saved.insert(
                "targetPath".to_owned(),
                json!(
                    self.data
                        .canonical_path_text(choice.target.node())
                        .ok_or_else(|| StoryError::InvalidStoryState(
                            "choice has invalid target".to_owned()
                        ))?
                ),
            );
            saved.insert("tags".to_owned(), json!(choice.tags));
            saved_choices.push(serde_json::Value::Object(saved));
            choice_threads.insert(
                choice.thread.thread_index.to_string(),
                choice.thread.write_json(&self.data)?,
            );
        }
        flow.insert(
            "currentChoices".to_owned(),
            serde_json::Value::Array(saved_choices),
        );
        if !choice_threads.is_empty() {
            flow.insert(
                "choiceThreads".to_owned(),
                serde_json::Value::Object(choice_threads),
            );
        }
        Ok(serde_json::Value::Object(flow))
    }

    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    pub(crate) fn save_state_json(&self) -> Result<String, StoryError> {
        let mut state = Map::new();
        let mut flows = Map::new();
        flows.insert(
            self.current_flow_name.clone(),
            self.write_flow_json(
                &self.callstack.borrow(),
                &self.output,
                &self.current_tags,
                &self.choices,
            )?,
        );
        for (name, flow) in &self.named_flows {
            flows.insert(
                name.clone(),
                self.write_flow_json(
                    &flow.callstack.borrow(),
                    &flow.output,
                    &flow.current_tags,
                    &flow.choices,
                )?,
            );
        }
        state.insert("flows".to_owned(), serde_json::Value::Object(flows));
        state.insert("currentFlowName".to_owned(), json!(self.current_flow_name));
        state.insert("variablesState".to_owned(), self.variables.write_json()?);
        state.insert(
            "evalStack".to_owned(),
            json_write::write_list_rt_objs(&self.evaluation_stack)?,
        );
        if !self.diverted.is_null() {
            state.insert(
                "currentDivertTarget".to_owned(),
                json!(
                    self.diverted
                        .path(&self.data)
                        .ok_or_else(|| StoryError::InvalidStoryState(
                            "divert has invalid path".to_owned()
                        ))?
                        .to_string()
                ),
            );
        }
        state.insert(
            "visitCounts".to_owned(),
            json!(self.counters.visit_paths_for_save(&self.data)?),
        );
        state.insert(
            "turnIndices".to_owned(),
            json!(self.counters.turn_paths_for_save(&self.data)?),
        );
        state.insert("turnIdx".to_owned(), json!(self.current_turn_index));
        state.insert("storySeed".to_owned(), json!(self.story_seed));
        state.insert("previousRandom".to_owned(), json!(self.previous_random));
        state.insert("inkSaveVersion".to_owned(), json!(INK_SAVE_STATE_VERSION));
        state.insert(
            "inkFormatVersion".to_owned(),
            json!(crate::story::INK_VERSION_CURRENT),
        );
        serde_json::to_string(&state).map_err(|error| StoryError::BadJson(error.to_string()))
    }

    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    fn load_flow_json(&self, encoded: &serde_json::Value) -> Result<FlatFlow, StoryError> {
        let object = encoded
            .as_object()
            .ok_or_else(|| StoryError::BadJson("flow must be an object".to_owned()))?;
        let root = self.data.container_id(self.data.root()).unwrap();
        let mut flow = FlatFlow::new(root);
        flow.callstack.borrow_mut().load_json(
            &self.data,
            object
                .get("callstack")
                .ok_or_else(|| StoryError::BadJson("flow callstack not found".to_owned()))?,
        )?;
        let output = object
            .get("outputStream")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| StoryError::BadJson("outputStream not found".to_owned()))?;
        let output = json_read::jarray_to_runtime_obj_list(output, false)?;
        let mut tag = None::<String>;
        for item in output {
            if let Some(command) = item.as_any().downcast_ref::<ControlCommand>() {
                match command.command_type {
                    CommandType::BeginTag => tag = Some(String::new()),
                    CommandType::EndTag => {
                        if let Some(value) = tag.take() {
                            flow.current_tags
                                .push(StoryState::clean_output_whitespace(&value));
                        }
                    }
                    _ => {}
                }
            } else if let Some(text) = Value::get_value::<&StringValue>(item.as_ref()) {
                if let Some(tag) = tag.as_mut() {
                    tag.push_str(&text.string);
                } else {
                    flow.output.push_str(&text.string);
                }
            } else if let Some(static_tag) = item.as_any().downcast_ref::<Tag>() {
                flow.current_tags.push(static_tag.get_text().clone());
            }
        }
        let saved_threads = object
            .get("choiceThreads")
            .and_then(serde_json::Value::as_object);
        let choices = object
            .get("currentChoices")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| StoryError::BadJson("currentChoices not found".to_owned()))?;
        for encoded in choices {
            let saved = encoded
                .as_object()
                .ok_or_else(|| StoryError::BadJson("choice must be an object".to_owned()))?;
            let get_str = |key: &str| {
                saved
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| StoryError::BadJson(format!("choice {key} not found")))
            };
            let target_path = Path::new_with_components_string(Some(get_str("targetPath")?));
            let source_path =
                Path::new_with_components_string(Some(get_str("originalChoicePath")?));
            let target = self
                .data
                .resolve_path(self.data.root(), &target_path)
                .and_then(|id| self.data.container_id(id))
                .ok_or_else(|| StoryError::BadJson("choice target does not resolve".to_owned()))?;
            let source = self
                .data
                .resolve_path(self.data.root(), &source_path)
                .ok_or_else(|| StoryError::BadJson("choice source does not resolve".to_owned()))?;
            let thread_index = saved
                .get("originalThreadIndex")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| StoryError::BadJson("choice thread index not found".to_owned()))?
                as usize;
            let thread =
                if let Some(thread) = flow.callstack.borrow().get_thread_with_index(thread_index) {
                    thread.clone()
                } else {
                    let encoded = saved_threads
                        .and_then(|threads| threads.get(&thread_index.to_string()))
                        .ok_or_else(|| {
                            StoryError::BadJson(format!(
                                "choice references missing thread {thread_index}"
                            ))
                        })?;
                    FlatThread::from_json(&self.data, encoded)?
                };
            let tags = saved
                .get("tags")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| StoryError::BadJson("choice tags not found".to_owned()))?
                .iter()
                .map(|tag| {
                    tag.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| StoryError::BadJson("choice tag must be text".to_owned()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            flow.choices.push(FlatChoice {
                target,
                source,
                is_invisible_default: false,
                text: get_str("text")?.to_owned(),
                tags,
                thread,
            });
        }
        Ok(flow)
    }

    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    pub(crate) fn load_state_json(&mut self, saved: &str) -> Result<(), StoryError> {
        let encoded: serde_json::Value =
            serde_json::from_str(saved).map_err(|error| StoryError::BadJson(error.to_string()))?;
        let object = encoded
            .as_object()
            .ok_or_else(|| StoryError::BadJson("save state must be an object".to_owned()))?;
        let version = object
            .get("inkSaveVersion")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                StoryError::BadJson("ink save format incorrect, can't load.".to_owned())
            })?;
        if version < crate::story_state::MIN_COMPATIBLE_LOAD_VERSION as u64 {
            return Err(StoryError::BadJson(format!(
                "Ink save format isn't compatible with the current version (saw '{version}', but minimum is {}), so can't load.",
                crate::story_state::MIN_COMPATIBLE_LOAD_VERSION
            )));
        }
        let flows = object
            .get("flows")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| StoryError::BadJson("flows not found".to_owned()))?;
        let current_name = object
            .get("currentFlowName")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| StoryError::BadJson("currentFlowName not found".to_owned()))?;
        let mut parsed_flows = HashMap::new();
        for (name, flow) in flows {
            parsed_flows.insert(name.clone(), self.load_flow_json(flow)?);
        }
        let current = parsed_flows
            .remove(current_name)
            .ok_or_else(|| StoryError::BadJson("current flow not found".to_owned()))?;
        let mut loaded = self.snapshot();
        loaded.current_flow_name = current_name.to_owned();
        loaded.named_flows = parsed_flows;
        loaded.callstack = current.callstack;
        loaded.output = current.output;
        loaded.current_tags = current.current_tags;
        loaded.choices = current.choices;
        loaded
            .variables
            .set_flat_callstack(loaded.callstack.clone());
        loaded.variables.load_json(
            object
                .get("variablesState")
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| StoryError::BadJson("variablesState not found".to_owned()))?,
        )?;
        loaded.evaluation_stack = json_read::jarray_to_runtime_obj_list(
            object
                .get("evalStack")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| StoryError::BadJson("evalStack not found".to_owned()))?,
            false,
        )?;
        loaded.diverted = object
            .get("currentDivertTarget")
            .and_then(serde_json::Value::as_str)
            .map(|path| {
                loaded
                    .data
                    .pointer_at_path(&Path::new_with_components_string(Some(path)))
                    .ok_or_else(|| {
                        StoryError::BadJson("current divert target does not resolve".to_owned())
                    })
            })
            .transpose()?
            .unwrap_or(FlatPointer::NULL);
        let visits = serde_json::from_value(
            object
                .get("visitCounts")
                .cloned()
                .unwrap_or_else(|| json!({})),
        )
        .map_err(|error| StoryError::BadJson(error.to_string()))?;
        let turns = serde_json::from_value(
            object
                .get("turnIndices")
                .cloned()
                .unwrap_or_else(|| json!({})),
        )
        .map_err(|error| StoryError::BadJson(error.to_string()))?;
        loaded.counters.restore_visit_paths(&loaded.data, &visits)?;
        loaded.counters.restore_turn_paths(&loaded.data, &turns)?;
        loaded.current_turn_index = object
            .get("turnIdx")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(-1) as i32;
        loaded.story_seed = object
            .get("storySeed")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32;
        loaded.previous_random = object
            .get("previousRandom")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32;
        *self = loaded;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::LegacyStory;

    fn compare_first_choice_path(json: &str, context: &str) {
        let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = FlatRuntime::new(data).unwrap();
        for step in 0..10 {
            while legacy.can_continue() && flat.can_continue() {
                match (flat.cont(), legacy.cont()) {
                    (Ok(flat_line), Ok(legacy_line)) => {
                        assert_eq!(
                            flat_line,
                            legacy_line,
                            "{context} at choice cycle {step}, flat turn {}, flat counters {:?}",
                            flat.current_turn_index,
                            flat.counters.turn_paths_for_save(&flat.data).ok()
                        );
                        assert_eq!(
                            flat.current_tags(),
                            legacy.get_current_tags().unwrap(),
                            "{context} tags"
                        );
                    }
                    (Err(flat_error), Err(legacy_error)) => {
                        assert_eq!(
                            flat_error.to_string(),
                            legacy_error.to_string(),
                            "{context}"
                        );
                        return;
                    }
                    (flat_result, legacy_result) => {
                        panic!(
                            "{context}: flat={:?}, legacy={:?}, node={:?}",
                            flat_result.err(),
                            legacy_result.err(),
                            flat.current_node()
                                .and_then(|id| flat.data.canonical_path_text(id))
                        );
                    }
                }
            }
            assert_eq!(flat.can_continue(), legacy.can_continue());
            let choices = legacy.get_current_choices();
            let flat_choices: Vec<_> = flat
                .choices()
                .iter()
                .filter(|choice| !choice.is_invisible_default)
                .collect();
            assert_eq!(flat_choices.len(), choices.len());
            for (flat_choice, choice) in flat_choices.iter().zip(&choices) {
                assert_eq!(flat_choice.text, choice.text);
                assert_eq!(flat_choice.tags, choice.tags);
            }
            if choices.is_empty() {
                return;
            }
            flat.choose_choice_index(0).unwrap();
            legacy.choose_choice_index(0).unwrap();
        }
        // Sticky choices can intentionally repeat without an ending.
    }

    #[test]
    fn flat_runtime_runs_text_divert_and_expression_without_a_tree() {
        let json = r#"{"inkVersion":21,"root":["^Hello ",{"->":"target"},{"target":["ev",1,2,"+","out","/ev","^!\n","done",null]}],"listDefs":{}}"#;
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut runtime = FlatRuntime::new(data).unwrap();
        assert_eq!(runtime.cont().unwrap(), "Hello 3!\n");
        assert!(!runtime.can_continue());
    }

    #[test]
    fn flat_runtime_generates_and_follows_choice_by_id() {
        let json = r#"{"inkVersion":21,"root":["ev","^Option","/ev",{"*":"choice","flg":4},{"choice":["^Selected\n","done",null]}],"listDefs":{}}"#;
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut runtime = FlatRuntime::new(data).unwrap();
        assert_eq!(runtime.cont().unwrap(), "");
        assert!(!runtime.can_continue());
        assert_eq!(runtime.choices().len(), 1);
        assert_eq!(runtime.choices()[0].text, "Option");
        runtime.choose_choice_index(0).unwrap();
        assert_eq!(runtime.cont().unwrap(), "Selected\n");
    }

    #[test]
    fn flat_runtime_matches_legacy_on_simple_divert_fixture() {
        let json = include_str!("../../conformance-tests/inkfiles/divert/simple-divert.ink.json");
        let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = FlatRuntime::new(data).unwrap();
        while legacy.can_continue() && flat.can_continue() {
            assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        }
        assert_eq!(flat.can_continue(), legacy.can_continue());
    }

    #[test]
    fn flat_runtime_matches_legacy_on_single_choice_fixture() {
        let json = include_str!("../../conformance-tests/inkfiles/choices/single-choice.ink.json");
        let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = FlatRuntime::new(data).unwrap();
        while legacy.can_continue() && flat.can_continue() {
            assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        }
        assert_eq!(flat.can_continue(), legacy.can_continue());
        let legacy_choices = legacy.get_current_choices();
        assert_eq!(flat.choices().len(), legacy_choices.len());
        assert_eq!(flat.choices()[0].text, legacy_choices[0].text);
        flat.choose_choice_index(0).unwrap();
        legacy.choose_choice_index(0).unwrap();
        while legacy.can_continue() && flat.can_continue() {
            assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        }
        assert_eq!(flat.can_continue(), legacy.can_continue());
    }

    #[test]
    fn flat_runtime_matches_legacy_on_choice_with_variable_target() {
        let json = include_str!("../../conformance-tests/inkfiles/choices/one.ink.json");
        let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = FlatRuntime::new(data).unwrap();
        while legacy.can_continue() && flat.can_continue() {
            assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        }
        assert_eq!(flat.can_continue(), legacy.can_continue());
        let legacy_choices = legacy.get_current_choices();
        assert_eq!(flat.choices().len(), legacy_choices.len());
        assert_eq!(flat.choices()[0].text, legacy_choices[0].text);
        flat.choose_choice_index(0).unwrap();
        legacy.choose_choice_index(0).unwrap();
        while legacy.can_continue() && flat.can_continue() {
            assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        }
        assert_eq!(flat.can_continue(), legacy.can_continue());
    }

    #[test]
    fn flat_runtime_matches_legacy_on_multiple_choices() {
        let json = include_str!("../../conformance-tests/inkfiles/choices/multi-choice.ink.json");
        let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = FlatRuntime::new(data).unwrap();
        while legacy.can_continue() && flat.can_continue() {
            assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        }
        let legacy_choices = legacy.get_current_choices();
        let flat_choices = flat.choices();
        assert_eq!(flat_choices.len(), legacy_choices.len());
        for (flat_choice, legacy_choice) in flat_choices.iter().zip(&legacy_choices) {
            assert_eq!(flat_choice.text, legacy_choice.text);
        }
        let index = legacy_choices.len() - 1;
        flat.choose_choice_index(index).unwrap();
        legacy.choose_choice_index(index).unwrap();
        while legacy.can_continue() && flat.can_continue() {
            assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        }
        assert_eq!(flat.can_continue(), legacy.can_continue());
    }

    #[test]
    fn flat_runtime_matches_simple_text_and_glue_fixtures() {
        for (name, json) in [
            (
                "twolines",
                include_str!("../../conformance-tests/inkfiles/basictext/twolines.ink.json"),
            ),
            (
                "simple-glue",
                include_str!("../../conformance-tests/inkfiles/glue/simple-glue.ink.json"),
            ),
            (
                "left-right-glue",
                include_str!(
                    "../../conformance-tests/inkfiles/glue/left-right-glue-matching.ink.json"
                ),
            ),
            (
                "glue-with-divert",
                include_str!("../../conformance-tests/inkfiles/glue/glue-with-divert.ink.json"),
            ),
            (
                "glue-bugfix1",
                include_str!("../../conformance-tests/inkfiles/glue/testbugfix1.ink.json"),
            ),
            (
                "glue-bugfix2",
                include_str!("../../conformance-tests/inkfiles/glue/testbugfix2.ink.json"),
            ),
        ] {
            compare_first_choice_path(json, name);
        }
    }

    #[test]
    fn flat_runtime_matches_choice_fixtures() {
        for (name, json) in [
            (
                "conditional-choice",
                include_str!(
                    "../../conformance-tests/inkfiles/choices/conditional-choice.ink.json"
                ),
            ),
            (
                "fallback-choice",
                include_str!("../../conformance-tests/inkfiles/choices/fallback-choice.ink.json"),
            ),
            (
                "no-choice-text",
                include_str!("../../conformance-tests/inkfiles/choices/no-choice-text.ink.json"),
            ),
            (
                "varying-choice",
                include_str!("../../conformance-tests/inkfiles/choices/varying-choice.ink.json"),
            ),
            (
                "sticky-choice",
                include_str!("../../conformance-tests/inkfiles/choices/sticky-choice.ink.json"),
            ),
        ] {
            compare_first_choice_path(json, name);
        }
    }

    #[test]
    fn flat_runtime_matches_count_and_sequence_fixtures() {
        for (name, json) in [
            (
                "read-counts",
                include_str!("../../conformance-tests/inkfiles/misc/read-counts.ink.json"),
            ),
            (
                "turns-since",
                include_str!("../../conformance-tests/inkfiles/misc/turns-since.ink.json"),
            ),
            (
                "shuffle",
                include_str!("../../conformance-tests/inkfiles/conditional/shuffle.ink.json"),
            ),
            (
                "once",
                include_str!("../../conformance-tests/inkfiles/conditional/once.ink.json"),
            ),
        ] {
            compare_first_choice_path(json, name);
        }
    }

    #[test]
    fn flat_runtime_matches_list_fixtures() {
        for (name, json) in [
            (
                "list-range",
                include_str!("../../conformance-tests/inkfiles/lists/list-range.ink.json"),
            ),
            (
                "list-basic",
                include_str!("../../conformance-tests/inkfiles/lists/basic-operations.ink.json"),
            ),
            (
                "list-mixed",
                include_str!("../../conformance-tests/inkfiles/lists/list-mixed-items.ink.json"),
            ),
            (
                "empty-list-origin",
                include_str!("../../conformance-tests/inkfiles/lists/empty-list-origin.ink.json"),
            ),
            (
                "empty-list-origin-after-assignment",
                include_str!(
                    "../../conformance-tests/inkfiles/lists/empty-list-origin-after-assignment.ink.json"
                ),
            ),
            (
                "more-list-operations",
                include_str!(
                    "../../conformance-tests/inkfiles/lists/more-list-operations.ink.json"
                ),
            ),
        ] {
            compare_first_choice_path(json, name);
        }
    }

    #[test]
    fn flat_runtime_matches_tag_fixtures() {
        for (name, json) in [
            (
                "tags",
                include_str!("../../conformance-tests/inkfiles/tags/tags.ink.json"),
            ),
            (
                "tags-in-choice",
                include_str!("../../conformance-tests/inkfiles/tags/tagsInChoice.ink.json"),
            ),
            (
                "tags-in-choice-dynamic",
                include_str!("../../conformance-tests/inkfiles/tags/tagsInChoiceDynamic.ink.json"),
            ),
        ] {
            compare_first_choice_path(json, name);
        }
    }

    #[test]
    fn flat_runtime_matches_variable_and_flow_fixtures() {
        for (name, json) in [
            (
                "varcalc",
                include_str!("../../conformance-tests/inkfiles/variable/varcalc.ink.json"),
            ),
            (
                "var-divert",
                include_str!("../../conformance-tests/inkfiles/variable/var-divert.ink.json"),
            ),
            (
                "param-ints",
                include_str!("../../conformance-tests/inkfiles/knot/param-ints.ink.json"),
            ),
            (
                "sequence-tunnel",
                include_str!("../../conformance-tests/inkfiles/tunnels/sequence-tunnel.ink.json"),
            ),
            (
                "tunnel-override",
                include_str!(
                    "../../conformance-tests/inkfiles/tunnels/tunnel-onwards-divert-override.ink.json"
                ),
            ),
            (
                "thread-bug",
                include_str!("../../conformance-tests/inkfiles/threads/thread-bug.ink.json"),
            ),
            (
                "complex-flow",
                include_str!("../../conformance-tests/inkfiles/gather/complex-flow.ink.json"),
            ),
        ] {
            compare_first_choice_path(json, name);
        }
    }

    #[test]
    fn flat_runtime_matches_the_intercept_first_path() {
        let json = include_str!("../../conformance-tests/inkfiles/TheIntercept.ink.json");
        compare_first_choice_path(json, "TheIntercept");
    }

    #[test]
    fn flat_runtime_matches_external_binding_and_fallback() {
        let json = include_str!(
            "../../conformance-tests/inkfiles/runtime/external-function-2-arg.ink.json"
        );
        for fallback in [false, true] {
            let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
            let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
            let mut flat = FlatRuntime::new(data).unwrap();
            if fallback {
                legacy.set_allow_external_function_fallbacks(true);
                flat.set_allow_external_fallbacks(true);
            } else {
                legacy
                    .bind_external_function(
                        "externalFunction",
                        |_, args| {
                            assert_eq!(args.len(), 2);
                            Ok(Some(ValueType::Float(7.0)))
                        },
                        true,
                    )
                    .unwrap();
                flat.bind_external_function(
                    "externalFunction",
                    |_: &str, args: &[ValueType]| {
                        assert_eq!(args.len(), 2);
                        Ok(Some(ValueType::Float(7.0)))
                    },
                    true,
                )
                .unwrap();
            }
            assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        }
    }

    #[test]
    fn flat_runtime_defers_unsafe_external_until_after_newline() {
        let json = r#"{"inkVersion":21,"root":["^First\n","ev",{"x()":"externalFunction","exArgs":0},"out","/ev","^\n","done",null],"listDefs":{}}"#;
        let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = FlatRuntime::new(data).unwrap();
        let legacy_calls = Rc::new(RefCell::new(0));
        let flat_calls = Rc::new(RefCell::new(0));
        let observed = legacy_calls.clone();
        legacy
            .bind_external_function(
                "externalFunction",
                move |_, _| {
                    *observed.borrow_mut() += 1;
                    Ok(Some(ValueType::new::<&str>("Second")))
                },
                false,
            )
            .unwrap();
        let observed = flat_calls.clone();
        flat.bind_external_function(
            "externalFunction",
            move |_: &str, _: &[ValueType]| {
                *observed.borrow_mut() += 1;
                Ok(Some(ValueType::new::<&str>("Second")))
            },
            false,
        )
        .unwrap();
        assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        assert_eq!(*flat_calls.borrow(), 0);
        assert_eq!(*legacy_calls.borrow(), 0);
        assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        assert_eq!(*flat_calls.borrow(), 1);
        assert_eq!(*legacy_calls.borrow(), 1);
    }

    #[test]
    fn flat_runtime_keeps_independent_flows() {
        let json =
            include_str!("../../conformance-tests/inkfiles/runtime/multiflow-basics.ink.json");
        let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = FlatRuntime::new(data).unwrap();
        for (flow, path) in [("First", "knot1"), ("Second", "knot2")] {
            legacy.switch_flow(flow).unwrap();
            flat.switch_flow(flow);
            legacy.choose_path_string(path, true, None).unwrap();
            flat.choose_path_string(path, true).unwrap();
            assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        }
        for flow in ["First", "Second"] {
            legacy.switch_flow(flow).unwrap();
            flat.switch_flow(flow);
            assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        }
        flat.remove_flow("Second").unwrap();
        legacy.remove_flow("Second").unwrap();
        assert_eq!(flat.current_flow_name, "DEFAULT_FLOW");
    }

    #[test]
    fn flat_runtime_reads_and_writes_global_variables() {
        let json =
            include_str!("../../conformance-tests/inkfiles/runtime/set-get-variables.ink.json");
        let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = FlatRuntime::new(data).unwrap();
        for name in ["x", "y", "z"] {
            assert!(
                flat.get_variable(name) == legacy.get_variable(name),
                "{name}"
            );
        }
        if let Some(value) = legacy.get_variable("x") {
            legacy.set_variable("x", &value).unwrap();
            flat.set_variable("x", &value).unwrap();
            assert!(flat.get_variable("x") == legacy.get_variable("x"));
        }
    }

    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    #[test]
    fn flat_callstack_uses_legacy_save_shape() {
        let json = include_str!("../../conformance-tests/inkfiles/choices/single-choice.ink.json");
        let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = FlatRuntime::new(data).unwrap();
        legacy.cont().unwrap();
        flat.cont().unwrap();
        let legacy_stack = legacy
            .get_state()
            .get_callstack()
            .borrow()
            .write_json()
            .unwrap();
        let flat_stack = flat.callstack.borrow().write_json(&flat.data).unwrap();
        assert_eq!(flat_stack, legacy_stack);
        let root = flat.data.container_id(flat.data.root()).unwrap();
        let mut loaded = FlatCallStack::new(root);
        loaded.load_json(&flat.data, &legacy_stack).unwrap();
        assert_eq!(loaded.write_json(&flat.data).unwrap(), legacy_stack);
    }

    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    #[test]
    fn flat_state_uses_legacy_save_shape() {
        let json = include_str!("../../conformance-tests/inkfiles/choices/single-choice.ink.json");
        let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = FlatRuntime::new(data).unwrap();
        legacy.cont().unwrap();
        flat.cont().unwrap();
        let legacy_saved: serde_json::Value =
            serde_json::from_str(&legacy.save_state().unwrap()).unwrap();
        let flat_saved: serde_json::Value =
            serde_json::from_str(&flat.save_state_json().unwrap()).unwrap();
        assert_eq!(flat_saved, legacy_saved);
        let legacy_json = legacy.save_state().unwrap();
        let flat_json = flat.save_state_json().unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut restored_flat = FlatRuntime::new(data).unwrap();
        restored_flat.load_state_json(&legacy_json).unwrap();
        let mut restored_legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        restored_legacy.load_state(&flat_json).unwrap();
        restored_flat.choose_choice_index(0).unwrap();
        restored_legacy.choose_choice_index(0).unwrap();
        assert_eq!(
            restored_flat.cont().unwrap(),
            restored_legacy.cont().unwrap()
        );
    }

    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    #[test]
    fn flat_state_roundtrips_multiple_flows_and_threads() {
        let json = include_str!(
            "../../conformance-tests/inkfiles/runtime/multiflow-saveloadthreads.ink.json"
        );
        let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = FlatRuntime::new(data).unwrap();
        assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        for (flow, path) in [("Blue Flow", "blue"), ("Red Flow", "red")] {
            legacy.switch_flow(flow).unwrap();
            flat.switch_flow(flow);
            legacy.choose_path_string(path, true, None).unwrap();
            flat.choose_path_string(path, true).unwrap();
            assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
            while flat.can_continue() && legacy.can_continue() {
                assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
            }
            assert_eq!(
                flat.choices().len(),
                legacy.get_current_choices().len(),
                "before save {flow}"
            );
            assert_eq!(flat.choices().len(), 2, "expected two choices in {flow}");
        }
        let flat_json = flat.save_state_json().unwrap();
        let legacy_json = legacy.save_state().unwrap();
        // The legacy serializer can retain a stale duplicate of the active
        // flow after speculative continuation. The flat state keeps the
        // active flow out of the inactive-flow map, preserving its choices.
        let flat_value: serde_json::Value = serde_json::from_str(&flat_json).unwrap();
        assert_eq!(
            flat_value["flows"]["Red Flow"]["currentChoices"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut restored_flat = FlatRuntime::new(data).unwrap();
        restored_flat.load_state_json(&flat_json).unwrap();
        assert_eq!(
            restored_flat.choices().len(),
            2,
            "after loading flat red state"
        );
        let mut restored_legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        restored_legacy.load_state(&flat_json).unwrap();
        for flow in ["Red Flow", "Blue Flow"] {
            restored_flat.switch_flow(flow);
            restored_legacy.switch_flow(flow).unwrap();
            assert_eq!(
                restored_flat.choices().len(),
                restored_legacy.get_current_choices().len(),
                "{flow}"
            );
            restored_flat.choose_choice_index(0).unwrap();
            restored_legacy.choose_choice_index(0).unwrap();
            while restored_flat.can_continue() && restored_legacy.can_continue() {
                assert_eq!(
                    restored_flat.cont().unwrap(),
                    restored_legacy.cont().unwrap()
                );
            }
        }
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut from_legacy = FlatRuntime::new(data).unwrap();
        from_legacy.load_state_json(&legacy_json).unwrap();
        from_legacy.switch_flow("Blue Flow");
        assert_eq!(from_legacy.choices().len(), 2);
    }

    #[cfg(feature = "stream-json-parser")]
    #[test]
    fn flat_stream_state_loads_in_legacy_runtime() {
        let json = include_str!("../../conformance-tests/inkfiles/choices/single-choice.ink.json");
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = FlatRuntime::new(data).unwrap();
        flat.cont().unwrap();
        let saved = flat.save_state_json().unwrap();
        let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        legacy.load_state(&saved).unwrap();
        assert_eq!(flat.choices().len(), legacy.get_current_choices().len());
        flat.choose_choice_index(0).unwrap();
        legacy.choose_choice_index(0).unwrap();
        assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());

        let mut legacy = LegacyStory::new_with_seed(json, 1).unwrap();
        legacy.cont().unwrap();
        let legacy_saved = legacy.save_state().unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut restored = FlatRuntime::new(data).unwrap();
        restored.load_state_json(&legacy_saved).unwrap();
        assert_eq!(restored.choices().len(), legacy.get_current_choices().len());
        restored.choose_choice_index(0).unwrap();
        legacy.choose_choice_index(0).unwrap();
        assert_eq!(restored.cont().unwrap(), legacy.cont().unwrap());
    }

    #[cfg(feature = "stream-json-parser")]
    #[test]
    fn flat_stream_state_roundtrips_multiple_flows_and_threads() {
        let json = include_str!(
            "../../conformance-tests/inkfiles/runtime/multiflow-saveloadthreads.ink.json"
        );
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = FlatRuntime::new(data).unwrap();
        flat.cont().unwrap();
        for (flow, path) in [("Blue Flow", "blue"), ("Red Flow", "red")] {
            flat.switch_flow(flow);
            flat.choose_path_string(path, true).unwrap();
            while flat.can_continue() {
                flat.cont().unwrap();
            }
            assert_eq!(flat.choices().len(), 2);
        }
        let saved = flat.save_state_json().unwrap();
        let (_, data) = FlatStoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut restored = FlatRuntime::new(data).unwrap();
        restored.load_state_from_reader(saved.as_bytes()).unwrap();
        for flow in ["Red Flow", "Blue Flow"] {
            restored.switch_flow(flow);
            assert_eq!(restored.choices().len(), 2, "{flow}");
            restored.choose_choice_index(1).unwrap();
            let mut text = String::new();
            while restored.can_continue() {
                text.push_str(&restored.cont().unwrap());
            }
            assert!(text.contains("Thread 2"), "{flow}: {text}");
        }
    }
}
