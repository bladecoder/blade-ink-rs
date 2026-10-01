//! Ink interpreter that reads static instructions by arena ID.
//! Dynamic values, counters, variables and call frames belong to one run.

#[allow(unused_imports)]
use crate::prelude::*;

use crate::{
    callstack::{CallStack, Thread},
    choice::Choice,
    compat::{cell::RefCell, collections::HashMap, rc::Rc},
    control_command::CommandType,
    ink_list::InkList,
    ink_list_item::InkListItem,
    list_definition::ListDefinition,
    native_function_call::NativeFunctionCall,
    object::RTObject,
    output_text::clean_output_whitespace,
    path::Path,
    push_pop::PushPopType,
    runtime_state::StoryCounters,
    story::external_functions::{ExternalFunction, ExternalFunctionResult},
    story_content::{
        ContainerId, ContentPointer, NodeId, NodeKindView, StaticStoryView, StoryContent,
        StoryData, ValueView,
    },
    story_error::StoryError,
    tag::Tag,
    value::Value,
    value_type::{StringValue, ValueType},
    variable_assignment::VariableAssignment,
    variables_state::VariablesState,
    void::Void,
};
use rand::{RngExt, SeedableRng, rngs::StdRng};

#[cfg(any(feature = "stream-json-parser", feature = "binary-image"))]
#[path = "state_stream.rs"]
mod state_stream;

#[cfg(all(
    not(any(feature = "stream-json-parser", feature = "binary-image")),
    feature = "serde-json-parser"
))]
#[path = "state_serde.rs"]
mod state_serde;

#[derive(Clone)]
pub(crate) struct RuntimeChoice {
    pub(crate) target: ContainerId,
    pub(crate) source: NodeId,
    pub(crate) is_invisible_default: bool,
    pub(crate) text: String,
    pub(crate) tags: Vec<String>,
    pub(crate) thread: Thread,
}

pub(crate) struct Runtime {
    data: Rc<StoryContent>,
    callstack: Rc<RefCell<CallStack>>,
    variables: VariablesState,
    counters: StoryCounters,
    evaluation_stack: Vec<Rc<dyn RTObject>>,
    output: String,
    string_stack: Vec<String>,
    tag_stack: Vec<String>,
    current_tags: Vec<String>,
    glue_active: bool,
    diverted: ContentPointer,
    did_safe_exit: bool,
    choices: Vec<RuntimeChoice>,
    current_turn_index: i32,
    story_seed: i32,
    previous_random: i32,
    externals: Rc<RefCell<HashMap<String, BoundExternal>>>,
    allow_external_fallbacks: bool,
    lookahead_active: bool,
    lookahead_unsafe_external: bool,
    current_flow_name: String,
    named_flows: HashMap<String, RuntimeFlow>,
    changed_variables: HashMap<String, ValueType>,
    previsited_container: Option<ContainerId>,
    continuation_active: bool,
    newline_snapshot: Option<Box<Runtime>>,
}

struct BoundExternal {
    function: Box<dyn ExternalFunction>,
    lookahead_safe: bool,
}

struct RuntimeFlow {
    callstack: Rc<RefCell<CallStack>>,
    output: String,
    choices: Vec<RuntimeChoice>,
    current_tags: Vec<String>,
}

impl RuntimeFlow {
    fn new(root: ContainerId) -> Self {
        Self {
            callstack: Rc::new(RefCell::new(CallStack::new(root))),
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

    fn new_shared(data: Rc<StoryContent>, seed: i32) -> Result<Self, StoryError> {
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
        Ok(Some(clean_output_whitespace(&self.output)))
    }

    fn snapshot(&self) -> Self {
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

    fn step(&mut self) -> Result<(), StoryError> {
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
                    self.diverted = ContentPointer::start_of(target);
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
                if let Some(definition) = self.data.list_definition_index(&name) {
                    let items: HashMap<_, _> = (0..self
                        .data
                        .list_definition_item_count(definition)
                        .unwrap_or(0))
                        .filter_map(|index| self.data.list_definition_item_at(definition, index))
                        .map(|(item, value)| (item.to_owned(), value))
                        .collect();
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
                NodeKindView::Container { count_flags, .. } => count_flags,
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

    fn record_entered_ancestors(&mut self, previous: ContentPointer, target: ContentPointer) {
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
            if let NodeKindView::Container { count_flags, .. } =
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
        clean_output_whitespace(&self.output)
    }

    pub(crate) fn switch_flow(&mut self, name: &str) {
        if name == self.current_flow_name {
            return;
        }
        let root = self.data.container_id(self.data.root()).unwrap();
        let next = self
            .named_flows
            .remove(name)
            .unwrap_or_else(|| RuntimeFlow::new(root));
        let previous = RuntimeFlow {
            callstack: core::mem::replace(&mut self.callstack, next.callstack),
            output: core::mem::replace(&mut self.output, next.output),
            choices: core::mem::replace(&mut self.choices, next.choices),
            current_tags: core::mem::replace(&mut self.current_tags, next.current_tags),
        };
        self.variables.set_callstack(self.callstack.clone());
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
            .list_definition_index(origin_name)
            .ok_or_else(|| {
                StoryError::BadArgument(format!("List origin '{origin_name}' does not exist."))
            })?;
        let list = InkList::new();
        list.set_initial_origin_names(vec![origin_name.to_owned()]);
        let values: HashMap<_, _> = (0..self
            .data
            .list_definition_item_count(definition)
            .unwrap_or(0))
            .filter_map(|index| self.data.list_definition_item_at(definition, index))
            .map(|(name, value)| (name.to_owned(), value))
            .collect();
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
            .list_item(full_item_name)
            .map(|(_, value)| value)
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
            .set_current_pointer(ContentPointer::start_of(target));
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
}

#[cfg(all(
    test,
    any(feature = "stream-json-parser", feature = "serde-json-parser")
))]
mod tests {
    use super::*;

    #[test]
    fn runtime_runs_text_divert_and_expression_without_a_tree() {
        let json = r#"{"inkVersion":21,"root":["^Hello ",{"->":"target"},{"target":["ev",1,2,"+","out","/ev","^!\n","done",null]}],"listDefs":{}}"#;
        let (_, data) = StoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut runtime = Runtime::new(data).unwrap();
        assert_eq!(runtime.cont().unwrap(), "Hello 3!\n");
        assert!(!runtime.can_continue());
    }

    #[test]
    fn runtime_generates_and_follows_choice_by_id() {
        let json = r#"{"inkVersion":21,"root":["ev","^Option","/ev",{"*":"choice","flg":4},{"choice":["^Selected\n","done",null]}],"listDefs":{}}"#;
        let (_, data) = StoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut runtime = Runtime::new(data).unwrap();
        assert_eq!(runtime.cont().unwrap(), "");
        assert!(!runtime.can_continue());
        assert_eq!(runtime.choices().len(), 1);
        assert_eq!(runtime.choices()[0].text, "Option");
        runtime.choose_choice_index(0).unwrap();
        assert_eq!(runtime.cont().unwrap(), "Selected\n");
    }

    #[cfg(feature = "stream-json-parser")]
    #[test]
    fn stream_state_roundtrips_multiple_flows_and_threads() {
        let json = include_str!(
            "../../conformance-tests/inkfiles/runtime/multiflow-saveloadthreads.ink.json"
        );
        let (_, data) = StoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut flat = Runtime::new(data).unwrap();
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
        let (_, data) = StoryData::from_json_reader(json.as_bytes()).unwrap();
        let mut restored = Runtime::new(data).unwrap();
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
