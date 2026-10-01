//! Streaming Ink save-state codec for JSON and binary-image configurations.

#[allow(unused_imports)]
use crate::prelude::*;

use crate::{
    callstack::{CallStack, CallStackElement, Thread},
    compat::{
        cell::RefCell,
        collections::HashMap,
        io::{Read, Write},
        rc::Rc,
    },
    control_command::{CommandType, ControlCommand},
    json::{
        json_tokenizer::JsonTokenizer,
        json_writer::JsonWriter,
        state_read_stream::{read_runtime_object, read_runtime_object_list},
        state_write_stream,
    },
    object::RTObject,
    output_text::clean_output_whitespace,
    path::Path,
    push_pop::PushPopType,
    save_format::{INK_SAVE_STATE_VERSION, MIN_COMPATIBLE_LOAD_VERSION},
    story_content::{ContentPointer, StaticStoryView, StoryContent},
    story_error::StoryError,
    tag::Tag,
    value::Value,
    value_type::StringValue,
};

use super::{Runtime, RuntimeChoice, RuntimeFlow};

fn write_thread<W: Write>(
    writer: &mut JsonWriter<W>,
    runtime: &Runtime,
    thread: &Thread,
) -> Result<(), StoryError> {
    writer.raw("{\"callstack\":[")?;
    let mut first = true;
    for element in &thread.callstack {
        writer.separator(&mut first)?;
        writer.raw("{")?;
        let mut first_property = true;
        if let Some(container) = element.current_pointer.container {
            writer.key(&mut first_property, "cPath")?;
            writer.string(
                &runtime
                    .data
                    .canonical_path_text(container.node())
                    .ok_or_else(|| {
                        StoryError::InvalidStoryState(
                            "callstack pointer has invalid container".to_owned(),
                        )
                    })?,
            )?;
            writer.key(&mut first_property, "idx")?;
            writer.integer(element.current_pointer.index)?;
        }
        writer.key(&mut first_property, "exp")?;
        writer.boolean(element.in_expression_evaluation)?;
        writer.key(&mut first_property, "type")?;
        writer.integer(element.push_pop_type as u32)?;
        if !element.temporary_variables.is_empty() {
            writer.key(&mut first_property, "temp")?;
            state_write_stream::write_dictionary_values(writer, &element.temporary_variables)?;
        }
        writer.raw("}")?;
    }
    writer.raw("],\"threadIndex\":")?;
    writer.integer(thread.thread_index)?;
    if !thread.previous_pointer.is_null() {
        writer.raw(",\"previousContentObject\":")?;
        writer.string(
            &thread
                .previous_pointer
                .path(&runtime.data)
                .ok_or_else(|| {
                    StoryError::InvalidStoryState("previous pointer does not resolve".to_owned())
                })?
                .to_string(),
        )?;
    }
    writer.raw("}")?;
    Ok(())
}

fn write_stack<W: Write>(
    writer: &mut JsonWriter<W>,
    runtime: &Runtime,
    stack: &CallStack,
) -> Result<(), StoryError> {
    writer.raw("{\"threads\":[")?;
    let mut first = true;
    for thread in stack.threads() {
        writer.separator(&mut first)?;
        write_thread(writer, runtime, thread)?;
    }
    writer.raw("],\"threadCounter\":")?;
    writer.integer(stack.thread_counter())?;
    writer.raw("}")?;
    Ok(())
}

fn write_output<W: Write>(
    writer: &mut JsonWriter<W>,
    output: &str,
    tags: &[String],
) -> Result<(), StoryError> {
    let mut stream: Vec<Rc<dyn RTObject>> = Vec::new();
    if !output.is_empty() {
        for segment in output.split_inclusive('\n') {
            let text = segment.strip_suffix('\n').unwrap_or(segment);
            if !text.is_empty() {
                stream.push(Rc::new(Value::new(text)));
            }
            if segment.ends_with('\n') {
                stream.push(Rc::new(Value::new("\n")));
            }
        }
    }
    for tag in tags {
        stream.push(Rc::new(ControlCommand::new(CommandType::BeginTag)));
        stream.push(Rc::new(Value::new(tag.as_str())));
        stream.push(Rc::new(ControlCommand::new(CommandType::EndTag)));
    }
    state_write_stream::write_list_rt_objs(writer, &stream)
}

