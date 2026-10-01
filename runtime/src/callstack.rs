//! Call stack for the ID-based interpreter. Values are mutable run state;
//! instruction locations are compact IDs rather than container `Rc`s.

#[allow(unused_imports)]
use crate::prelude::*;

use crate::{
    compat::{collections::HashMap, rc::Rc},
    push_pop::PushPopType,
    story_content::{ContainerId, ContentPointer},
    story_error::StoryError,
    value::Value,
};

#[cfg(all(
    not(any(feature = "stream-json-parser", feature = "binary-image")),
    feature = "serde-json-parser"
))]
use crate::story_content::StaticStoryView;

#[cfg(all(
    not(any(feature = "stream-json-parser", feature = "binary-image")),
    feature = "serde-json-parser"
))]
use serde_json::{Map, json};

#[cfg(all(
    not(any(feature = "stream-json-parser", feature = "binary-image")),
    feature = "serde-json-parser"
))]
use crate::json::{state_read_serde, state_write_serde};

#[derive(Clone)]
pub(crate) struct CallStackElement {
    pub(crate) current_pointer: ContentPointer,
    pub(crate) in_expression_evaluation: bool,
    pub(crate) temporary_variables: HashMap<String, Rc<Value>>,
    pub(crate) push_pop_type: PushPopType,
    pub(crate) evaluation_stack_height_when_pushed: usize,
    pub(crate) function_start_in_output_stream: i32,
    pub(crate) function_output_started: bool,
}

impl CallStackElement {
    pub(crate) fn new(push_pop_type: PushPopType, pointer: ContentPointer) -> Self {
        Self {
            current_pointer: pointer,
            in_expression_evaluation: false,
            temporary_variables: HashMap::new(),
            push_pop_type,
            evaluation_stack_height_when_pushed: 0,
            function_start_in_output_stream: 0,
            function_output_started: false,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Thread {
    pub(crate) callstack: Vec<CallStackElement>,
    pub(crate) previous_pointer: ContentPointer,
    pub(crate) thread_index: usize,
}

impl Thread {
    fn new() -> Self {
        Self {
            callstack: Vec::new(),
            previous_pointer: ContentPointer::NULL,
            thread_index: 0,
        }
    }

    #[cfg(all(
        not(any(feature = "stream-json-parser", feature = "binary-image")),
        feature = "serde-json-parser"
    ))]
    pub(crate) fn write_json(
        &self,
        data: &impl StaticStoryView,
    ) -> Result<serde_json::Value, StoryError> {
        let mut thread = Map::new();
        let mut elements = Vec::with_capacity(self.callstack.len());
        for element in &self.callstack {
            let mut encoded = Map::new();
            if !element.current_pointer.is_null() {
                let container = element.current_pointer.container.unwrap();
                let path = data.canonical_path_text(container.node()).ok_or_else(|| {
                    StoryError::InvalidStoryState(
                        "callstack pointer has invalid container".to_owned(),
                    )
                })?;
                encoded.insert("cPath".to_owned(), json!(path));
                encoded.insert("idx".to_owned(), json!(element.current_pointer.index));
            }
            encoded.insert("exp".to_owned(), json!(element.in_expression_evaluation));
            encoded.insert("type".to_owned(), json!(element.push_pop_type as u32));
            if !element.temporary_variables.is_empty() {
                encoded.insert(
                    "temp".to_owned(),
                    state_write_serde::write_dictionary_values(&element.temporary_variables)?,
                );
            }
            elements.push(serde_json::Value::Object(encoded));
        }
        thread.insert("callstack".to_owned(), serde_json::Value::Array(elements));
        thread.insert("threadIndex".to_owned(), json!(self.thread_index));
        if !self.previous_pointer.is_null() {
            let previous = self.previous_pointer.path(data).ok_or_else(|| {
                StoryError::InvalidStoryState("previous pointer does not resolve".to_owned())
            })?;
            thread.insert(
                "previousContentObject".to_owned(),
                json!(previous.to_string()),
            );
        }
        Ok(serde_json::Value::Object(thread))
    }

