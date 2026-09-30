#[allow(unused_imports)]
use crate::prelude::*;

use crate::compat::{collections::HashMap, io::Read, rc::Rc};

use crate::{
    callstack::{CallStack, Element, Thread},
    choice::Choice,
    container::Container,
    flow::Flow,
    object::RTObject,
    path::Path,
    pointer::{self, Pointer},
    push_pop::PushPopType,
    story::Story,
    story_error::StoryError,
    story_state::{DEFAULT_FLOW_NAME, MIN_COMPATIBLE_LOAD_VERSION, StoryState},
    value::Value,
};

use super::{
    json_read_stream::{read_runtime_object, read_runtime_object_list},
    json_tokenizer::JsonTokenizer,
};

#[derive(Default)]
struct ParsedState {
    version: Option<i32>,
    flows: Option<HashMap<String, Flow>>,
    current_flow_name: Option<String>,
    variables: Option<HashMap<String, Rc<Value>>>,
    evaluation_stack: Option<Vec<Rc<dyn RTObject>>>,
    diverted_path: Option<String>,
    visit_counts: Option<HashMap<String, i32>>,
    turn_indices: Option<HashMap<String, i32>>,
    turn_index: Option<i32>,
    story_seed: Option<i32>,
    previous_random: Option<i32>,
    legacy_threads: Option<Vec<Thread>>,
    legacy_thread_counter: Option<usize>,
    legacy_output: Option<Vec<Rc<dyn RTObject>>>,
    legacy_choices: Option<Vec<Rc<Choice>>>,
    legacy_choice_threads: HashMap<usize, Thread>,
}

fn bad(message: impl Into<String>) -> StoryError {
    StoryError::BadJson(message.into())
}

fn read_i32<R: Read>(tokenizer: &mut JsonTokenizer<R>, field: &str) -> Result<i32, StoryError> {
    tokenizer
        .read_number()?
        .as_integer()
        .ok_or_else(|| bad(format!("{field} must be an integer")))
}

fn read_usize<R: Read>(tokenizer: &mut JsonTokenizer<R>, field: &str) -> Result<usize, StoryError> {
    usize::try_from(read_i32(tokenizer, field)?)
        .map_err(|_| bad(format!("{field} must be non-negative")))
}

fn next_property<R: Read>(tokenizer: &mut JsonTokenizer<R>) -> Result<bool, StoryError> {
    match tokenizer.peek()? {
        '}' => Ok(false),
        ',' => {
            tokenizer.expect(',')?;
            Ok(true)
        }
        found => Err(bad(format!("expected ',' or '}}', found '{found}'"))),
    }
}

fn parse_value_map<R: Read>(
    tokenizer: &mut JsonTokenizer<R>,
) -> Result<HashMap<String, Rc<Value>>, StoryError> {
    tokenizer.expect('{')?;
    let mut values = HashMap::new();
    if tokenizer.peek()? != '}' {
        loop {
            let key = tokenizer.read_obj_key()?;
            let object = read_runtime_object(tokenizer)?;
            let value = object
                .into_any()
                .downcast::<Value>()
                .map_err(|_| bad(format!("variable '{key}' is not a value")))?;
            if values.insert(key.clone(), value).is_some() {
                return Err(bad(format!("duplicate variable '{key}'")));
            }
            if !next_property(tokenizer)? {
                break;
            }
        }
    }
    tokenizer.expect('}')?;
    Ok(values)
}

fn parse_int_map<R: Read>(
    tokenizer: &mut JsonTokenizer<R>,
) -> Result<HashMap<String, i32>, StoryError> {
    tokenizer.expect('{')?;
    let mut values = HashMap::new();
    if tokenizer.peek()? != '}' {
        loop {
            let key = tokenizer.read_obj_key()?;
            let value = read_i32(tokenizer, &key)?;
            if values.insert(key.clone(), value).is_some() {
                return Err(bad(format!("duplicate key '{key}'")));
            }
            if !next_property(tokenizer)? {
                break;
            }
        }
    }
    tokenizer.expect('}')?;
    Ok(values)
}

fn pointer_from_parts(
    root: &Rc<Container>,
    path: Option<&str>,
    index: i32,
) -> Result<Pointer, StoryError> {
    let Some(path) = path else {
        return Ok(pointer::NULL.clone());
    };
    let result = root.content_at_path(&Path::new_with_components_string(Some(path)), 0, -1);
    let container = result
        .container()
        .ok_or_else(|| bad(format!("callstack path '{path}' does not resolve")))?;
    Ok(Pointer::new(Some(container), index))
}

