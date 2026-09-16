use std::{cell::RefCell, rc::Rc};

#[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
use serde_json::Map;

use crate::{
    callstack::{CallStack, Thread},
    choice::Choice,
    container::Container,
    object::RTObject,
    story_error::StoryError,
};

#[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
use crate::json::{json_read, json_write};
#[cfg(feature = "stream-json-parser")]
use crate::json::{json_write_stream, json_writer::JsonWriter};
#[cfg(feature = "stream-json-parser")]
use std::{collections::BTreeMap, collections::HashMap, io::Write};

#[derive(Clone)]
pub(crate) struct Flow {
    pub name: String,
    pub callstack: Rc<RefCell<CallStack>>,
    pub output_stream: Vec<Rc<dyn RTObject>>,
    pub current_choices: Vec<Rc<Choice>>,
}

impl Flow {
    pub fn new(name: &str, main_content_container: Rc<Container>) -> Flow {
        Flow {
            name: name.to_string(),
            callstack: Rc::new(RefCell::new(CallStack::new(main_content_container))),
            output_stream: Vec::new(),
            current_choices: Vec::new(),
        }
    }

    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    pub fn from_json(
        name: &str,
        main_content_container: Rc<Container>,
        j_obj: &Map<String, serde_json::Value>,
    ) -> Result<Flow, StoryError> {
        let mut flow = Self {
            name: name.to_string(),
            callstack: Rc::new(RefCell::new(CallStack::new(main_content_container.clone()))),
            output_stream: json_read::jarray_to_runtime_obj_list(
                j_obj
                    .get("outputStream")
                    .ok_or(StoryError::BadJson("outputStream not found.".to_owned()))?
                    .as_array()
                    .unwrap(),
                false,
            )?,
            current_choices: json_read::jarray_to_runtime_obj_list(
                j_obj
                    .get("currentChoices")
                    .ok_or(StoryError::BadJson("currentChoices not found.".to_owned()))?
                    .as_array()
                    .unwrap(),
                false,
            )?
            .iter()
            .map(|o| o.clone().into_any().downcast::<Choice>().unwrap())
            .collect::<Vec<Rc<Choice>>>(),
        };

        flow.callstack.borrow_mut().load_json(
            &main_content_container,
            j_obj
                .get("callstack")
                .ok_or(StoryError::BadJson("loading callstack".to_owned()))?
                .as_object()
                .unwrap(),
        )?;
        let j_choice_threads = j_obj.get("choiceThreads");

        flow.load_flow_choice_threads(j_choice_threads, main_content_container)?;

        Ok(flow)
    }

    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    pub(crate) fn write_json(&self) -> Result<serde_json::Value, StoryError> {
        let mut flow: Map<String, serde_json::Value> = Map::new();

        flow.insert(
            "callstack".to_owned(),
            self.callstack.borrow().write_json()?,
        );
        flow.insert(
            "outputStream".to_owned(),
            json_write::write_list_rt_objs(&self.output_stream)?,
        );

        // choiceThreads: optional
        // Has to come BEFORE the choices themselves are written out
        // since the originalThreadIndex of each choice needs to be set
        let mut has_choice_threads = false;
        let mut jct: Map<String, serde_json::Value> = Map::new();
        for c in self.current_choices.iter() {
            c.original_thread_index
                .replace(c.get_thread_at_generation().unwrap().thread_index);

            if self
                .callstack
                .borrow()
                .get_thread_with_index(*c.original_thread_index.borrow())
                .is_none()
            {
                if !has_choice_threads {
                    has_choice_threads = true;
                }

                jct.insert(
                    c.original_thread_index.borrow().to_string(),
                    c.get_thread_at_generation().unwrap().write_json()?,
                );
            }
        }

        if has_choice_threads {
            flow.insert("choiceThreads".to_owned(), serde_json::Value::Object(jct));
        }

        let mut c_array: Vec<serde_json::Value> = Vec::new();
        for c in self.current_choices.iter() {
            c_array.push(json_write::write_choice(c));
        }

        flow.insert(
            "currentChoices".to_owned(),
            serde_json::Value::Array(c_array),
        );

        Ok(serde_json::Value::Object(flow))
    }

    #[cfg(feature = "stream-json-parser")]
    pub(crate) fn from_stream_parts(
        name: String,
        callstack: CallStack,
        output_stream: Vec<Rc<dyn RTObject>>,
        current_choices: Vec<Rc<Choice>>,
        choice_threads: HashMap<usize, Thread>,
    ) -> Result<Self, StoryError> {
        let flow = Self {
            name,
            callstack: Rc::new(RefCell::new(callstack)),
            output_stream,
            current_choices,
        };
        for choice in &flow.current_choices {
            let index = *choice.original_thread_index.borrow();
            if let Some(thread) = flow.callstack.borrow().get_thread_with_index(index) {
                choice.set_thread_at_generation(thread.clone());
            } else if let Some(thread) = choice_threads.get(&index) {
                choice.set_thread_at_generation(thread.clone());
            } else {
                return Err(StoryError::BadJson(format!(
                    "choice references missing thread {index}"
                )));
            }
        }
        Ok(flow)
    }

    #[cfg(feature = "stream-json-parser")]
    pub(crate) fn write_json_stream<W: Write>(
        &self,
        writer: &mut JsonWriter<W>,
    ) -> Result<(), StoryError> {
        writer.raw("{\"callstack\":")?;
        self.callstack.borrow().write_json_stream(writer)?;
        writer.raw(",\"outputStream\":")?;
        json_write_stream::write_list_rt_objs(writer, &self.output_stream)?;

        let mut choice_threads = BTreeMap::new();
        for choice in &self.current_choices {
            let thread = choice.get_thread_at_generation().ok_or_else(|| {
                StoryError::InvalidStoryState("choice has no generation thread".to_owned())
            })?;
            choice.original_thread_index.replace(thread.thread_index);
            if self
                .callstack
                .borrow()
                .get_thread_with_index(thread.thread_index)
                .is_none()
            {
                choice_threads.insert(thread.thread_index, thread);
            }
        }
        if !choice_threads.is_empty() {
            writer.raw(",\"choiceThreads\":{")?;
            let mut first = true;
            for (index, thread) in choice_threads {
                writer.separator(&mut first)?;
                writer.string(&index.to_string())?;
                writer.raw(":")?;
                thread.write_json_stream(writer)?;
            }
            writer.raw("}")?;
        }
        writer.raw(",\"currentChoices\":[")?;
        let mut first = true;
        for choice in &self.current_choices {
            writer.separator(&mut first)?;
            json_write_stream::write_choice(writer, choice)?;
        }
        writer.raw("]}")?;
        Ok(())
    }

    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    pub fn load_flow_choice_threads(
        &mut self,
        j_choice_threads: Option<&serde_json::Value>,
        main_content_container: Rc<Container>,
    ) -> Result<(), StoryError> {
        for choice in self.current_choices.iter_mut() {
            self.callstack
                .borrow()
                .get_thread_with_index(*choice.original_thread_index.borrow())
                .map(|o| choice.set_thread_at_generation(o.clone()))
                .or_else(|| {
                    let j_saved_choice_thread = j_choice_threads
                        .and_then(|c| c.get(choice.original_thread_index.borrow().to_string()))
                        .ok_or("loading choice threads")
                        .unwrap();
                    choice.set_thread_at_generation(
                        Thread::from_json(
                            &main_content_container,
                            j_saved_choice_thread.as_object().unwrap(),
                        )
                        .unwrap(),
                    );
                    Some(())
                });
        }

        Ok(())
    }
}