fn write_choices<W: Write>(
    writer: &mut JsonWriter<W>,
    runtime: &Runtime,
    choices: &[RuntimeChoice],
) -> Result<(), StoryError> {
    writer.raw("[")?;
    let mut first = true;
    for (index, choice) in choices.iter().enumerate() {
        writer.separator(&mut first)?;
        writer.raw("{\"text\":")?;
        writer.string(&choice.text)?;
        writer.raw(",\"index\":")?;
        writer.integer(index)?;
        writer.raw(",\"originalChoicePath\":")?;
        writer.string(
            &runtime
                .data
                .canonical_path_text(choice.source)
                .ok_or_else(|| {
                    StoryError::InvalidStoryState("choice has invalid source".to_owned())
                })?,
        )?;
        writer.raw(",\"originalThreadIndex\":")?;
        writer.integer(choice.thread.thread_index)?;
        writer.raw(",\"targetPath\":")?;
        writer.string(
            &runtime
                .data
                .canonical_path_text(choice.target.node())
                .ok_or_else(|| {
                    StoryError::InvalidStoryState("choice has invalid target".to_owned())
                })?,
        )?;
        writer.raw(",\"tags\":[")?;
        let mut first_tag = true;
        for tag in &choice.tags {
            writer.separator(&mut first_tag)?;
            writer.string(tag)?;
        }
        writer.raw("]}")?;
    }
    writer.raw("]")?;
    Ok(())
}

fn write_flow<W: Write>(
    writer: &mut JsonWriter<W>,
    runtime: &Runtime,
    stack: &CallStack,
    output: &str,
    tags: &[String],
    choices: &[RuntimeChoice],
) -> Result<(), StoryError> {
    writer.raw("{\"callstack\":")?;
    write_stack(writer, runtime, stack)?;
    writer.raw(",\"outputStream\":")?;
    write_output(writer, output, tags)?;
    if !choices.is_empty() {
        writer.raw(",\"choiceThreads\":{")?;
        let mut first = true;
        for choice in choices {
            if stack
                .get_thread_with_index(choice.thread.thread_index)
                .is_none()
            {
                writer.key(&mut first, &choice.thread.thread_index.to_string())?;
                write_thread(writer, runtime, &choice.thread)?;
            }
        }
        writer.raw("}")?;
    }
    writer.raw(",\"currentChoices\":")?;
    write_choices(writer, runtime, choices)?;
    writer.raw("}")?;
    Ok(())
}

