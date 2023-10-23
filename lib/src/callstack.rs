use std::collections::HashMap;

use serde_json::{json, Map};

use crate::{
    container::Container,
    json_read, json_write,
    object::Object,
    path::Path,
    pointer::{self, Pointer},
    push_pop::PushPopType,
    story::Story,
    story_error::StoryError,
    threadsafe::Brc,
    value::Value,
};

pub struct Element {
    pub current_pointer: Pointer,
    pub in_expression_evaluation: bool,
    pub temporary_variables: HashMap<String, Brc<Value>>,
    pub push_pop_type: PushPopType,
    pub evaluation_stack_height_when_pushed: usize,
    pub function_start_in_output_stream: i32,
}

impl Element {
    fn new(
        push_pop_type: PushPopType,
        pointer: Pointer,
        in_expression_evaluation: bool,
    ) -> Element {
        Element {
            current_pointer: pointer,
            in_expression_evaluation,
            temporary_variables: HashMap::new(),
            push_pop_type,
            evaluation_stack_height_when_pushed: 0,
            function_start_in_output_stream: 0,
        }
    }

    fn copy(&self) -> Element {
        let mut copy = Element::new(
            self.push_pop_type,
            self.current_pointer.clone(),
            self.in_expression_evaluation,
        );
        copy.temporary_variables = self.temporary_variables.clone();
        copy.evaluation_stack_height_when_pushed = self.evaluation_stack_height_when_pushed;
        copy.function_start_in_output_stream = self.function_start_in_output_stream;

        copy
    }
}

pub struct Thread {
    pub callstack: Vec<Element>,
    pub previous_pointer: Pointer,
    pub thread_index: usize,
}

impl Thread {
    fn new() -> Thread {
        Thread {
            callstack: Vec::new(),
            previous_pointer: pointer::NULL.clone(),
            thread_index: 0,
        }
    }

    pub fn from_json(
        main_content_container: &Brc<Container>,
        j_obj: &Map<String, serde_json::Value>,
    ) -> Result<Thread, StoryError> {
        let mut thread = Thread::new();

        thread.thread_index = j_obj
            .get("threadIndex")
            .and_then(|i| i.as_i64())
            .ok_or(StoryError::BadJson("Invalid thread index".to_owned()))?
            as usize;

        if let Some(j_thread_callstack) = j_obj
            .get("callstack")
            .and_then(|callstack| callstack.as_array())
        {
            for j_el_tok in j_thread_callstack.iter() {
                if let Some(j_element_obj) = j_el_tok.as_object() {
                    let push_pop_type = PushPopType::from_value(
                        j_element_obj
                            .get("type")
                            .and_then(|t| t.as_i64())
                            .ok_or(StoryError::BadJson("Invalid push/pop type".to_owned()))?
                            as usize,
                    )?;

                    let mut pointer = pointer::NULL.clone();

                    let current_container_path_str =
                        j_element_obj.get("cPath").and_then(|c| c.as_str());
                    if current_container_path_str.is_some() {
                        let thread_pointer_result = main_content_container.content_at_path(
                            &Path::new_with_components_string(current_container_path_str),
                            0,
                            -1,
                        );

                        pointer.container = thread_pointer_result.container();
                        let pointer_index = j_element_obj
                            .get("idx")
                            .and_then(|i| i.as_i64())
                            .ok_or(StoryError::BadJson("Invalid pointer index".to_owned()))?
                            as i32;
                        pointer.index = pointer_index;

                        if thread_pointer_result.approximate {
                            // TODO
                            // story_context.warning(format!("When loading state, exact internal story location couldn't be found: '{}', so it was approximated to '{}' to recover. Has the story changed since this save data was created?", current_container_path_str, pointer_container.get_path().to_string()));
                        }
                    }

                    let in_expression_evaluation = j_element_obj
                        .get("exp")
                        .and_then(|exp| exp.as_bool())
                        .unwrap_or(false);

                    let mut el = Element::new(push_pop_type, pointer, in_expression_evaluation);

                    if let Some(temps) = j_element_obj.get("temp").and_then(|temp| temp.as_object())
                    {
                        el.temporary_variables = json_read::jobject_to_hashmap_values(temps)?;
                    } else {
                        el.temporary_variables.clear();
                    }

                    thread.callstack.push(el);
                }
            }
        }

        if let Some(prev_content_obj_path) =
            j_obj.get("previousContentObject").and_then(|p| p.as_str())
        {
            let prev_path = Path::new_with_components_string(Some(prev_content_obj_path));
            thread.previous_pointer = Story::pointer_at_path(main_content_container, &prev_path)?;
        }

        Ok(thread)
    }

    pub fn copy(&self) -> Thread {
        let mut copy = Thread::new();
        copy.thread_index = self.thread_index;

        for e in self.callstack.iter() {
            copy.callstack.push(e.copy());
        }

        copy.previous_pointer = self.previous_pointer.clone();

        copy
    }

