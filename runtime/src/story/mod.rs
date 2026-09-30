//! [`Story`] is the entry point to load and run an Ink story.
use crate::compat::{cell::RefCell, collections::HashMap, rc::Rc};
#[allow(unused_imports)]
use crate::prelude::*;
use crate::{
    container::Container,
    list_definitions_origin::ListDefinitionsOrigin,
    story::{
        errors::ErrorHandler,
        external_functions::ExternalFunctionDef,
        variable_observer::{VariableObserverDef, VariableObserverHandle},
    },
    story_state::StoryState,
};

/// Supplies elapsed time to time-limited story continuation.
pub trait TimeSource {
    /// Returns a monotonically increasing duration.
    fn now(&self) -> core::time::Duration;
}

impl<F> TimeSource for F
where
    F: Fn() -> core::time::Duration,
{
    fn now(&self) -> core::time::Duration {
        self()
    }
}

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
    pub(crate) variable_observers: HashMap<String, Vec<VariableObserverHandle>>,
    pub(crate) variable_observer_defs: HashMap<VariableObserverHandle, VariableObserverDef>,
    pub(crate) next_variable_observer_id: u64,
    pub(crate) has_validated_externals: bool,
    pub(crate) allow_external_function_fallbacks: bool,
    pub(crate) saw_lookahead_unsafe_function_after_new_line: bool,
    pub(crate) externals: HashMap<String, ExternalFunctionDef>,
    pub(crate) fixed_seed: Option<i32>,
    pub(crate) time_source: Option<Rc<dyn TimeSource>>,
    pub(crate) last_time: Option<core::time::Duration>,
}
mod misc {
    #[allow(unused_imports)]
    use crate::prelude::*;

    use crate::compat::{collections::HashMap, io::Read, rc::Rc};
    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    use crate::json::json_read;
    #[cfg(feature = "stream-json-parser")]
    use crate::json::json_read_stream;
    use crate::{
        ink_list::InkList,
        ink_list_item::InkListItem,
        object::{Object, RTObject},
        path::Path,
        story::{INK_VERSION_CURRENT, Story},
        story_error::StoryError,
        story_state::StoryState,
        value::Value,
    };
    use rand::{RngExt, SeedableRng, rngs::StdRng};

    impl Story {
        /// Construct a `Story` out of a JSON string that was compiled with
        /// `inklecate`.
        #[cfg(feature = "std")]
        pub fn new(json_string: &str) -> Result<Self, StoryError> {
            Self::new_from_reader(json_string.as_bytes())
        }

        /// Construct a `Story` from a JSON reader without requiring an
        /// additional in-memory copy of the source document.
        #[cfg(feature = "std")]
        pub fn new_from_reader(reader: impl Read) -> Result<Self, StoryError> {
            let seed = rand::rng().random_range(0..100);
            Self::new_from_reader_internal(reader, seed, None)
        }

        /// Construct a `Story` from JSON with a reproducible random seed.
        pub fn new_with_seed(json_string: &str, seed: i32) -> Result<Self, StoryError> {
            Self::new_from_reader_with_seed(json_string.as_bytes(), seed)
        }

        /// Construct a `Story` from a JSON reader with a reproducible random seed.
        pub fn new_from_reader_with_seed(reader: impl Read, seed: i32) -> Result<Self, StoryError> {
            Self::new_from_reader_internal(reader, seed, Some(seed))
        }

        fn new_from_reader_internal(
            reader: impl Read,
            seed: i32,
            fixed_seed: Option<i32>,
        ) -> Result<Self, StoryError> {
            #[cfg(feature = "stream-json-parser")]
            let (version, main_content_container, list_definitions) =
                json_read_stream::load_from_reader(reader)?;

            #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
            let (version, main_content_container, list_definitions) =
                json_read::load_from_reader(reader)?;

            let mut story = Story {
                main_content_container: main_content_container.clone(),
                state: StoryState::new(
                    main_content_container.clone(),
                    list_definitions.clone(),
                    seed,
                ),
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
                variable_observer_defs: HashMap::with_capacity(0),
                next_variable_observer_id: 0,
                has_validated_externals: false,
                allow_external_function_fallbacks: false,
                externals: HashMap::with_capacity(0),
                fixed_seed,
                time_source: Self::default_time_source(),
                last_time: None,
            };

            story.reset_globals()?;

            if version != INK_VERSION_CURRENT {
                story.add_error(&format!("WARNING: Version of ink used to build story ({}) doesn't match current version ({}) of engine. Non-critical, but recommend synchronising.", version, INK_VERSION_CURRENT), true);
            }

            Ok(story)
        }

        #[cfg(feature = "std")]
        fn default_time_source() -> Option<Rc<dyn super::TimeSource>> {
            let origin = web_time::Instant::now();
            Some(Rc::new(move || origin.elapsed()))
        }