impl Runtime {
    pub(crate) fn save_state_to_writer(&self, output: impl Write) -> Result<(), StoryError> {
        let mut writer = JsonWriter::new(output);
        writer.raw("{\"flows\":{")?;
        let mut names: Vec<_> = self.named_flows.keys().collect();
        names.push(&self.current_flow_name);
        names.sort_unstable();
        let mut first = true;
        for name in names {
            writer.key(&mut first, name)?;
            if name == &self.current_flow_name {
                write_flow(
                    &mut writer,
                    self,
                    &self.callstack.borrow(),
                    &self.output,
                    &self.current_tags,
                    &self.choices,
                )?;
            } else {
                let flow = &self.named_flows[name];
                write_flow(
                    &mut writer,
                    self,
                    &flow.callstack.borrow(),
                    &flow.output,
                    &flow.current_tags,
                    &flow.choices,
                )?;
            }
        }
        writer.raw("},\"currentFlowName\":")?;
        writer.string(&self.current_flow_name)?;
        writer.raw(",\"variablesState\":")?;
        self.variables.write_json_stream(&mut writer)?;
        writer.raw(",\"evalStack\":")?;
        state_write_stream::write_list_rt_objs(&mut writer, &self.evaluation_stack)?;
        if !self.diverted.is_null() {
            writer.raw(",\"currentDivertTarget\":")?;
            writer.string(
                &self
                    .diverted
                    .path(&self.data)
                    .ok_or_else(|| {
                        StoryError::InvalidStoryState("divert has invalid path".to_owned())
                    })?
                    .to_string(),
            )?;
        }
        writer.raw(",\"visitCounts\":")?;
        state_write_stream::write_int_dictionary(
            &mut writer,
            &self.counters.visit_paths_for_save(&self.data)?,
        )?;
        writer.raw(",\"turnIndices\":")?;
        state_write_stream::write_int_dictionary(
            &mut writer,
            &self.counters.turn_paths_for_save(&self.data)?,
        )?;
        writer.raw(",\"turnIdx\":")?;
        writer.integer(self.current_turn_index)?;
        writer.raw(",\"storySeed\":")?;
        writer.integer(self.story_seed)?;
        writer.raw(",\"previousRandom\":")?;
        writer.integer(self.previous_random)?;
        writer.raw(",\"inkSaveVersion\":")?;
        writer.integer(INK_SAVE_STATE_VERSION)?;
        writer.raw(",\"inkFormatVersion\":")?;
        writer.integer(crate::story::INK_VERSION_CURRENT)?;
        writer.raw("}")?;
        Ok(())
    }

    pub(crate) fn save_state_json(&self) -> Result<String, StoryError> {
        let mut bytes = Vec::new();
        self.save_state_to_writer(&mut bytes)?;
        String::from_utf8(bytes).map_err(|error| StoryError::BadJson(error.to_string()))
    }
}

fn bad(message: impl Into<String>) -> StoryError {
    StoryError::BadJson(message.into())
}

fn read_i32<R: Read>(reader: &mut JsonTokenizer<R>, field: &str) -> Result<i32, StoryError> {
    reader
        .read_number()?
        .as_integer()
        .ok_or_else(|| bad(format!("{field} must be an integer")))
}

fn read_usize<R: Read>(reader: &mut JsonTokenizer<R>, field: &str) -> Result<usize, StoryError> {
    usize::try_from(read_i32(reader, field)?)
        .map_err(|_| bad(format!("{field} must be non-negative")))
}

fn next_property<R: Read>(reader: &mut JsonTokenizer<R>) -> Result<bool, StoryError> {
    match reader.peek()? {
        '}' => Ok(false),
        ',' => {
            reader.expect(',')?;
            Ok(true)
        }
        found => Err(bad(format!("expected ',' or '}}', found '{found}'"))),
    }
}

fn parse_value_map<R: Read>(
    reader: &mut JsonTokenizer<R>,
) -> Result<HashMap<String, Rc<Value>>, StoryError> {
    reader.expect('{')?;
    let mut values = HashMap::new();
    if reader.peek()? != '}' {
        loop {
            let key = reader.read_obj_key()?;
            let value = read_runtime_object(reader)?
                .into_any()
                .downcast::<Value>()
                .map_err(|_| bad(format!("variable '{key}' is not a value")))?;
            if values.insert(key.clone(), value).is_some() {
                return Err(bad(format!("duplicate variable '{key}'")));
            }
            if !next_property(reader)? {
                break;
            }
        }
    }
    reader.expect('}')?;
    Ok(values)
}

fn parse_int_map<R: Read>(
    reader: &mut JsonTokenizer<R>,
) -> Result<HashMap<String, i32>, StoryError> {
    reader.expect('{')?;
    let mut values = HashMap::new();
    if reader.peek()? != '}' {
        loop {
            let key = reader.read_obj_key()?;
            let value = read_i32(reader, &key)?;
            if values.insert(key.clone(), value).is_some() {
                return Err(bad(format!("duplicate key '{key}'")));
            }
            if !next_property(reader)? {
                break;
            }
        }
    }
    reader.expect('}')?;
    Ok(values)
}