fn parse_element<R: Read>(
    tokenizer: &mut JsonTokenizer<R>,
    root: &Rc<Container>,
) -> Result<Element, StoryError> {
    tokenizer.expect('{')?;
    let mut path = None;
    let mut index = -1;
    let mut expression = false;
    let mut push_type = None;
    let mut temporary_variables = HashMap::new();
    if tokenizer.peek()? != '}' {
        loop {
            let key = tokenizer.read_obj_key()?;
            match key.as_str() {
                "cPath" => path = Some(tokenizer.read_string()?),
                "idx" => index = read_i32(tokenizer, "idx")?,
                "exp" => expression = tokenizer.read_boolean()?,
                "type" => {
                    push_type = Some(PushPopType::from_value(read_usize(tokenizer, "type")?)?)
                }
                "temp" => temporary_variables = parse_value_map(tokenizer)?,
                _ => tokenizer.skip_value()?,
            }
            if !next_property(tokenizer)? {
                break;
            }
        }
    }
    tokenizer.expect('}')?;
    Ok(Element::from_json_parts(
        push_type.ok_or_else(|| bad("callstack element has no type"))?,
        pointer_from_parts(root, path.as_deref(), index)?,
        expression,
        temporary_variables,
    ))
}

fn parse_thread<R: Read>(
    tokenizer: &mut JsonTokenizer<R>,
    root: &Rc<Container>,
) -> Result<Thread, StoryError> {
    tokenizer.expect('{')?;
    let mut callstack = Vec::new();
    let mut thread_index = None;
    let mut previous_pointer = pointer::NULL.clone();
    if tokenizer.peek()? != '}' {
        loop {
            let key = tokenizer.read_obj_key()?;
            match key.as_str() {
                "callstack" => {
                    tokenizer.expect('[')?;
                    while tokenizer.peek()? != ']' {
                        callstack.push(parse_element(tokenizer, root)?);
                        if tokenizer.peek()? != ']' {
                            tokenizer.expect(',')?;
                        }
                    }
                    tokenizer.expect(']')?;
                }
                "threadIndex" => thread_index = Some(read_usize(tokenizer, "threadIndex")?),
                "previousContentObject" => {
                    let path = tokenizer.read_string()?;
                    previous_pointer = Story::pointer_at_path(
                        root,
                        &Path::new_with_components_string(Some(&path)),
                    )?;
                }
                _ => tokenizer.skip_value()?,
            }
            if !next_property(tokenizer)? {
                break;
            }
        }
    }
    tokenizer.expect('}')?;
    Ok(Thread::from_json_parts(
        callstack,
        previous_pointer,
        thread_index.ok_or_else(|| bad("thread has no threadIndex"))?,
    ))
}

fn parse_threads<R: Read>(
    tokenizer: &mut JsonTokenizer<R>,
    root: &Rc<Container>,
) -> Result<Vec<Thread>, StoryError> {
    tokenizer.expect('[')?;
    let mut threads = Vec::new();
    while tokenizer.peek()? != ']' {
        threads.push(parse_thread(tokenizer, root)?);
        if tokenizer.peek()? != ']' {
            tokenizer.expect(',')?;
        }
    }
    tokenizer.expect(']')?;
    Ok(threads)
}

fn parse_callstack<R: Read>(
    tokenizer: &mut JsonTokenizer<R>,
    root: &Rc<Container>,
) -> Result<CallStack, StoryError> {
    tokenizer.expect('{')?;
    let mut threads = None;
    let mut thread_counter = None;
    if tokenizer.peek()? != '}' {
        loop {
            let key = tokenizer.read_obj_key()?;
            match key.as_str() {
                "threads" => threads = Some(parse_threads(tokenizer, root)?),
                "threadCounter" => thread_counter = Some(read_usize(tokenizer, "threadCounter")?),
                _ => tokenizer.skip_value()?,
            }
            if !next_property(tokenizer)? {
                break;
            }
        }
    }
    tokenizer.expect('}')?;
    let mut callstack = CallStack::new(root.clone());
    callstack.load_stream_parts(
        root,
        threads.ok_or_else(|| bad("callstack has no threads"))?,
        thread_counter.ok_or_else(|| bad("callstack has no threadCounter"))?,
    );
    Ok(callstack)
}

