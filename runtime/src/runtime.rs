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
    runtime::counters::StoryCounters,
    story::error::StoryError,
    story::external_functions::{ExternalFunction, ExternalFunctionResult},
    story_content::{
        ContainerId, ContentPointer, NodeId, NodeKindView, StaticStoryView, StoryContent,
        StoryData, ValueView,
    },
    tag::Tag,
    value::Value,
    value_type::{StringValue, ValueType},
    variable_assignment::VariableAssignment,
    variables_state::VariablesState,
    void::Void,
};
use rand::{RngExt, SeedableRng, rngs::StdRng};

mod commands;
mod counters;
mod engine;
mod external;
mod flow;
mod navigation;

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
    pub(crate) data: Rc<StoryContent>,
    pub(crate) callstack: Rc<RefCell<CallStack>>,
    pub(crate) variables: VariablesState,
    pub(crate) counters: StoryCounters,
    pub(crate) evaluation_stack: Vec<Rc<dyn RTObject>>,
    pub(crate) output: String,
    string_stack: Vec<String>,
    tag_stack: Vec<String>,
    pub(crate) current_tags: Vec<String>,
    glue_active: bool,
    pub(crate) diverted: ContentPointer,
    did_safe_exit: bool,
    pub(crate) choices: Vec<RuntimeChoice>,
    pub(crate) current_turn_index: i32,
    pub(crate) story_seed: i32,
    pub(crate) previous_random: i32,
    externals: Rc<RefCell<HashMap<String, BoundExternal>>>,
    allow_external_fallbacks: bool,
    lookahead_active: bool,
    lookahead_unsafe_external: bool,
    pub(crate) current_flow_name: String,
    pub(crate) named_flows: HashMap<String, RuntimeFlow>,
    changed_variables: HashMap<String, ValueType>,
    previsited_container: Option<ContainerId>,
    continuation_active: bool,
    newline_snapshot: Option<Box<Runtime>>,
}

struct BoundExternal {
    function: Box<dyn ExternalFunction>,
    lookahead_safe: bool,
}

pub(crate) struct RuntimeFlow {
    pub(crate) callstack: Rc<RefCell<CallStack>>,
    pub(crate) output: String,
    pub(crate) choices: Vec<RuntimeChoice>,
    pub(crate) current_tags: Vec<String>,
}

impl RuntimeFlow {
    pub(crate) fn new(root: ContainerId) -> Self {
        Self {
            callstack: Rc::new(RefCell::new(CallStack::new(root))),
            output: String::new(),
            choices: Vec::new(),
            current_tags: Vec::new(),
        }
    }

    pub(crate) fn snapshot(&self) -> Self {
        Self {
            callstack: Rc::new(RefCell::new(self.callstack.borrow().clone())),
            output: self.output.clone(),
            choices: self.choices.clone(),
            current_tags: self.current_tags.clone(),
        }
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
