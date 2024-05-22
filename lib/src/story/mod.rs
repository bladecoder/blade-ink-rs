//! [`Story`] is the entry point to load and run an Ink story.
use crate::{
    container::Container,
    list_definitions_origin::ListDefinitionsOrigin,
    story::{
        errors::ErrorHandler, external_functions::ExternalFunctionDef,
        variable_observer::VariableObserver,
    },
    story_state::StoryState,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// The current version of the Ink story file format.
pub const INK_VERSION_CURRENT: i32 = 21;
/// The minimum legacy version of ink that can be loaded by the current version
/// of the code.
pub const INK_VERSION_MINIMUM_COMPATIBLE: i32 = 18;

#[derive(PartialEq)]
pub(crate) enum OutputStateChange {
    NoChange,
    ExtendedBeyondNewline,
    NewlineRemoved,
}

/// A `Story` is the core struct representing a complete Ink narrative,
/// managing evaluation and state.
pub struct Story {
    main_content_container: Rc<Container>,
    state: StoryState,
    temporary_evaluation_container: Option<Rc<Container>>,
    recursive_continue_count: usize,
    async_continue_active: bool,
    async_saving: bool,
    prev_containers: Vec<Rc<Container>>,
    list_definitions: Rc<ListDefinitionsOrigin>,
    pub(crate) on_error: Option<Rc<RefCell<dyn ErrorHandler>>>,
    pub(crate) state_snapshot_at_last_new_line: Option<StoryState>,
    pub(crate) variable_observers: HashMap<String, Vec<Rc<RefCell<dyn VariableObserver>>>>,
    pub(crate) has_validated_externals: bool,
    pub(crate) allow_external_function_fallbacks: bool,
    pub(crate) saw_lookahead_unsafe_function_after_new_line: bool,
    pub(crate) externals: HashMap<String, ExternalFunctionDef>,
}
mod misc {
    use crate::{
        json::{json_read, json_read_stream},
        object::{Object, RTObject},
        story::{Story, INK_VERSION_CURRENT},
        story_error::StoryError,
        story_state::StoryState,
        value::Value,
    };
    use rand::{rngs::StdRng, Rng, SeedableRng};
    use std::{collections::HashMap, rc::Rc};

    impl Story {
        /// Construct a `Story` out of a JSON string that was compiled with
        /// `inklecate`.
        pub fn new(json_string: &str) -> Result<Self, StoryError> {
            let (version, main_content_container, list_definitions) =
                if cfg!(feature = "stream-json-parser") {
                    json_read_stream::load_from_string(json_string)?
                } else {
                    json_read::load_from_string(json_string)?
                };

            let mut story = Story {
                main_content_container: main_content_container.clone(),
                state: StoryState::new(main_content_container.clone(), list_definitions.clone()),
                temporary_evaluation_container: None,
                recursive_continue_count: 0,
                async_continue_active: false,
                async_saving: false,
                saw_lookahead_unsafe_function_after_new_line: false,
                state_snapshot_at_last_new_line: None,
                on_error: None,
                prev_containers: Vec::new(),
                list_definitions,
                variable_observers: HashMap::with_capacity(0),
                has_validated_externals: false,
                allow_external_function_fallbacks: false,
                externals: HashMap::with_capacity(0),
            };

            story.reset_globals()?;

            if version != INK_VERSION_CURRENT {
                story.add_error(&format!("WARNING: Version of ink used to build story ({}) doesn't match current version ({}) of engine. Non-critical, but recommend synchronising.", version, INK_VERSION_CURRENT), true);
            }

            Ok(story)
        }

        /// Creates a string representing the hierarchy of objects and
        /// containers in a story.
        pub fn build_string_of_hierarchy(&self) -> String {
            let mut sb = String::new();

            let cp = self.get_state().get_current_pointer().resolve();

            let cp = cp.as_ref().map(|cp| cp.as_ref());

            self.main_content_container
                .build_string_of_hierarchy(&mut sb, 0, cp);

            sb
        }

        pub(crate) fn is_truthy(&self, obj: Rc<dyn RTObject>) -> Result<bool, StoryError> {
            let truthy = false;

            if let Some(val) = obj.as_ref().as_any().downcast_ref::<Value>() {
                if let Some(target_path) = Value::get_divert_target_value(obj.as_ref()) {
                    return Err(StoryError::InvalidStoryState(format!("Shouldn't use a divert target (to {}) as a conditional value. Did you intend a function call 'likeThis()' or a read count check 'likeThis'? (no arrows)", target_path)));
                }

                return val.is_truthy();
            }

            Ok(truthy)
        }

        pub(crate) fn next_sequence_shuffle_index(&mut self) -> Result<i32, StoryError> {
            let pop_evaluation_stack = self.get_state_mut().pop_evaluation_stack();
            let num_elements = if let Some(v) = Value::get_int_value(pop_evaluation_stack.as_ref())
            {
                v
            } else {
                return Err(StoryError::InvalidStoryState(
                    "Expected number of elements in sequence for shuffle index".to_owned(),
                ));
            };

            let seq_container = self.get_state().get_current_pointer().container.unwrap();

            let seq_count = if let Some(v) =
                Value::get_int_value(self.get_state_mut().pop_evaluation_stack().as_ref())
            {
                v
            } else {
                return Err(StoryError::InvalidStoryState(
                    "Expected sequence count value for shuffle index".to_owned(),
                ));
            };

            let loop_index = seq_count / num_elements;
            let iteration_index = seq_count % num_elements;

            // Generate the same shuffle based on:
            // - The hash of this container, to make sure it's consistent each time the
            //   runtime returns to the sequence
            // - How many times the runtime has looped around this full shuffle
            let seq_path_str = Object::get_path(seq_container.as_ref()).to_string();
            let sequence_hash: i32 = seq_path_str.chars().map(|c| c as i32).sum();
            let random_seed = sequence_hash + loop_index + self.get_state().story_seed;

            let mut rng = StdRng::seed_from_u64(random_seed as u64);

            let mut unpicked_indices: Vec<i32> = (0..num_elements).collect();

            for i in 0..=iteration_index {
                let chosen = rng.gen::<i32>().rem_euclid(unpicked_indices.len() as i32);
                let chosen_index = unpicked_indices[chosen as usize];
                unpicked_indices.retain(|&x| x != chosen_index);

                if i == iteration_index {
                    return Ok(chosen_index);
                }
            }

            Err(StoryError::InvalidStoryState(
                "Should never reach here".to_owned(),
            ))
        }
    }
}

mod choices;
mod control_logic;
pub mod errors;
pub mod external_functions;
mod flow;
mod navigation;
mod progress;
mod state;
mod tags;
pub mod variable_observer;