        #[cfg(not(feature = "std"))]
        fn default_time_source() -> Option<Rc<dyn super::TimeSource>> {
            None
        }

        /// Overrides the clock used by time-limited continuation.
        pub fn set_time_source(&mut self, time_source: impl super::TimeSource + 'static) {
            self.time_source = Some(Rc::new(time_source));
            self.last_time = None;
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

        /// Creates an empty Ink list associated with the named list origin.
        ///
        /// The returned value retains the origin definition, so operations such
        /// as `LIST_ALL` and `LIST_INVERT` continue to behave correctly when it
        /// is assigned back to the story.
        pub fn list_from_origin(&self, origin_name: &str) -> Result<InkList, StoryError> {
            InkList::from_single_origin(origin_name.to_owned(), &self.list_definitions)
        }

        /// Creates a one-item Ink list using the value declared by the story's
        /// list definition.
        pub fn list_from_item(&self, full_item_name: &str) -> Result<InkList, StoryError> {
            let item = InkListItem::from_full_name(full_item_name);
            let Some(origin_name) = item.get_origin_name() else {
                return Err(StoryError::BadArgument(format!(
                    "List item '{full_item_name}' must use the 'origin.item' format."
                )));
            };
            let Some(definition) = self.list_definitions.get_list_definition(origin_name) else {
                return Err(StoryError::BadArgument(format!(
                    "List origin '{origin_name}' does not exist."
                )));
            };
            let Some(value) = definition.get_value_for_item(&item) else {
                return Err(StoryError::BadArgument(format!(
                    "List item '{full_item_name}' does not exist."
                )));
            };

            let mut list = self.list_from_origin(origin_name)?;
            list.items.insert(item, *value);
            Ok(list)
        }

        pub(crate) fn is_truthy(&self, obj: Rc<dyn RTObject>) -> Result<bool, StoryError> {
            let truthy = false;

            if let Some(val) = obj.as_ref().as_any().downcast_ref::<Value>() {
                if let Some(target_path) = Value::get_value::<&Path>(obj.as_ref()) {
                    return Err(StoryError::InvalidStoryState(format!(
                        "Shouldn't use a divert target (to {}) as a conditional value. Did you intend a function call 'likeThis()' or a read count check 'likeThis'? (no arrows)",
                        target_path
                    )));
                }

                return val.is_truthy();
            }

            Ok(truthy)
        }

        pub(crate) fn next_sequence_shuffle_index(&mut self) -> Result<i32, StoryError> {
            let pop_evaluation_stack = self.get_state_mut().pop_evaluation_stack();
            let num_elements =
                if let Some(v) = Value::get_value::<i32>(pop_evaluation_stack.as_ref()) {
                    v
                } else {
                    return Err(StoryError::InvalidStoryState(
                        "Expected number of elements in sequence for shuffle index".to_owned(),
                    ));
                };

            let seq_container = self.get_state().get_current_pointer().container.unwrap();

            let seq_count = if let Some(v) =
                Value::get_value::<i32>(self.get_state_mut().pop_evaluation_stack().as_ref())
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
                let chosen = rng
                    .random::<i32>()
                    .rem_euclid(unpicked_indices.len() as i32);
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

#[cfg(test)]
mod tests {
    use super::Story;
    #[allow(unused_imports)]
    use crate::prelude::*;
    use core::{cell::Cell, time::Duration};

    #[cfg(feature = "stream-json-parser")]
    struct OneByteReader<'a>(&'a [u8]);

    #[cfg(feature = "stream-json-parser")]
    impl crate::compat::io::Read for OneByteReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> crate::compat::io::Result<usize> {
            if self.0.is_empty() || buffer.is_empty() {
                return Ok(0);
            }
            buffer[0] = self.0[0];
            self.0 = &self.0[1..];
            Ok(1)
        }
    }

    #[cfg(feature = "stream-json-parser")]
    struct OneByteWriter(Vec<u8>);

    #[cfg(feature = "stream-json-parser")]
    impl crate::compat::io::Write for OneByteWriter {
        fn write(&mut self, buffer: &[u8]) -> crate::compat::io::Result<usize> {
            if buffer.is_empty() {
                return Ok(0);
            }
            self.0.push(buffer[0]);
            Ok(1)
        }

        fn flush(&mut self) -> crate::compat::io::Result<()> {
            Ok(())
        }
    }

    const STORY_WITH_LIST: &str = r#"{
        "inkVersion": 21,
        "root": [["done", null], "done", {
            "global decl": ["ev", {"list": {}, "origins": ["items"]}, {"VAR=": "items"}, "/ev", "end", null]
        }],
        "listDefs": {"items": {"one": 1, "two": 2}}
    }"#;