fn pointer_at_text(data: &StoryContent, text: &str) -> Result<ContentPointer, StoryError> {
    data.pointer_at_path(&Path::new_with_components_string(Some(text)))
        .ok_or_else(|| bad(format!("pointer path '{text}' does not resolve")))
}

fn parse_element<R: Read>(
    reader: &mut JsonTokenizer<R>,
    data: &StoryContent,
) -> Result<CallStackElement, StoryError> {
    reader.expect('{')?;
    let mut path = None;
    let mut index = -1;
    let mut expression = false;
    let mut kind = None;
    let mut temporary = HashMap::new();
    if reader.peek()? != '}' {
        loop {
            let key = reader.read_obj_key()?;
            match key.as_str() {
                "cPath" => path = Some(reader.read_string()?),
                "idx" => index = read_i32(reader, "idx")?,
                "exp" => expression = reader.read_boolean()?,
                "type" => kind = Some(PushPopType::from_value(read_usize(reader, "type")?)?),
                "temp" => temporary = parse_value_map(reader)?,
                _ => reader.skip_value()?,
            }
            if !next_property(reader)? {
                break;
            }
        }
    }
    reader.expect('}')?;
    let pointer = if let Some(path) = path {
        let container = data
            .resolve_path(data.root(), &Path::new_with_components_string(Some(&path)))
            .and_then(|id| data.container_id(id))
            .ok_or_else(|| bad(format!("callstack path '{path}' does not resolve")))?;
        ContentPointer {
            container: Some(container),
            index,
        }
    } else {
        ContentPointer::NULL
    };
    let mut element = CallStackElement::new(
        kind.ok_or_else(|| bad("callstack element has no type"))?,
        pointer,
    );
    element.in_expression_evaluation = expression;
    element.temporary_variables = temporary;
    Ok(element)
}

fn parse_thread<R: Read>(
    reader: &mut JsonTokenizer<R>,
    data: &StoryContent,
) -> Result<Thread, StoryError> {
    reader.expect('{')?;
    let mut frames = Vec::new();
    let mut index = None;
    let mut previous = ContentPointer::NULL;
    if reader.peek()? != '}' {
        loop {
            let key = reader.read_obj_key()?;
            match key.as_str() {
                "callstack" => {
                    reader.expect('[')?;
                    while reader.peek()? != ']' {
                        frames.push(parse_element(reader, data)?);
                        if reader.peek()? != ']' {
                            reader.expect(',')?;
                        }
                    }
                    reader.expect(']')?;
                }
                "threadIndex" => index = Some(read_usize(reader, "threadIndex")?),
                "previousContentObject" => {
                    previous = pointer_at_text(data, &reader.read_string()?)?
                }
                _ => reader.skip_value()?,
            }
            if !next_property(reader)? {
                break;
            }
        }
    }
    reader.expect('}')?;
    Ok(Thread {
        callstack: frames,
        previous_pointer: previous,
        thread_index: index.ok_or_else(|| bad("thread has no threadIndex"))?,
    })
}