    #[cfg(all(
        not(any(feature = "stream-json-parser", feature = "binary-image")),
        feature = "serde-json-parser"
    ))]
    pub(crate) fn from_json(
        data: &impl StaticStoryView,
        encoded: &serde_json::Value,
    ) -> Result<Self, StoryError> {
        let object = encoded
            .as_object()
            .ok_or_else(|| StoryError::BadJson("callstack thread must be an object".to_owned()))?;
        let thread_index = object
            .get("threadIndex")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| StoryError::BadJson("Invalid thread index".to_owned()))?
            as usize;
        let encoded_elements = object
            .get("callstack")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| StoryError::BadJson("callstack elements not found".to_owned()))?;
        let mut elements = Vec::with_capacity(encoded_elements.len());
        for encoded in encoded_elements {
            let entry = encoded.as_object().ok_or_else(|| {
                StoryError::BadJson("callstack element must be an object".to_owned())
            })?;
            let push_pop_type = PushPopType::from_value(
                entry
                    .get("type")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| StoryError::BadJson("Invalid push/pop type".to_owned()))?
                    as usize,
            )?;
            let pointer = if let Some(path) = entry.get("cPath").and_then(serde_json::Value::as_str)
            {
                let container = data
                    .resolve_path(
                        data.root(),
                        &crate::path::Path::new_with_components_string(Some(path)),
                    )
                    .and_then(|id| data.container_id(id))
                    .ok_or_else(|| {
                        StoryError::BadJson(format!("callstack path '{path}' does not resolve"))
                    })?;
                let index = entry
                    .get("idx")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or_else(|| StoryError::BadJson("Invalid pointer index".to_owned()))?
                    as i32;
                ContentPointer {
                    container: Some(container),
                    index,
                }
            } else {
                ContentPointer::NULL
            };
            let mut element = CallStackElement::new(push_pop_type, pointer);
            element.in_expression_evaluation = entry
                .get("exp")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            if let Some(temps) = entry.get("temp").and_then(serde_json::Value::as_object) {
                element.temporary_variables = state_read_serde::jobject_to_hashmap_values(temps)?;
            }
            elements.push(element);
        }
        let previous_pointer = object
            .get("previousContentObject")
            .and_then(serde_json::Value::as_str)
            .map(|path| {
                data.pointer_at_path(&crate::path::Path::new_with_components_string(Some(path)))
                    .ok_or_else(|| {
                        StoryError::BadJson(format!(
                            "previous pointer path '{path}' does not resolve"
                        ))
                    })
            })
            .transpose()?
            .unwrap_or(ContentPointer::NULL);
        Ok(Self {
            callstack: elements,
            previous_pointer,
            thread_index,
        })
    }
}

#[derive(Clone)]
pub(crate) struct CallStack {
    thread_counter: usize,
    start_of_root: ContentPointer,
    threads: Vec<Thread>,
}

impl CallStack {
    pub(crate) fn new(root: ContainerId) -> Self {
        let mut stack = Self {
            thread_counter: 0,
            start_of_root: ContentPointer::start_of(root),
            threads: Vec::new(),
        };
        stack.reset();
        stack
    }

    pub(crate) fn reset(&mut self) {
        let mut thread = Thread::new();
        thread.callstack.push(CallStackElement::new(
            PushPopType::Tunnel,
            self.start_of_root,
        ));
        self.threads.clear();
        self.threads.push(thread);
    }

