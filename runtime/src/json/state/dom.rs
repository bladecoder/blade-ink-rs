//! Serde-based Ink save-state codec for the runtime.

#[allow(unused_imports)]
use crate::prelude::*;

use crate::runtime::{Runtime, RuntimeChoice, RuntimeFlow};
use crate::{
    callstack::{CallStack, Thread},
    compat::{collections::HashMap, rc::Rc},
    control_command::{CommandType, ControlCommand},
    json::object::{read_dom, write_dom},
    json::state::format::{INK_SAVE_STATE_VERSION, MIN_COMPATIBLE_LOAD_VERSION},
    object::RTObject,
    output_text::clean_output_whitespace,
    path::Path,
    story::error::StoryError,
    story_content::{ContentPointer, StaticStoryView},
    tag::Tag,
    value::Value,
    value_type::StringValue,
};
use serde_json::{Map, json};

impl Runtime {
    fn write_flow_json(
        &self,
        stack: &CallStack,
        output: &str,
        tags: &[String],
        choices: &[RuntimeChoice],
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
            write_dom::write_runtime_object_list(&output_stream)?,
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
            write_dom::write_runtime_object_list(&self.evaluation_stack)?,
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

    fn load_flow_json(&self, encoded: &serde_json::Value) -> Result<RuntimeFlow, StoryError> {
        let object = encoded
            .as_object()
            .ok_or_else(|| StoryError::BadJson("flow must be an object".to_owned()))?;
        let root = self.data.container_id(self.data.root()).unwrap();
        let mut flow = RuntimeFlow::new(root);
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
        let output = read_dom::read_runtime_object_list(output, false)?;
        let mut tag = None::<String>;
        for item in output {
            if let Some(command) = item.as_any().downcast_ref::<ControlCommand>() {
                match command.command_type {
                    CommandType::BeginTag => tag = Some(String::new()),
                    CommandType::EndTag => {
                        if let Some(value) = tag.take() {
                            flow.current_tags.push(clean_output_whitespace(&value));
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
                    Thread::from_json(&self.data, encoded)?
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
            flow.choices.push(RuntimeChoice {
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
        if version < MIN_COMPATIBLE_LOAD_VERSION as u64 {
            return Err(StoryError::BadJson(format!(
                "Ink save format isn't compatible with the current version (saw '{version}', but minimum is {}), so can't load.",
                MIN_COMPATIBLE_LOAD_VERSION
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
        loaded.variables.set_callstack(loaded.callstack.clone());
        loaded.variables.load_json(
            object
                .get("variablesState")
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| StoryError::BadJson("variablesState not found".to_owned()))?,
        )?;
        loaded.evaluation_stack = read_dom::read_runtime_object_list(
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
            .unwrap_or(ContentPointer::NULL);
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
