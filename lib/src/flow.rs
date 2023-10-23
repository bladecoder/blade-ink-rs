use serde_json::Map;

use crate::{
    callstack::{CallStack, Thread},
    choice::Choice,
    container::Container,
    json_read, json_write,
    object::RTObject,
    story_error::StoryError,
    BrCell, Brc,
};

#[derive(Clone)]
pub(crate) struct Flow {
    pub name: String,
    pub callstack: Brc<BrCell<CallStack>>,
    pub output_stream: Vec<Brc<dyn RTObject>>,
    pub current_choices: Vec<Brc<Choice>>,
}

impl Flow {
    pub fn new(name: &str, main_content_container: Brc<Container>) -> Flow {
        Flow {
            name: name.to_string(),
            callstack: Brc::new(BrCell::new(CallStack::new(main_content_container))),
            output_stream: Vec::new(),
            current_choices: Vec::new(),
        }
    }

    pub fn from_json(
        name: &str,
        main_content_container: Brc<Container>,
        j_obj: &Map<String, serde_json::Value>,
    ) -> Result<Flow, StoryError> {
        let mut flow = Self {
            name: name.to_string(),
            callstack: Brc::new(BrCell::new(CallStack::new(main_content_container.clone()))),
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
            .collect::<Vec<Brc<Choice>>>(),
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

    pub fn load_flow_choice_threads(
        &mut self,
        j_choice_threads: Option<&serde_json::Value>,
        main_content_container: Brc<Container>,
    ) -> Result<(), StoryError> {
        for choice in self.current_choices.iter_mut() {
            self.callstack
                .borrow()
                .get_thread_with_index(*choice.original_thread_index.borrow())
                .map(|o| choice.set_thread_at_generation(o.copy()))
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