    #[cfg(all(
        not(any(feature = "stream-json-parser", feature = "binary-image")),
        feature = "serde-json-parser"
    ))]
    pub(crate) fn write_json(
        &self,
        data: &impl StaticStoryView,
    ) -> Result<serde_json::Value, StoryError> {
        let mut stack = Map::new();
        stack.insert(
            "threads".to_owned(),
            serde_json::Value::Array(
                self.threads
                    .iter()
                    .map(|thread| thread.write_json(data))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        );
        stack.insert("threadCounter".to_owned(), json!(self.thread_counter));
        Ok(serde_json::Value::Object(stack))
    }

    #[cfg(all(
        not(any(feature = "stream-json-parser", feature = "binary-image")),
        feature = "serde-json-parser"
    ))]
    pub(crate) fn load_json(
        &mut self,
        data: &impl StaticStoryView,
        encoded: &serde_json::Value,
    ) -> Result<(), StoryError> {
        let object = encoded
            .as_object()
            .ok_or_else(|| StoryError::BadJson("callstack must be an object".to_owned()))?;
        let thread_counter = object
            .get("threadCounter")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| StoryError::BadJson("Invalid thread counter".to_owned()))?
            as usize;
        let encoded_threads = object
            .get("threads")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| StoryError::BadJson("callstack threads not found".to_owned()))?;
        let threads = encoded_threads
            .iter()
            .map(|thread| Thread::from_json(data, thread))
            .collect::<Result<Vec<_>, _>>()?;
        if threads.is_empty() || threads.iter().any(|thread| thread.callstack.is_empty()) {
            return Err(StoryError::BadJson(
                "callstack has no active frame".to_owned(),
            ));
        }
        self.thread_counter = thread_counter;
        self.threads = threads;
        Ok(())
    }

    pub(crate) fn current_thread(&self) -> &Thread {
        self.threads.last().unwrap()
    }

    pub(crate) fn current_thread_mut(&mut self) -> &mut Thread {
        self.threads.last_mut().unwrap()
    }

    pub(crate) fn set_current_thread(&mut self, thread: Thread) {
        self.threads.clear();
        self.threads.push(thread);
    }

    pub(crate) fn get_thread_with_index(&self, index: usize) -> Option<&Thread> {
        self.threads
            .iter()
            .find(|thread| thread.thread_index == index)
    }

    #[cfg(any(feature = "stream-json-parser", feature = "binary-image"))]
    pub(crate) fn threads(&self) -> &[Thread] {
        &self.threads
    }

    #[cfg(any(feature = "stream-json-parser", feature = "binary-image"))]
    pub(crate) fn thread_counter(&self) -> usize {
        self.thread_counter
    }

    #[cfg(any(feature = "stream-json-parser", feature = "binary-image"))]
    pub(crate) fn replace_threads(
        &mut self,
        threads: Vec<Thread>,
        counter: usize,
    ) -> Result<(), StoryError> {
        if threads.is_empty() || threads.iter().any(|thread| thread.callstack.is_empty()) {
            return Err(StoryError::BadJson(
                "callstack has no active frame".to_owned(),
            ));
        }
        self.threads = threads;
        self.thread_counter = counter;
        Ok(())
    }

    pub(crate) fn current_element(&self) -> &CallStackElement {
        self.current_thread().callstack.last().unwrap()
    }

    pub(crate) fn current_element_index(&self) -> i32 {
        self.current_thread().callstack.len() as i32 - 1
    }

    pub(crate) fn current_element_mut(&mut self) -> &mut CallStackElement {
        self.current_thread_mut().callstack.last_mut().unwrap()
    }

    pub(crate) fn current_pointer(&self) -> ContentPointer {
        self.current_element().current_pointer
    }

    pub(crate) fn set_current_pointer(&mut self, pointer: ContentPointer) {
        self.current_element_mut().current_pointer = pointer;
    }

    pub(crate) fn can_pop(&self) -> bool {
        self.current_thread().callstack.len() > 1
    }

    pub(crate) fn can_pop_type(&self, kind: Option<PushPopType>) -> bool {
        self.can_pop() && kind.is_none_or(|kind| self.current_element().push_pop_type == kind)
    }

    pub(crate) fn pop(&mut self, kind: Option<PushPopType>) -> Result<(), StoryError> {
        if !self.can_pop_type(kind) {
            return Err(StoryError::InvalidStoryState(
                "Mismatched push/pop in Callstack".to_owned(),
            ));
        }
        self.current_thread_mut().callstack.pop();
        Ok(())
    }

    pub(crate) fn push(
        &mut self,
        kind: PushPopType,
        evaluation_stack_height: usize,
        output_stream_length: i32,
    ) {
        let mut element = CallStackElement::new(kind, self.current_pointer());
        element.evaluation_stack_height_when_pushed = evaluation_stack_height;
        element.function_start_in_output_stream = output_stream_length;
        self.current_thread_mut().callstack.push(element);
    }

    pub(crate) fn can_pop_thread(&self) -> bool {
        self.threads.len() > 1
            && self.current_element().push_pop_type != PushPopType::FunctionEvaluationFromGame
    }

    pub(crate) fn push_thread(&mut self) {
        let mut thread = self.current_thread().clone();
        self.thread_counter += 1;
        thread.thread_index = self.thread_counter;
        self.threads.push(thread);
    }

    pub(crate) fn pop_thread(&mut self) -> Result<(), StoryError> {
        if !self.can_pop_thread() {
            return Err(StoryError::InvalidStoryState("Can't pop thread".to_owned()));
        }
        self.threads.pop();
        Ok(())
    }

    pub(crate) fn fork_thread(&mut self) -> Thread {
        let mut thread = self.current_thread().clone();
        self.thread_counter += 1;
        thread.thread_index = self.thread_counter;
        thread
    }

    pub(crate) fn context_for_variable_named(&self, name: &str) -> usize {
        if self
            .current_element()
            .temporary_variables
            .contains_key(name)
        {
            self.current_thread().callstack.len()
        } else {
            0
        }
    }

    pub(crate) fn temporary_variable(&self, name: &str, context_index: i32) -> Option<Rc<Value>> {
        let context_index = if context_index == -1 {
            self.current_thread().callstack.len()
        } else {
            usize::try_from(context_index).ok()?
        };
        self.current_thread()
            .callstack
            .get(context_index.checked_sub(1)?)?
            .temporary_variables
            .get(name)
            .cloned()
    }

    pub(crate) fn set_temporary_variable(
        &mut self,
        name: String,
        value: Rc<Value>,
        declare_new: bool,
        context_index: i32,
    ) -> Result<(), StoryError> {
        let context_index = if context_index == -1 {
            self.current_thread().callstack.len()
        } else {
            usize::try_from(context_index).map_err(|_| {
                StoryError::InvalidStoryState("Invalid temporary variable context".to_owned())
            })?
        };
        let element = self
            .current_thread_mut()
            .callstack
            .get_mut(context_index.checked_sub(1).ok_or_else(|| {
                StoryError::InvalidStoryState("Invalid temporary variable context".to_owned())
            })?)
            .ok_or_else(|| {
                StoryError::InvalidStoryState("Invalid temporary variable context".to_owned())
            })?;

        if !declare_new && !element.temporary_variables.contains_key(&name) {
            return Err(StoryError::InvalidStoryState(format!(
                "Could not find temporary variable to set: {}",
                name
            )));
        }
        if let Some(old) = element.temporary_variables.get(&name) {
            Value::retain_list_origins_for_assignment(old.as_ref(), value.as_ref());
        }
        element.temporary_variables.insert(name, value);
        Ok(())
    }
}

#[cfg(all(
    test,
    any(feature = "stream-json-parser", feature = "serde-json-parser")
))]
mod tests {
    use super::*;
    use crate::story_content::StoryData;

    #[test]
    fn stack_keeps_pointers_and_temporaries_in_run_state() {
        let json = r#"{"inkVersion":21,"root":["done",null],"listDefs":{}}"#;
        let (_, data) = StoryData::from_json_reader(json.as_bytes()).unwrap();
        let root = data.container_id(data.root()).unwrap();
        let mut stack = CallStack::new(root);
        assert_eq!(stack.current_pointer(), ContentPointer::start_of(root));

        stack.push(PushPopType::Function, 0, 0);
        stack
            .set_temporary_variable("x".to_owned(), Rc::new(Value::new(3)), true, -1)
            .unwrap();
        assert_eq!(stack.context_for_variable_named("x"), 2);
        assert!(stack.temporary_variable("x", -1).is_some());

        stack.push_thread();
        assert!(stack.can_pop_thread());
        stack.pop_thread().unwrap();
        stack.pop(Some(PushPopType::Function)).unwrap();
        assert!(!stack.can_pop());
    }
}