fn parse_choice<R: Read>(tokenizer: &mut JsonTokenizer<R>) -> Result<Rc<Choice>, StoryError> {
    tokenizer.expect('{')?;
    let mut text = None;
    let mut index = None;
    let mut source_path = None;
    let mut original_thread_index = None;
    let mut target_path = None;
    let mut tags = Vec::new();
    if tokenizer.peek()? != '}' {
        loop {
            let key = tokenizer.read_obj_key()?;
            match key.as_str() {
                "text" => text = Some(tokenizer.read_string()?),
                "index" => index = Some(read_usize(tokenizer, "index")?),
                "originalChoicePath" => source_path = Some(tokenizer.read_string()?),
                "originalThreadIndex" => {
                    original_thread_index = Some(read_usize(tokenizer, "originalThreadIndex")?)
                }
                "targetPath" => target_path = Some(tokenizer.read_string()?),
                "tags" => {
                    tokenizer.expect('[')?;
                    while tokenizer.peek()? != ']' {
                        tags.push(tokenizer.read_string()?);
                        if tokenizer.peek()? != ']' {
                            tokenizer.expect(',')?;
                        }
                    }
                    tokenizer.expect(']')?;
                }
                _ => tokenizer.skip_value()?,
            }
            if !next_property(tokenizer)? {
                break;
            }
        }
    }
    tokenizer.expect('}')?;
    Ok(Rc::new(Choice::new_from_json(
        &target_path.ok_or_else(|| bad("choice has no targetPath"))?,
        source_path.ok_or_else(|| bad("choice has no originalChoicePath"))?,
        &text.ok_or_else(|| bad("choice has no text"))?,
        index.ok_or_else(|| bad("choice has no index"))?,
        original_thread_index.ok_or_else(|| bad("choice has no originalThreadIndex"))?,
        tags,
    )))
}

fn parse_choices<R: Read>(tokenizer: &mut JsonTokenizer<R>) -> Result<Vec<Rc<Choice>>, StoryError> {
    tokenizer.expect('[')?;
    let mut choices = Vec::new();
    while tokenizer.peek()? != ']' {
        choices.push(parse_choice(tokenizer)?);
        if tokenizer.peek()? != ']' {
            tokenizer.expect(',')?;
        }
    }
    tokenizer.expect(']')?;
    Ok(choices)
}

fn parse_choice_threads<R: Read>(
    tokenizer: &mut JsonTokenizer<R>,
    root: &Rc<Container>,
) -> Result<HashMap<usize, Thread>, StoryError> {
    tokenizer.expect('{')?;
    let mut threads = HashMap::new();
    if tokenizer.peek()? != '}' {
        loop {
            let key = tokenizer.read_obj_key()?;
            let index = key
                .parse::<usize>()
                .map_err(|_| bad(format!("invalid choice thread index '{key}'")))?;
            let thread = parse_thread(tokenizer, root)?;
            if threads.insert(index, thread).is_some() {
                return Err(bad(format!("duplicate choice thread '{key}'")));
            }
            if !next_property(tokenizer)? {
                break;
            }
        }
    }
    tokenizer.expect('}')?;
    Ok(threads)
}

fn parse_flow<R: Read>(
    tokenizer: &mut JsonTokenizer<R>,
    root: &Rc<Container>,
    name: String,
) -> Result<Flow, StoryError> {
    tokenizer.expect('{')?;
    let mut callstack = None;
    let mut output = None;
    let mut choices = None;
    let mut choice_threads = HashMap::new();
    if tokenizer.peek()? != '}' {
        loop {
            let key = tokenizer.read_obj_key()?;
            match key.as_str() {
                "callstack" => callstack = Some(parse_callstack(tokenizer, root)?),
                "outputStream" => output = Some(read_runtime_object_list(tokenizer)?),
                "currentChoices" => choices = Some(parse_choices(tokenizer)?),
                "choiceThreads" => choice_threads = parse_choice_threads(tokenizer, root)?,
                _ => tokenizer.skip_value()?,
            }
            if !next_property(tokenizer)? {
                break;
            }
        }
    }
    tokenizer.expect('}')?;
    Flow::from_stream_parts(
        name,
        callstack.ok_or_else(|| bad("flow has no callstack"))?,
        output.ok_or_else(|| bad("flow has no outputStream"))?,
        choices.ok_or_else(|| bad("flow has no currentChoices"))?,
        choice_threads,
    )
}

fn parse_flows<R: Read>(
    tokenizer: &mut JsonTokenizer<R>,
    root: &Rc<Container>,
) -> Result<HashMap<String, Flow>, StoryError> {
    tokenizer.expect('{')?;
    let mut flows = HashMap::new();
    if tokenizer.peek()? != '}' {
        loop {
            let name = tokenizer.read_obj_key()?;
            let flow = parse_flow(tokenizer, root, name.clone())?;
            if flows.insert(name.clone(), flow).is_some() {
                return Err(bad(format!("duplicate flow '{name}'")));
            }
            if !next_property(tokenizer)? {
                break;
            }
        }
    }
    tokenizer.expect('}')?;
    Ok(flows)
}