    #[test]
    fn constructs_lists_with_story_definitions() {
        let story = Story::new_with_seed(STORY_WITH_LIST, 1).expect("story should load");

        let empty = story
            .list_from_origin("items")
            .expect("origin should exist");
        assert!(empty.items.is_empty());
        assert_eq!(empty.get_origin_names(), vec!["items"]);

        let item = story
            .list_from_item("items.two")
            .expect("item should exist");
        assert_eq!(item.items.len(), 1);
        assert_eq!(item.get_origin_names(), vec!["items"]);
    }

    #[test]
    fn rejects_unknown_list_items() {
        let story = Story::new_with_seed(STORY_WITH_LIST, 1).expect("story should load");
        assert!(story.list_from_origin("unknown").is_err());
        assert!(story.list_from_item("items.unknown").is_err());
        assert!(story.list_from_item("two").is_err());
    }

    #[test]
    fn fixed_seed_is_reproducible_across_reset() {
        const RANDOM_STORY: &str = r##"{"inkVersion":21,"root":[["ev",1,100,"rnd","out","/ev","^,","ev",1,100,"rnd","out","/ev","^,","ev",1,100,"rnd","out","/ev",["done",{"#f":5,"#n":"g-0"}],null],"done",{"#f":1}],"listDefs":{}}"##;
        let mut first = Story::new_with_seed(RANDOM_STORY, 42).unwrap();
        let mut second = Story::new_with_seed(RANDOM_STORY, 42).unwrap();
        let first_run = first.continue_maximally().unwrap();
        assert_eq!(first_run, second.continue_maximally().unwrap());

        first.reset_state().unwrap();
        assert_eq!(first_run, first.continue_maximally().unwrap());
    }

    #[test]
    fn positive_async_limit_requires_a_clock_before_mutating_state() {
        let mut story = Story::new_with_seed(STORY_WITH_LIST, 1).unwrap();
        story.time_source = None;

        let error = story.continue_async(1.0).unwrap_err();
        assert!(matches!(
            error,
            crate::story_error::StoryError::BadArgument(_)
        ));
        assert!(story.can_continue());
        assert!(!story.is_async_continue_active());
    }

    #[test]
    fn zero_async_limit_does_not_require_a_clock() {
        let mut story = Story::new_with_seed(STORY_WITH_LIST, 1).unwrap();
        story.time_source = None;
        story.continue_async(0.0).unwrap();
    }

    #[test]
    fn async_continuation_can_pause_and_resume_with_a_simulated_clock() {
        const STORY: &str =
            r#"{"inkVersion":21,"root":["^one","^two","^three","done",null],"listDefs":{}}"#;
        let mut story = Story::new_with_seed(STORY, 1).unwrap();
        let tick = Rc::new(Cell::new(0_u64));
        let clock = tick.clone();
        story.set_time_source(move || {
            let next = clock.get() + 2;
            clock.set(next);
            Duration::from_millis(next)
        });

        story.continue_async(1.0).unwrap();
        assert!(story.is_async_continue_active());
        for _ in 0..16 {
            if !story.is_async_continue_active() {
                break;
            }
            story.continue_async(1.0).unwrap();
        }
        assert!(!story.is_async_continue_active());
    }

    #[test]
    fn backwards_clock_is_rejected() {
        const STORY: &str = r#"{"inkVersion":21,"root":["^text","done",null],"listDefs":{}}"#;
        let mut story = Story::new_with_seed(STORY, 1).unwrap();
        let first = Cell::new(true);
        story.set_time_source(move || {
            if first.replace(false) {
                Duration::from_millis(2)
            } else {
                Duration::from_millis(1)
            }
        });

        let error = story.continue_async(1.0).unwrap_err();
        assert!(matches!(
            error,
            crate::story_error::StoryError::InvalidStoryState(_)
        ));
    }

    #[cfg(feature = "stream-json-parser")]
    #[test]
    fn streams_reordered_story_and_state_documents() {
        let json = br#"{
            "unknown": {"nested": [1, true, null]},
            "listDefs": {},
            "root": ["done", null],
            "inkVersion": 21
        }"#;
        let mut story = Story::new_from_reader_with_seed(OneByteReader(json), 1).unwrap();

        let mut state = OneByteWriter(Vec::new());
        story.save_state_to_writer(&mut state).unwrap();
        story
            .load_state_from_reader(OneByteReader(&state.0))
            .unwrap();
    }

    #[cfg(feature = "stream-json-parser")]
    #[test]
    fn malformed_json_returns_an_error() {
        for json in [
            r#"{"inkVersion":21,"root":[],"listDefs":{} trailing"#,
            r#"{"inkVersion":21.5,"root":["done",null],"listDefs":{}}"#,
            r#"{"inkVersion":21,"root":["",null],"listDefs":{}}"#,
        ] {
            assert!(
                Story::new_with_seed(json, 1).is_err(),
                "input should fail: {json}"
            );
        }
    }
}