fn parse_stack<R: Read>(
    reader: &mut JsonTokenizer<R>,
    data: &StoryContent,
) -> Result<CallStack, StoryError> {
    reader.expect('{')?;
    let mut threads = None;
    let mut counter = None;
    if reader.peek()? != '}' {
        loop {
            let key = reader.read_obj_key()?;
            match key.as_str() {
                "threads" => {
                    reader.expect('[')?;
                    let mut parsed = Vec::new();
                    while reader.peek()? != ']' {
                        parsed.push(parse_thread(reader, data)?);
                        if reader.peek()? != ']' {
                            reader.expect(',')?;
                        }
                    }
                    reader.expect(']')?;
                    threads = Some(parsed);
                }
                "threadCounter" => counter = Some(read_usize(reader, "threadCounter")?),
                _ => reader.skip_value()?,
            }
            if !next_property(reader)? {
                break;
            }
        }
    }
    reader.expect('}')?;
    let root = data
        .container_id(data.root())
        .ok_or_else(|| bad("root is not a container"))?;
    let mut stack = CallStack::new(root);
    stack.replace_threads(
        threads.ok_or_else(|| bad("callstack has no threads"))?,
        counter.ok_or_else(|| bad("callstack has no threadCounter"))?,
    )?;
    Ok(stack)
}

struct SavedChoice {
    text: String,
    source: String,
    target: String,
    thread_index: usize,
    tags: Vec<String>,
}

fn parse_choice<R: Read>(reader: &mut JsonTokenizer<R>) -> Result<SavedChoice, StoryError> {
    reader.expect('{')?;
    let mut text = None;
    let mut source = None;
    let mut target = None;
    let mut thread_index = None;
    let mut tags = Vec::new();
    if reader.peek()? != '}' {
        loop {
            let key = reader.read_obj_key()?;
            match key.as_str() {
                "text" => text = Some(reader.read_string()?),
                "originalChoicePath" => source = Some(reader.read_string()?),
                "targetPath" => target = Some(reader.read_string()?),
                "originalThreadIndex" => {
                    thread_index = Some(read_usize(reader, "originalThreadIndex")?)
                }
                "index" => {
                    read_usize(reader, "index")?;
                }
                "tags" => {
                    reader.expect('[')?;
                    while reader.peek()? != ']' {
                        tags.push(reader.read_string()?);
                        if reader.peek()? != ']' {
                            reader.expect(',')?;
                        }
                    }
                    reader.expect(']')?;
                }
                _ => reader.skip_value()?,
            }
            if !next_property(reader)? {
                break;
            }
        }
    }
    reader.expect('}')?;
    Ok(SavedChoice {
        text: text.ok_or_else(|| bad("choice has no text"))?,
        source: source.ok_or_else(|| bad("choice has no originalChoicePath"))?,
        target: target.ok_or_else(|| bad("choice has no targetPath"))?,
        thread_index: thread_index.ok_or_else(|| bad("choice has no originalThreadIndex"))?,
        tags,
    })
}

fn parse_output(output: Vec<Rc<dyn RTObject>>) -> (String, Vec<String>) {
    let mut text = String::new();
    let mut tags = Vec::new();
    let mut pending = None::<String>;
    for item in output {
        if let Some(command) = item.as_any().downcast_ref::<ControlCommand>() {
            match command.command_type {
                CommandType::BeginTag => pending = Some(String::new()),
                CommandType::EndTag => {
                    if let Some(value) = pending.take() {
                        tags.push(clean_output_whitespace(&value));
                    }
                }
                _ => {}
            }
        } else if let Some(value) = Value::get_value::<&StringValue>(item.as_ref()) {
            if let Some(tag) = pending.as_mut() {
                tag.push_str(&value.string);
            } else {
                text.push_str(&value.string);
            }
        } else if let Some(tag) = item.as_any().downcast_ref::<Tag>() {
            tags.push(tag.get_text().clone());
        }
    }
    (text, tags)
}