    pub(crate) fn write_json(&self) -> Result<serde_json::Value, StoryError> {
        let mut thread: Map<String, serde_json::Value> = Map::new();

        let mut cs_array: Vec<serde_json::Value> = Vec::new();

        for el in self.callstack.iter() {
            let mut el_map: Map<String, serde_json::Value> = Map::new();

            if !el.current_pointer.is_null() {
                el_map.insert(
                    "cPath".to_owned(),
                    json!(Object::get_path(
                        el.current_pointer.container.as_ref().unwrap().as_ref()
                    )
                    .get_components_string()),
                );
                el_map.insert("idx".to_owned(), json!(el.current_pointer.index));
            }
            el_map.insert("exp".to_owned(), json!(el.in_expression_evaluation));
            el_map.insert("type".to_owned(), json!(el.push_pop_type as u32));

            if !el.temporary_variables.is_empty() {
                el_map.insert(
                    "temp".to_owned(),
                    json_write::write_dictionary_values(&el.temporary_variables)?,
                );
            }

            cs_array.push(serde_json::Value::Object(el_map));
        }

        thread.insert("callstack".to_owned(), serde_json::Value::Array(cs_array));
        thread.insert("threadIndex".to_owned(), json!(self.thread_index));

        if !self.previous_pointer.is_null() {
            thread.insert(
                "previousContentObject".to_owned(),
                json!(
                    Object::get_path(self.previous_pointer.resolve().unwrap().as_ref()).to_string()
                ),
            );
        }

        Ok(serde_json::Value::Object(thread))
    }
}

pub struct CallStack {
    thread_counter: usize,
    start_of_root: Pointer,
    threads: Vec<Thread>,
}

impl CallStack {
    pub fn new(main_content_container: Brc<Container>) -> CallStack {
        let mut cs = CallStack {
            thread_counter: 0,
            start_of_root: Pointer::start_of(main_content_container),
            threads: Vec::new(),
        };

        cs.reset();

        cs
    }

    pub fn new_from(to_copy: &CallStack) -> CallStack {
        let mut cs = CallStack {
            thread_counter: to_copy.thread_counter,
            start_of_root: to_copy.start_of_root.clone(),
            threads: Vec::new(),
        };

        for other_thread in &to_copy.threads {
            cs.threads.push(other_thread.copy());
        }

        cs
    }

    pub fn get_current_element(&self) -> &Element {
        let thread = self.threads.last().unwrap();
        let cs = &thread.callstack;
        cs.last().unwrap()
    }

    pub fn get_current_element_mut(&mut self) -> &mut Element {
        let thread = self.threads.last_mut().unwrap();
        let cs = &mut thread.callstack;
        cs.last_mut().unwrap()
    }

    pub fn get_current_element_index(&self) -> i32 {
        self.get_callstack().len() as i32 - 1
    }

    pub fn reset(&mut self) {
        self.threads.clear();
        self.threads.push(Thread::new());
        self.threads[0].callstack.push(Element::new(
            PushPopType::Tunnel,
            self.start_of_root.clone(),
            false,
        ));
    }

    pub fn can_pop_thread(&self) -> bool {
        self.threads.len() > 1 && !self.element_is_evaluate_from_game()
    }

    pub fn pop_thread(&mut self) -> Result<(), StoryError> {
        if self.can_pop_thread() {
            self.threads.remove(self.threads.len() - 1);
            Ok(())
        } else {
            Err(StoryError::InvalidStoryState("Can't pop thread".to_owned()))
        }
    }

    pub fn push_thread(&mut self) {
        let mut new_thread = self.get_current_thread().copy();
        self.thread_counter += 1;
        new_thread.thread_index = self.thread_counter;
        self.threads.push(new_thread);
    }

    pub fn can_pop(&self) -> bool {
        self.get_callstack().len() > 1
    }

    pub fn can_pop_type(&self, t: Option<PushPopType>) -> bool {
        if !self.can_pop() {
            return false;
        }

        if t.is_none() {
            return true;
        }

        self.get_current_element().push_pop_type == t.unwrap()
    }

    pub fn pop(&mut self, t: Option<PushPopType>) -> Result<(), StoryError> {
        if self.can_pop_type(t) {
            let l = self.get_callstack().len() - 1;
            self.get_callstack_mut().remove(l);
        } else {
            return Err(StoryError::InvalidStoryState(
                "Mismatched push/pop in Callstack".to_owned(),
            ));
        }

        Ok(())
    }

    pub fn element_is_evaluate_from_game(&self) -> bool {
        self.get_current_element().push_pop_type == PushPopType::FunctionEvaluationFromGame
    }

    pub fn get_elements_mut(&mut self) -> &mut Vec<Element> {
        self.get_callstack_mut()
    }

    pub fn get_callstack(&self) -> &Vec<Element> {
        &self.get_current_thread().callstack
    }

    pub fn get_callstack_mut(&mut self) -> &mut Vec<Element> {
        &mut self.get_current_thread_mut().callstack
    }

    pub fn get_current_thread(&self) -> &Thread {
        self.threads.last().unwrap()
    }