fn parse_document<R: Read>(
    tokenizer: &mut JsonTokenizer<R>,
    root: &Rc<Container>,
) -> Result<ParsedState, StoryError> {
    tokenizer.expect('{')?;
    let mut state = ParsedState::default();
    if tokenizer.peek()? != '}' {
        loop {
            let key = tokenizer.read_obj_key()?;
            match key.as_str() {
                "inkSaveVersion" => state.version = Some(read_i32(tokenizer, &key)?),
                "flows" => state.flows = Some(parse_flows(tokenizer, root)?),
                "currentFlowName" => state.current_flow_name = Some(tokenizer.read_string()?),
                "variablesState" => state.variables = Some(parse_value_map(tokenizer)?),
                "evalStack" => state.evaluation_stack = Some(read_runtime_object_list(tokenizer)?),
                "currentDivertTarget" => state.diverted_path = Some(tokenizer.read_string()?),
                "visitCounts" => state.visit_counts = Some(parse_int_map(tokenizer)?),
                "turnIndices" => state.turn_indices = Some(parse_int_map(tokenizer)?),
                "turnIdx" => state.turn_index = Some(read_i32(tokenizer, &key)?),
                "storySeed" => state.story_seed = Some(read_i32(tokenizer, &key)?),
                "previousRandom" => state.previous_random = Some(read_i32(tokenizer, &key)?),
                "callstackThreads" => state.legacy_threads = Some(parse_threads(tokenizer, root)?),
                "callstackThreadCounter" => {
                    state.legacy_thread_counter = Some(read_usize(tokenizer, &key)?)
                }
                "outputStream" => state.legacy_output = Some(read_runtime_object_list(tokenizer)?),
                "currentChoices" => state.legacy_choices = Some(parse_choices(tokenizer)?),
                "choiceThreads" => {
                    state.legacy_choice_threads = parse_choice_threads(tokenizer, root)?
                }
                _ => tokenizer.skip_value()?,
            }
            if !next_property(tokenizer)? {
                break;
            }
        }
    }
    tokenizer.expect('}')?;
    tokenizer.expect_eof()?;
    Ok(state)
}

fn commit(state: &mut StoryState, mut parsed: ParsedState) -> Result<(), StoryError> {
    let version = parsed
        .version
        .ok_or_else(|| bad("ink save format incorrect, can't load"))?;
    if version < MIN_COMPATIBLE_LOAD_VERSION as i32 {
        return Err(bad(format!(
            "Ink save format isn't compatible (saw {version}, minimum is {MIN_COMPATIBLE_LOAD_VERSION})"
        )));
    }
    let (current_flow, named_flows) = if let Some(mut flows) = parsed.flows.take() {
        let requested = parsed
            .current_flow_name
            .take()
            .or_else(|| {
                if flows.len() == 1 {
                    flows.keys().next().cloned()
                } else {
                    None
                }
            })
            .ok_or_else(|| bad("currentFlowName is missing"))?;
        let current = flows
            .remove(&requested)
            .ok_or_else(|| bad(format!("current flow '{requested}' does not exist")))?;
        (current, if flows.is_empty() { None } else { Some(flows) })
    } else {
        let mut callstack = CallStack::new(state.main_content_container.clone());
        callstack.load_stream_parts(
            &state.main_content_container,
            parsed
                .legacy_threads
                .take()
                .ok_or_else(|| bad("legacy save has no callstackThreads"))?,
            parsed.legacy_thread_counter.unwrap_or_default(),
        );
        (
            Flow::from_stream_parts(
                DEFAULT_FLOW_NAME.to_owned(),
                callstack,
                parsed.legacy_output.take().unwrap_or_default(),
                parsed.legacy_choices.take().unwrap_or_default(),
                parsed.legacy_choice_threads,
            )?,
            None,
        )
    };

    let diverted_pointer = if let Some(path) = parsed.diverted_path {
        Story::pointer_at_path(
            &state.main_content_container,
            &Path::new_with_components_string(Some(&path)),
        )?
    } else {
        pointer::NULL.clone()
    };

    state.current_flow = current_flow;
    state.named_flows = named_flows;
    state
        .variables_state
        .load_stream_values(parsed.variables.take().unwrap_or_default());
    state
        .variables_state
        .set_callstack(state.current_flow.callstack.clone());
    state.evaluation_stack = parsed.evaluation_stack.take().unwrap_or_default();
    state.diverted_pointer = diverted_pointer;
    state.visit_counts = parsed.visit_counts.take().unwrap_or_default();
    state.turn_indices = parsed.turn_indices.take().unwrap_or_default();
    state.current_turn_index = parsed.turn_index.unwrap_or_default();
    state.story_seed = parsed.story_seed.unwrap_or_default();
    state.previous_random = parsed.previous_random.unwrap_or_default();
    state.output_stream_dirty();
    state.alive_flow_names_dirty = true;
    Ok(())
}

pub(crate) fn load_state<R: Read>(state: &mut StoryState, reader: R) -> Result<(), StoryError> {
    let mut tokenizer = JsonTokenizer::new(reader);
    let parsed = parse_document(&mut tokenizer, &state.main_content_container)?;
    commit(state, parsed)
}