fn parse_flow<R: Read>(
    reader: &mut JsonTokenizer<R>,
    data: &StoryContent,
) -> Result<RuntimeFlow, StoryError> {
    reader.expect('{')?;
    let mut stack = None;
    let mut output = None;
    let mut saved_choices = None;
    let mut choice_threads = HashMap::new();
    if reader.peek()? != '}' {
        loop {
            let key = reader.read_obj_key()?;
            match key.as_str() {
                "callstack" => stack = Some(parse_stack(reader, data)?),
                "outputStream" => output = Some(read_runtime_object_list(reader)?),
                "currentChoices" => {
                    reader.expect('[')?;
                    let mut choices = Vec::new();
                    while reader.peek()? != ']' {
                        choices.push(parse_choice(reader)?);
                        if reader.peek()? != ']' {
                            reader.expect(',')?;
                        }
                    }
                    reader.expect(']')?;
                    saved_choices = Some(choices);
                }
                "choiceThreads" => {
                    reader.expect('{')?;
                    if reader.peek()? != '}' {
                        loop {
                            let key = reader.read_obj_key()?;
                            let index = key
                                .parse::<usize>()
                                .map_err(|_| bad(format!("invalid choice thread index '{key}'")))?;
                            let thread = parse_thread(reader, data)?;
                            if choice_threads.insert(index, thread).is_some() {
                                return Err(bad(format!("duplicate choice thread '{key}'")));
                            }
                            if !next_property(reader)? {
                                break;
                            }
                        }
                    }
                    reader.expect('}')?;
                }
                _ => reader.skip_value()?,
            }
            if !next_property(reader)? {
                break;
            }
        }
    }
    reader.expect('}')?;
    let stack = stack.ok_or_else(|| bad("flow has no callstack"))?;
    let output = output.ok_or_else(|| bad("flow has no outputStream"))?;
    let (text, tags) = parse_output(output);
    let saved_choices = saved_choices.ok_or_else(|| bad("flow has no currentChoices"))?;
    let mut choices = Vec::with_capacity(saved_choices.len());
    for saved in saved_choices {
        let target = data
            .resolve_path(
                data.root(),
                &Path::new_with_components_string(Some(&saved.target)),
            )
            .and_then(|id| data.container_id(id))
            .ok_or_else(|| bad("choice target does not resolve"))?;
        let source = data
            .resolve_path(
                data.root(),
                &Path::new_with_components_string(Some(&saved.source)),
            )
            .ok_or_else(|| bad("choice source does not resolve"))?;
        let thread = stack
            .get_thread_with_index(saved.thread_index)
            .cloned()
            .or_else(|| choice_threads.get(&saved.thread_index).cloned())
            .ok_or_else(|| {
                bad(format!(
                    "choice references missing thread {}",
                    saved.thread_index
                ))
            })?;
        choices.push(RuntimeChoice {
            target,
            source,
            is_invisible_default: false,
            text: saved.text,
            tags: saved.tags,
            thread,
        });
    }
    Ok(RuntimeFlow {
        callstack: Rc::new(RefCell::new(stack)),
        output: text,
        choices,
        current_tags: tags,
    })
}

#[derive(Default)]
struct ParsedState {
    version: Option<i32>,
    flows: Option<HashMap<String, RuntimeFlow>>,
    current_flow_name: Option<String>,
    variables: Option<HashMap<String, Rc<Value>>>,
    evaluation_stack: Option<Vec<Rc<dyn RTObject>>>,
    diverted_path: Option<String>,
    visit_counts: Option<HashMap<String, i32>>,
    turn_indices: Option<HashMap<String, i32>>,
    turn_index: Option<i32>,
    story_seed: Option<i32>,
    previous_random: Option<i32>,
}