    pub fn get_current_thread_mut(&mut self) -> &mut Thread {
        self.threads.last_mut().unwrap()
    }

    pub fn set_current_thread(&mut self, value: Thread) {
        // Debug.Assert (threads.Count == 1, "Shouldn't be directly setting the
        // current thread when we have a stack of them");
        self.threads.clear();
        self.threads.push(value);
    }

    pub fn fork_thread(&mut self) -> Thread {
        let mut forked_thread = self.get_current_thread().copy();
        self.thread_counter += 1;
        forked_thread.thread_index = self.thread_counter;
        forked_thread
    }

    pub fn set_temporary_variable(
        &mut self,
        name: String,
        value: Brc<Value>,
        declare_new: bool,
        mut context_index: i32,
    ) -> Result<(), StoryError> {
        if context_index == -1 {
            context_index = self.get_current_element_index() + 1;
        }

        let context_element = self
            .get_callstack_mut()
            .get_mut((context_index - 1) as usize)
            .unwrap();

        if !declare_new && !context_element.temporary_variables.contains_key(&name) {
            return Err(StoryError::InvalidStoryState(format!(
                "Could not find temporary variable to set: {}",
                name
            )));
        }

        let old_value = context_element.temporary_variables.get(&name).cloned();

        if let Some(old_value) = &old_value {
            Value::retain_list_origins_for_assignment(old_value.as_ref(), value.as_ref());
        }

        context_element.temporary_variables.insert(name, value);

        Ok(())
    }

    pub fn context_for_variable_named(&self, name: &str) -> usize {
        // Check if the current temporary context contains the variable.
        if self
            .get_current_element()
            .temporary_variables
            .contains_key(name)
        {
            return (self.get_current_element_index() + 1) as usize;
        }

        // Otherwise, it's a global variable.
        0
    }

    pub fn get_temporary_variable_with_name(
        &self,
        name: &str,
        context_index: i32,
    ) -> Option<Brc<Value>> {
        let mut context_index = context_index;
        if context_index == -1 {
            context_index = self.get_current_element_index() + 1;
        }

        let context_element = self.get_callstack().get((context_index - 1) as usize);
        let var_value = context_element.unwrap().temporary_variables.get(name);

        var_value.cloned()
    }

    pub fn push(
        &mut self,
        t: PushPopType,
        external_evaluation_stack_height: usize,
        output_stream_length_with_pushed: i32,
    ) {
        // When pushing to callstack, maintain the current content path, but
        // jump
        // out of expressions by default
        let mut element =
            Element::new(t, self.get_current_element().current_pointer.clone(), false);

        element.evaluation_stack_height_when_pushed = external_evaluation_stack_height;
        element.function_start_in_output_stream = output_stream_length_with_pushed;

        self.get_callstack_mut().push(element);
    }

    pub(crate) fn write_json(&self) -> Result<serde_json::Value, StoryError> {
        let mut cs: Map<String, serde_json::Value> = Map::new();

        let mut treads_array: Vec<serde_json::Value> = Vec::new();

        for thread in &self.threads {
            treads_array.push(thread.write_json()?);
        }

        cs.insert("threads".to_owned(), serde_json::Value::Array(treads_array));
        cs.insert("threadCounter".to_owned(), json!(self.thread_counter));

        Ok(serde_json::Value::Object(cs))
    }

    pub fn get_thread_with_index(&self, index: usize) -> Option<&Thread> {
        self.threads.iter().find(|&t| t.thread_index == index)
    }

    pub fn load_json(
        &mut self,
        main_content_container: &Brc<Container>,
        j_obj: &Map<String, serde_json::Value>,
    ) -> Result<(), StoryError> {
        self.threads.clear();

        let j_threads = j_obj.get("threads").unwrap();

        for j_thread_tok in j_threads.as_array().unwrap().iter() {
            let j_thread_obj = j_thread_tok.as_object().unwrap();
            let thread = Thread::from_json(main_content_container, j_thread_obj)?;
            self.threads.push(thread);
        }

        self.thread_counter = j_obj.get("threadCounter").unwrap().as_i64().unwrap() as usize;
        self.start_of_root = Pointer::start_of(main_content_container.clone()).clone();

        Ok(())
    }

    pub fn get_callstack_trace(&self) -> String {
        let mut sb = String::new();

        for (t, thread) in self.threads.iter().enumerate() {
            let is_current = t == self.threads.len() - 1;

            sb.push_str(&format!(
                "=== THREAD {}/{} {}===",
                t + 1,
                self.threads.len(),
                if is_current { &"(current) " } else { &"" }
            ));

            for element in &thread.callstack {
                if element.push_pop_type == PushPopType::Function {
                    sb.push_str("  [FUNCTION] ");
                } else {
                    sb.push_str("  [TUNNEL] ");
                }

                let pointer = &element.current_pointer;

                if !pointer.is_null() {
                    sb.push_str(&format!(
                        "<SOMEWHERE IN {}>\n",
                        pointer.container.as_ref().unwrap().get_path()
                    ))
                }
            }
        }

        sb
    }
}