fn parse_document<R: Read>(
    reader: &mut JsonTokenizer<R>,
    data: &StoryContent,
) -> Result<ParsedState, StoryError> {
    reader.expect('{')?;
    let mut state = ParsedState::default();
    if reader.peek()? != '}' {
        loop {
            let key = reader.read_obj_key()?;
            match key.as_str() {
                "inkSaveVersion" => state.version = Some(read_i32(reader, &key)?),
                "flows" => {
                    reader.expect('{')?;
                    let mut flows = HashMap::new();
                    if reader.peek()? != '}' {
                        loop {
                            let name = reader.read_obj_key()?;
                            let flow = parse_flow(reader, data)?;
                            if flows.insert(name.clone(), flow).is_some() {
                                return Err(bad(format!("duplicate flow '{name}'")));
                            }
                            if !next_property(reader)? {
                                break;
                            }
                        }
                    }
                    reader.expect('}')?;
                    state.flows = Some(flows);
                }
                "currentFlowName" => state.current_flow_name = Some(reader.read_string()?),
                "variablesState" => state.variables = Some(parse_value_map(reader)?),
                "evalStack" => state.evaluation_stack = Some(read_runtime_object_list(reader)?),
                "currentDivertTarget" => state.diverted_path = Some(reader.read_string()?),
                "visitCounts" => state.visit_counts = Some(parse_int_map(reader)?),
                "turnIndices" => state.turn_indices = Some(parse_int_map(reader)?),
                "turnIdx" => state.turn_index = Some(read_i32(reader, &key)?),
                "storySeed" => state.story_seed = Some(read_i32(reader, &key)?),
                "previousRandom" => state.previous_random = Some(read_i32(reader, &key)?),
                _ => reader.skip_value()?,
            }
            if !next_property(reader)? {
                break;
            }
        }
    }
    reader.expect('}')?;
    reader.expect_eof()?;
    Ok(state)
}

impl Runtime {
    pub(crate) fn load_state_from_reader(&mut self, input: impl Read) -> Result<(), StoryError> {
        let mut reader = JsonTokenizer::new(input);
        let mut parsed = parse_document(&mut reader, &self.data)?;
        let version = parsed
            .version
            .ok_or_else(|| bad("ink save format incorrect, can't load"))?;
        if version < MIN_COMPATIBLE_LOAD_VERSION as i32 {
            return Err(bad(format!(
                "Ink save format isn't compatible (saw {version}, minimum is {MIN_COMPATIBLE_LOAD_VERSION})"
            )));
        }
        let mut flows = parsed
            .flows
            .take()
            .ok_or_else(|| bad("save has no flows"))?;
        let current_name = parsed
            .current_flow_name
            .take()
            .or_else(|| {
                if flows.len() == 1 {
                    flows.keys().next().cloned()
                } else {
                    None
                }
            })
            .ok_or_else(|| bad("save has no currentFlowName"))?;
        let current = flows
            .remove(&current_name)
            .ok_or_else(|| bad("current flow not found"))?;
        let mut loaded = self.snapshot();
        loaded.current_flow_name = current_name;
        loaded.named_flows = flows;
        loaded.callstack = current.callstack;
        loaded.output = current.output;
        loaded.current_tags = current.current_tags;
        loaded.choices = current.choices;
        loaded.variables.set_callstack(loaded.callstack.clone());
        loaded.variables.load_stream_values(
            parsed
                .variables
                .ok_or_else(|| bad("save has no variablesState"))?,
        );
        loaded.evaluation_stack = parsed
            .evaluation_stack
            .ok_or_else(|| bad("save has no evalStack"))?;
        loaded.diverted = parsed
            .diverted_path
            .as_deref()
            .map(|path| pointer_at_text(&loaded.data, path))
            .transpose()?
            .unwrap_or(ContentPointer::NULL);
        loaded
            .counters
            .restore_visit_paths(&loaded.data, &parsed.visit_counts.unwrap_or_default())?;
        loaded
            .counters
            .restore_turn_paths(&loaded.data, &parsed.turn_indices.unwrap_or_default())?;
        loaded.current_turn_index = parsed.turn_index.unwrap_or(-1);
        loaded.story_seed = parsed.story_seed.unwrap_or(0);
        loaded.previous_random = parsed.previous_random.unwrap_or(0);
        *self = loaded;
        Ok(())
    }

    pub(crate) fn load_state_json(&mut self, saved: &str) -> Result<(), StoryError> {
        self.load_state_from_reader(saved.as_bytes())
    }
}
