//! Public entry point for the arena-based interpreter.
//!
//! It uses the same compiled Ink JSON as [`crate::story::Story`] while keeping
//! static instructions in an owned arena without a runtime object tree.

#[allow(unused_imports)]
use crate::prelude::*;

use crate::{
    choice::Choice,
    compat::collections::{HashMap, HashSet},
    compat::io::{Read, Write},
    compat::{cell::RefCell, rc::Rc},
    flat_runtime::FlatRuntime,
    flat_story::{FlatStoryData, LoadObserver, LoadPhase, NoopLoadObserver},
    ink_list::InkList,
    story::variable_observer::{VariableObserver, VariableObserverHandle, VariableObserverResult},
    story::{
        INK_VERSION_CURRENT, TimeSource,
        errors::{ErrorHandler, ErrorType},
        external_functions::{ExternalFunction, ExternalFunctionResult},
    },
    story_error::StoryError,
    value_type::ValueType,
};

#[cfg(feature = "load-profile")]
pub use crate::flat_story::FlatLoadProfile;

#[cfg(feature = "load-profile")]
use crate::flat_story::TimedLoadObserver;

/// A lightweight view of an available Ink choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatChoiceInfo {
    /// Text visible to the player.
    pub text: String,
    /// Tags attached to this choice.
    pub tags: Vec<String>,
}

/// Runs an Ink story through flat, ID-addressed static content.
///
/// This is the backing implementation of [`crate::story::Story`]. Construction
/// from JSON still allocates the arena in RAM; binary image loading belongs
/// to a later milestone.
pub struct FlatStory {
    runtime: FlatRuntime,
    fixed_seed: Option<i32>,
    observers: HashMap<VariableObserverHandle, Box<dyn VariableObserver>>,
    subscriptions: HashMap<String, Vec<VariableObserverHandle>>,
    next_observer_id: u64,
    on_error: Option<Rc<RefCell<dyn ErrorHandler>>>,
    errors: Vec<String>,
    warnings: Vec<String>,
    time_source: Option<Rc<dyn TimeSource>>,
    last_time: Option<core::time::Duration>,
    choice_cache: RefCell<Option<Vec<Rc<Choice>>>>,
    externals_validated: bool,
}

impl FlatStory {
    /// Builds a story from compiled Ink JSON with a reproducible random seed.
    pub fn new_with_seed(json: &str, seed: i32) -> Result<Self, StoryError> {
        Self::new_from_reader_with_seed(json.as_bytes(), seed)
    }

    /// Builds a story directly from a JSON reader with a reproducible seed.
    pub fn new_from_reader_with_seed(reader: impl Read, seed: i32) -> Result<Self, StoryError> {
        Self::new_from_reader_with_seed_observed(reader, seed, &mut NoopLoadObserver)
    }

    /// Builds a story and reports each construction phase. Available for
    /// diagnostic benchmarks when `load-profile` is enabled.
    #[cfg(feature = "load-profile")]
    pub fn new_with_seed_profiled(
        json: &str,
        seed: i32,
    ) -> Result<(Self, FlatLoadProfile), StoryError> {
        Self::new_from_reader_with_seed_profiled(json.as_bytes(), seed)
    }

    /// Profiles construction from an arbitrary JSON reader.
    #[cfg(feature = "load-profile")]
    pub fn new_from_reader_with_seed_profiled(
        reader: impl Read,
        seed: i32,
    ) -> Result<(Self, FlatLoadProfile), StoryError> {
        let mut observer = TimedLoadObserver::new();
        let story = Self::new_from_reader_with_seed_observed(reader, seed, &mut observer)?;
        Ok((story, observer.profile))
    }

    fn new_from_reader_with_seed_observed(
        reader: impl Read,
        seed: i32,
        observer: &mut impl LoadObserver,
    ) -> Result<Self, StoryError> {
        let (version, data) = FlatStoryData::from_json_reader_observed(reader, observer)?;
        let mut story = Self {
            runtime: FlatRuntime::new_with_seed(data, seed)?,
            fixed_seed: Some(seed),
            observers: HashMap::new(),
            subscriptions: HashMap::new(),
            next_observer_id: 0,
            on_error: None,
            errors: Vec::new(),
            warnings: Vec::new(),
            time_source: Self::default_time_source(),
            last_time: None,
            choice_cache: RefCell::new(None),
            externals_validated: false,
        };
        if version != INK_VERSION_CURRENT {
            let message = format!(
                "WARNING: Version of ink used to build story ({version}) doesn't match current version ({INK_VERSION_CURRENT}) of engine. Non-critical, but recommend synchronising."
            );
            let path = story.get_current_path().unwrap_or_default();
            story.warnings.push(if path.is_empty() {
                format!("RUNTIME WARNING: {message}")
            } else {
                format!("RUNTIME WARNING: ({path}): {message}")
            });
        }
        observer.record(LoadPhase::RuntimeInitialized);
        Ok(story)
    }

    /// Builds a story from compiled Ink JSON.
    #[cfg(feature = "std")]
    pub fn new(json: &str) -> Result<Self, StoryError> {
        Self::new_from_reader(json.as_bytes())
    }

    /// Builds a story from a JSON reader.
    #[cfg(feature = "std")]
    pub fn new_from_reader(reader: impl Read) -> Result<Self, StoryError> {
        let seed = rand::RngExt::random_range(&mut rand::rng(), 0..100);
        let mut story = Self::new_from_reader_with_seed(reader, seed)?;
        story.fixed_seed = None;
        Ok(story)
    }

    /// Reports whether more content can be generated in the current flow.
    pub fn can_continue(&self) -> bool {
        self.runtime.can_continue()
    }

    /// Continues the current flow until the next line or choice boundary.
    pub fn cont(&mut self) -> Result<String, StoryError> {
        self.continue_async(0.0)?;
        self.get_current_text()
    }

    /// Continues for at most the supplied number of milliseconds.
    pub fn continue_async(&mut self, millisecs_limit_async: f32) -> Result<(), StoryError> {
        if !self.is_async_continue_active() && !self.can_continue() {
            return Err(StoryError::InvalidStoryState(
                "Can't continue - should check can_continue before calling Continue".to_owned(),
            ));
        }
        if millisecs_limit_async > 0.0 && self.time_source.is_none() {
            return Err(StoryError::BadArgument(
                "time-limited continuation requires a TimeSource".to_owned(),
            ));
        }
        if !self.externals_validated {
            self.runtime.validate_external_bindings()?;
            self.externals_validated = true;
        }
        self.choice_cache.borrow_mut().take();
        let mut last_time = self.last_time;
        let start = if millisecs_limit_async > 0.0 {
            let now = self.time_source.as_ref().unwrap().now();
            if last_time.is_some_and(|last| now < last) {
                return Err(StoryError::InvalidStoryState(
                    "TimeSource moved backwards during continue_async".to_owned(),
                ));
            }
            last_time = Some(now);
            Some(now)
        } else {
            None
        };
        let clock = self.time_source.clone();
        let result = self.runtime.cont_with_budget(|| {
            let Some(start) = start else {
                return Ok(false);
            };
            let now = clock.as_ref().unwrap().now();
            if last_time.is_some_and(|last| now < last) {
                return Err(StoryError::InvalidStoryState(
                    "TimeSource moved backwards during continue_async".to_owned(),
                ));
            }
            last_time = Some(now);
            Ok(now.saturating_sub(start).as_secs_f32() * 1000.0 >= millisecs_limit_async)
        });
        self.last_time = last_time;
        match result {
            Ok(Some(_)) => {
                self.notify_changed_variables()?;
                if let Some(handler) = &self.on_error {
                    for warning in &self.warnings {
                        handler.borrow_mut().error(warning, ErrorType::Warning);
                    }
                }
                self.warnings.clear();
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(error) => {
                if matches!(
                    error,
                    StoryError::ExternalFunctionFailed { .. }
                        | StoryError::VariableObserverFailed { .. }
                ) || error.get_message().contains("TimeSource moved backwards")
                {
                    self.runtime.force_end();
                    return Err(error);
                }
                let message = error.get_message();
                self.errors.push(message.clone());
                self.runtime.force_end();
                if let Some(handler) = &self.on_error {
                    handler.borrow_mut().error(&message, ErrorType::Error);
                    self.errors.clear();
                    return Ok(());
                }
                Err(error)
            }
        }
    }

    /// Whether a time-limited continuation needs another call.
    pub fn is_async_continue_active(&self) -> bool {
        self.runtime.is_async_continue_active()
    }

    /// Overrides the clock used by time-limited continuation.
    pub fn set_time_source(&mut self, source: impl TimeSource + 'static) {
        self.time_source = Some(Rc::new(source));
        self.last_time = None;
    }

    #[cfg(feature = "std")]
    fn default_time_source() -> Option<Rc<dyn TimeSource>> {
        let origin = web_time::Instant::now();
        Some(Rc::new(move || origin.elapsed()))
    }

    #[cfg(not(feature = "std"))]
    fn default_time_source() -> Option<Rc<dyn TimeSource>> {
        None
    }

    /// Continues until the current flow stops.
    pub fn continue_maximally(&mut self) -> Result<String, StoryError> {
        self.if_async_we_cant("continue_maximally")?;
        let mut text = String::new();
        while self.can_continue() {
            text.push_str(&self.cont()?);
        }
        Ok(text)
    }

    /// Returns the text produced by the most recent continuation.
    pub fn get_current_text(&self) -> Result<String, StoryError> {
        self.if_async_we_cant("call currentText since it's a work in progress")?;
        Ok(self.runtime.current_text())
    }

    /// Returns tags produced by the most recent continuation.
    pub fn get_current_tags(&self) -> Result<Vec<String>, StoryError> {
        self.if_async_we_cant("call currentTags since it's a work in progress")?;
        Ok(self.runtime.current_tags().to_vec())
    }

    /// Returns visible choices at the current boundary.
    pub fn get_current_choice_infos(&self) -> Vec<FlatChoiceInfo> {
        if self.can_continue() {
            return Vec::new();
        }
        self.runtime
            .choices()
            .iter()
            .filter(|choice| !choice.is_invisible_default)
            .map(|choice| FlatChoiceInfo {
                text: choice.text.clone(),
                tags: choice.tags.clone(),
            })
            .collect()
    }

    /// Returns choices in the same shape as the existing `Story` API.
    pub fn get_current_choices(&self) -> Vec<Rc<Choice>> {
        if self.can_continue() {
            return Vec::new();
        }
        self.choice_cache
            .borrow_mut()
            .get_or_insert_with(|| self.runtime.public_choices())
            .clone()
    }

    /// Assigns a handler for runtime diagnostics.
    pub fn set_error_handler(&mut self, handler: Rc<RefCell<dyn ErrorHandler>>) {
        self.on_error = Some(handler);
    }

    /// Reports whether the story has a recorded runtime error.
    pub fn has_error(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Returns runtime errors recorded by the current story state.
    pub fn get_current_errors(&self) -> &[String] {
        &self.errors
    }

    /// Returns runtime warnings recorded by the current story state.
    pub fn get_current_warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Prints the static arena hierarchy for diagnostics.
    pub fn build_string_of_hierarchy(&self) -> String {
        self.runtime.build_string_of_hierarchy()
    }

    /// Chooses a visible choice by its current index.
    pub fn choose_choice_index(&mut self, index: usize) -> Result<(), StoryError> {
        self.if_async_we_cant("choose a choice")?;
        self.choice_cache.borrow_mut().take();
        self.runtime.choose_choice_index(index)
    }

    /// Chooses a path within the story.
    pub fn choose_path_string(
        &mut self,
        path: &str,
        reset_call_stack: bool,
        args: Option<&Vec<ValueType>>,
    ) -> Result<(), StoryError> {
        self.if_async_we_cant("call ChoosePathString right now")?;
        self.choice_cache.borrow_mut().take();
        self.runtime
            .choose_path_string_with_arguments(path, reset_call_stack, args)
    }

    /// Returns the current Ink path, if the flow is still active.
    pub fn get_current_path(&self) -> Option<String> {
        self.runtime.current_path_string()
    }

    /// Returns the visit count for a container path.
    pub fn get_visit_count_at_path_string(&self, path: &str) -> Result<i32, StoryError> {
        self.runtime.visit_count_at_path_string(path)
    }

    /// Creates an empty Ink list with the named origin definition.
    pub fn list_from_origin(&self, origin_name: &str) -> Result<InkList, StoryError> {
        self.runtime.list_from_origin(origin_name)
    }

    /// Creates a one-item Ink list from an `origin.item` name.
    pub fn list_from_item(&self, full_item_name: &str) -> Result<InkList, StoryError> {
        self.runtime.list_from_item(full_item_name)
    }

    /// Gets tags at the beginning of the story.
    pub fn get_global_tags(&self) -> Result<Vec<String>, StoryError> {
        self.runtime.tags_for_content_at_path("")
    }

    /// Gets tags at the beginning of a knot or stitch path.
    pub fn tags_for_content_at_path(&self, path: &str) -> Result<Vec<String>, StoryError> {
        self.runtime.tags_for_content_at_path(path)
    }

    /// Reads a global Ink variable.
    pub fn get_variable(&self, name: &str) -> Option<ValueType> {
        self.runtime.get_variable(name)
    }

    /// Evaluates an Ink function and appends its emitted text to `text_output`.
    pub fn evaluate_function(
        &mut self,
        name: &str,
        args: Option<&Vec<ValueType>>,
        text_output: &mut String,
    ) -> Result<Option<ValueType>, StoryError> {
        self.if_async_we_cant("evaluate a function")?;
        self.choice_cache.borrow_mut().take();
        let value = self.runtime.evaluate_function(name, args, text_output)?;
        self.notify_changed_variables()?;
        Ok(value)
    }

    /// Sets a global Ink variable.
    pub fn set_variable(&mut self, name: &str, value: &ValueType) -> Result<(), StoryError> {
        self.runtime.set_variable(name, value)?;
        self.notify_changed_variables()
    }

    /// Observes one global variable.
    pub fn observe_variable<F>(
        &mut self,
        name: &str,
        observer: F,
    ) -> Result<VariableObserverHandle, StoryError>
    where
        F: FnMut(&str, &ValueType) -> VariableObserverResult + 'static,
    {
        self.observe_variables(&[name], observer)
    }

    /// Observes several global variables with one callback.
    pub fn observe_variables<F>(
        &mut self,
        names: &[&str],
        observer: F,
    ) -> Result<VariableObserverHandle, StoryError>
    where
        F: FnMut(&str, &ValueType) -> VariableObserverResult + 'static,
    {
        self.observe_variables_handler(names, observer)
    }

    /// Observes one global variable with a handler.
    pub fn observe_variable_handler<F>(
        &mut self,
        name: &str,
        observer: F,
    ) -> Result<VariableObserverHandle, StoryError>
    where
        F: VariableObserver + 'static,
    {
        self.observe_variables_handler(&[name], observer)
    }

    /// Observes several global variables with one handler.
    pub fn observe_variables_handler<F>(
        &mut self,
        names: &[&str],
        observer: F,
    ) -> Result<VariableObserverHandle, StoryError>
    where
        F: VariableObserver + 'static,
    {
        self.if_async_we_cant("observe a new variable")?;
        let names: HashSet<_> = names.iter().copied().collect();
        for name in &names {
            if !self.runtime.global_variable_exists(name) {
                return Err(StoryError::BadArgument(format!(
                    "Cannot observe variable '{name}' because it wasn't declared in the ink story."
                )));
            }
        }
        let handle = VariableObserverHandle(self.next_observer_id);
        self.next_observer_id = self.next_observer_id.wrapping_add(1);
        self.observers.insert(handle, Box::new(observer));
        for name in names {
            self.subscriptions
                .entry(name.to_owned())
                .or_default()
                .push(handle);
        }
        Ok(handle)
    }

    /// Removes all subscriptions for an observer handle.
    pub fn remove_variable_observer(
        &mut self,
        handle: VariableObserverHandle,
    ) -> Result<bool, StoryError> {
        self.if_async_we_cant("remove a variable observer")?;
        if self.observers.remove(&handle).is_none() {
            return Ok(false);
        }
        self.subscriptions.retain(|_, handles| {
            handles.retain(|registered| *registered != handle);
            !handles.is_empty()
        });
        Ok(true)
    }

    fn notify_changed_variables(&mut self) -> Result<(), StoryError> {
        let changes = self.runtime.take_changed_variables();
        for (name, value) in changes {
            if let Some(handles) = self.subscriptions.get(&name) {
                for handle in handles {
                    if let Some(observer) = self.observers.get_mut(handle) {
                        observer.changed(&name, &value).map_err(|error| {
                            StoryError::VariableObserverFailed {
                                variable_name: name.clone(),
                                error,
                            }
                        })?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Resets execution state while retaining the static arena and bindings.
    pub fn reset_state(&mut self) -> Result<(), StoryError> {
        self.if_async_we_cant("reset state")?;
        self.choice_cache.borrow_mut().take();
        #[cfg(feature = "std")]
        let seed = self
            .fixed_seed
            .unwrap_or_else(|| rand::RngExt::random_range(&mut rand::rng(), 0..100));
        #[cfg(not(feature = "std"))]
        let seed = self.fixed_seed.unwrap_or(0);
        self.runtime.reset_state(seed)
    }

    /// Selects or creates a named flow.
    pub fn switch_flow(&mut self, name: &str) -> Result<(), StoryError> {
        self.if_async_we_cant("switch flow")?;
        self.choice_cache.borrow_mut().take();
        self.runtime.switch_flow(name);
        Ok(())
    }

    /// Removes a named flow.
    pub fn remove_flow(&mut self, name: &str) -> Result<(), StoryError> {
        self.choice_cache.borrow_mut().take();
        self.runtime.remove_flow(name)
    }

    /// Returns to the default flow.
    pub fn switch_to_default_flow(&mut self) {
        self.choice_cache.borrow_mut().take();
        self.runtime.switch_to_default_flow();
    }

    /// Enables Ink fallback functions for unbound external calls.
    pub fn set_allow_external_function_fallbacks(&mut self, allowed: bool) {
        self.runtime.set_allow_external_fallbacks(allowed);
        self.externals_validated = false;
    }

    /// Binds a Rust handler to an Ink external function.
    pub fn bind_external_function_handler<F>(
        &mut self,
        name: &str,
        function: F,
        lookahead_safe: bool,
    ) -> Result<(), StoryError>
    where
        F: ExternalFunction + 'static,
    {
        self.if_async_we_cant("bind an external function")?;
        self.runtime
            .bind_external_function(name, function, lookahead_safe)?;
        self.externals_validated = false;
        Ok(())
    }

    /// Binds a closure to an Ink external function.
    pub fn bind_external_function<F>(
        &mut self,
        name: &str,
        function: F,
        lookahead_safe: bool,
    ) -> Result<(), StoryError>
    where
        F: FnMut(&str, &[ValueType]) -> ExternalFunctionResult + 'static,
    {
        self.if_async_we_cant("bind an external function")?;
        self.runtime
            .bind_external_function(name, function, lookahead_safe)?;
        self.externals_validated = false;
        Ok(())
    }

    /// Removes a previously bound external function.
    pub fn unbind_external_function(&mut self, name: &str) -> Result<(), StoryError> {
        self.if_async_we_cant("unbind an external function")?;
        self.runtime.unbind_external_function(name)?;
        self.externals_validated = false;
        Ok(())
    }

    fn if_async_we_cant(&self, activity: &str) -> Result<(), StoryError> {
        if self.is_async_continue_active() {
            return Err(StoryError::InvalidStoryState(format!(
                "Can't {activity}. Story is in the middle of a continue_async(). Make more continue_async() calls or a single cont() call beforehand."
            )));
        }
        Ok(())
    }

    /// Writes a save state in the existing Ink JSON format.
    #[cfg(feature = "stream-json-parser")]
    pub fn save_state_to_writer(&self, writer: impl Write) -> Result<(), StoryError> {
        self.runtime.save_state_to_writer(writer)
    }

    /// Writes a save state in the existing Ink JSON format.
    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    pub fn save_state_to_writer(&self, mut writer: impl Write) -> Result<(), StoryError> {
        writer.write_all(self.runtime.save_state_json()?.as_bytes())?;
        Ok(())
    }

    /// Saves state as Ink JSON.
    pub fn save_state(&self) -> Result<String, StoryError> {
        self.runtime.save_state_json()
    }

    /// Loads a state written by this runtime or the existing Ink runtime.
    #[cfg(feature = "stream-json-parser")]
    pub fn load_state_from_reader(&mut self, reader: impl Read) -> Result<(), StoryError> {
        self.choice_cache.borrow_mut().take();
        self.runtime.load_state_from_reader(reader)
    }

    /// Loads a state written by this runtime or the existing Ink runtime.
    #[cfg(all(not(feature = "stream-json-parser"), feature = "serde-json-parser"))]
    pub fn load_state_from_reader(&mut self, mut reader: impl Read) -> Result<(), StoryError> {
        self.choice_cache.borrow_mut().take();
        let mut saved = String::new();
        reader.read_to_string(&mut saved)?;
        self.runtime.load_state_json(&saved)
    }

    /// Loads a state written by this runtime or the existing Ink runtime.
    pub fn load_state(&mut self, saved: &str) -> Result<(), StoryError> {
        self.choice_cache.borrow_mut().take();
        self.runtime.load_state_json(saved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_flat_story_runs_and_restores_a_choice() {
        let json = include_str!("../../conformance-tests/inkfiles/choices/single-choice.ink.json");
        let mut story = FlatStory::new_with_seed(json, 1).unwrap();
        assert_eq!(story.cont().unwrap(), "Hello, world!\n");
        while story.can_continue() {
            story.cont().unwrap();
        }
        let choice = story.get_current_choices();
        assert_eq!(choice[0].text, "Hello back!");
        assert!(Rc::ptr_eq(&choice[0], &story.get_current_choices()[0]));
        let saved = story.save_state().unwrap();
        let mut restored = FlatStory::new_with_seed(json, 1).unwrap();
        restored.load_state_from_reader(saved.as_bytes()).unwrap();
        assert_eq!(
            restored.get_current_choice_infos(),
            story.get_current_choice_infos()
        );
        restored.choose_choice_index(0).unwrap();
        assert!(restored.cont().unwrap().contains("Hello"));
    }

    #[test]
    fn public_flat_story_reads_static_tags_and_list_definitions() {
        let tags_json = include_str!("../../conformance-tests/inkfiles/tags/tags.ink.json");
        let legacy = crate::story::LegacyStory::new_with_seed(tags_json, 1).unwrap();
        let flat = FlatStory::new_with_seed(tags_json, 1).unwrap();
        assert_eq!(
            flat.get_global_tags().unwrap(),
            legacy.get_global_tags().unwrap()
        );

        let lists_json =
            include_str!("../../conformance-tests/inkfiles/lists/basic-operations.ink.json");
        let legacy = crate::story::LegacyStory::new_with_seed(lists_json, 1).unwrap();
        let flat = FlatStory::new_with_seed(lists_json, 1).unwrap();
        let legacy_list = legacy.list_from_origin("list").unwrap();
        let flat_list = flat.list_from_origin("list").unwrap();
        assert_eq!(flat_list.get_origin_names(), legacy_list.get_origin_names());
        assert_eq!(
            flat.list_from_item("list.a").unwrap().items,
            legacy.list_from_item("list.a").unwrap().items
        );
    }

    #[test]
    fn public_flat_story_evaluates_an_ink_function() {
        let json = include_str!("../../conformance-tests/inkfiles/function/func-basic.ink.json");
        let mut legacy = crate::story::LegacyStory::new_with_seed(json, 1).unwrap();
        let mut flat = FlatStory::new_with_seed(json, 1).unwrap();
        let args = vec![ValueType::Int(2), ValueType::Int(8), ValueType::Float(0.4)];
        let mut legacy_output = String::new();
        let mut flat_output = String::new();
        let legacy_value = legacy
            .evaluate_function("lerp", Some(&args), &mut legacy_output)
            .unwrap();
        let flat_value = flat
            .evaluate_function("lerp", Some(&args), &mut flat_output)
            .unwrap();
        assert!(flat_value == legacy_value);
        assert_eq!(flat_output, legacy_output);
        assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
    }

    #[test]
    fn public_flat_story_resets_without_reparsing_static_content() {
        let json = include_str!("../../conformance-tests/inkfiles/choices/single-choice.ink.json");
        let mut flat = FlatStory::new_with_seed(json, 1).unwrap();
        let first = flat.cont().unwrap();
        flat.reset_state().unwrap();
        assert_eq!(flat.cont().unwrap(), first);
    }

    #[test]
    fn public_flat_story_chooses_path_with_arguments() {
        let json = include_str!("../../conformance-tests/inkfiles/knot/param-ints.ink.json");
        let mut legacy = crate::story::LegacyStory::new_with_seed(json, 1).unwrap();
        let mut flat = FlatStory::new_with_seed(json, 1).unwrap();
        let args = vec![ValueType::Int(5)];
        legacy
            .choose_path_string("give", true, Some(&args))
            .unwrap();
        flat.choose_path_string("give", true, Some(&args)).unwrap();
        assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
    }

    #[test]
    fn public_flat_story_observes_global_changes() {
        use crate::compat::{cell::RefCell, rc::Rc};

        let json =
            include_str!("../../conformance-tests/inkfiles/runtime/variable-observers.ink.json");
        let mut story = FlatStory::new_with_seed(json, 1).unwrap();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let captured = observed.clone();
        let handle = story
            .observe_variable("x", move |name, value| {
                captured
                    .borrow_mut()
                    .push((name.to_owned(), value.get::<i32>().unwrap()));
                Ok(())
            })
            .unwrap();
        while story.can_continue() {
            story.cont().unwrap();
        }
        story.choose_choice_index(0).unwrap();
        while story.can_continue() {
            story.cont().unwrap();
        }
        assert_eq!(
            observed.borrow().as_slice(),
            &[("x".to_owned(), 5), ("x".to_owned(), 10)]
        );
        assert!(story.remove_variable_observer(handle).unwrap());
        story.set_variable("x", &ValueType::Int(11)).unwrap();
        assert_eq!(observed.borrow().len(), 2);
    }

    #[cfg(feature = "std")]
    #[test]
    fn flat_story_matches_legacy_across_compiled_fixtures() {
        use std::{fs, path::Path};

        fn files(path: &Path, output: &mut Vec<std::path::PathBuf>) {
            for entry in fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    files(&path, output);
                } else if path.to_string_lossy().ends_with(".ink.json") {
                    output.push(path);
                }
            }
        }

        let mut paths = Vec::new();
        files(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../conformance-tests/inkfiles"),
            &mut paths,
        );
        paths.sort();
        let mut differences = Vec::new();
        let mut compared = 0;
        for path in paths {
            let json = fs::read_to_string(&path).unwrap();
            let (Ok(mut legacy), Ok(mut flat)) = (
                crate::story::LegacyStory::new_with_seed(&json, 1),
                FlatStory::new_with_seed(&json, 1),
            ) else {
                continue;
            };
            compared += 1;
            let mut mismatch = None;
            for step in 0..500 {
                if legacy.can_continue() != flat.can_continue() {
                    mismatch = Some(format!("step {step}: can_continue differs"));
                    break;
                }
                if legacy.can_continue() {
                    match (legacy.cont(), flat.cont()) {
                        (Ok(expected), Ok(actual)) if expected == actual => {}
                        (Err(_), Err(_)) => break,
                        (expected, actual) => {
                            mismatch =
                                Some(format!("step {step}: text {expected:?} != {actual:?}"));
                            break;
                        }
                    }
                } else {
                    let expected = legacy.get_current_choices();
                    let actual = flat.get_current_choices();
                    let expected: Vec<_> = expected
                        .iter()
                        .map(|choice| (&choice.text, &choice.tags))
                        .collect();
                    let actual: Vec<_> = actual
                        .iter()
                        .map(|choice| (&choice.text, &choice.tags))
                        .collect();
                    if expected != actual {
                        mismatch = Some(format!("step {step}: choices {expected:?} != {actual:?}"));
                        break;
                    }
                    if actual.is_empty() {
                        break;
                    }
                    legacy.choose_choice_index(0).unwrap();
                    flat.choose_choice_index(0).unwrap();
                }
            }
            if let Some(mismatch) = mismatch {
                differences.push(format!("{}: {mismatch}", path.display()));
            }
        }
        assert!(compared >= 100, "only compared {compared} fixtures");
        assert!(differences.is_empty(), "{}", differences.join("\n"));
    }

    #[test]
    fn flat_story_matches_legacy_on_intercept_playthrough() {
        let json = include_str!("../../conformance-tests/inkfiles/TheIntercept.ink.json");
        let mut legacy = crate::story::LegacyStory::new_with_seed(json, 7).unwrap();
        let mut flat = FlatStory::new_with_seed(json, 7).unwrap();
        let mut choices = 0_usize;
        for step in 0..10_000_usize {
            assert_eq!(flat.can_continue(), legacy.can_continue(), "step {step}");
            if legacy.can_continue() {
                assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap(), "step {step}");
            } else {
                let old_choices = legacy.get_current_choices();
                let new_choices = flat.get_current_choices();
                let old_text: Vec<_> = old_choices
                    .iter()
                    .map(|choice| (&choice.text, &choice.tags))
                    .collect();
                let new_text: Vec<_> = new_choices
                    .iter()
                    .map(|choice| (&choice.text, &choice.tags))
                    .collect();
                assert_eq!(new_text, old_text, "step {step}");
                if new_choices.is_empty() {
                    assert!(choices > 1);
                    return;
                }
                let index = (choices * 7 + 1) % new_choices.len();
                legacy.choose_choice_index(index).unwrap();
                flat.choose_choice_index(index).unwrap();
                choices += 1;
            }
        }
        panic!("The Intercept did not finish after 10,000 steps");
    }

    #[test]
    fn flat_story_async_continuation_pauses_and_resumes() {
        use crate::compat::cell::Cell;

        let json = r#"{"inkVersion":21,"root":["^one","^two","^three","done",null],"listDefs":{}}"#;
        let mut story = FlatStory::new_with_seed(json, 1).unwrap();
        let tick = Rc::new(Cell::new(0_u64));
        let clock = tick.clone();
        story.set_time_source(move || {
            let next = clock.get() + 2;
            clock.set(next);
            core::time::Duration::from_millis(next)
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
        assert_eq!(story.get_current_text().unwrap(), "onetwothree");
    }

    #[test]
    fn flat_story_rejects_backwards_async_clock() {
        use crate::compat::cell::Cell;

        let json = r#"{"inkVersion":21,"root":["^text","done",null],"listDefs":{}}"#;
        let mut story = FlatStory::new_with_seed(json, 1).unwrap();
        let first = Cell::new(true);
        story.set_time_source(move || {
            if first.replace(false) {
                core::time::Duration::from_millis(2)
            } else {
                core::time::Duration::from_millis(1)
            }
        });
        assert!(matches!(
            story.continue_async(1.0),
            Err(StoryError::InvalidStoryState(_))
        ));
    }

    #[test]
    fn flat_story_reports_runtime_error_to_handler() {
        struct Capture(Rc<RefCell<Vec<String>>>);
        impl ErrorHandler for Capture {
            fn error(&mut self, message: &str, error_type: ErrorType) {
                assert!(error_type == ErrorType::Error);
                self.0.borrow_mut().push(message.to_owned());
            }
        }

        let json = r#"{"inkVersion":21,"root":["^unfinished",null],"listDefs":{}}"#;
        let mut story = FlatStory::new_with_seed(json, 1).unwrap();
        let messages = Rc::new(RefCell::new(Vec::new()));
        story.set_error_handler(Rc::new(RefCell::new(Capture(messages.clone()))));
        assert_eq!(story.cont().unwrap(), "unfinished");
        assert!(!story.can_continue());
        assert!(!story.has_error());
        assert_eq!(messages.borrow().len(), 1);
    }

    #[test]
    fn flat_story_preserves_version_warning() {
        let json = r#"{"inkVersion":20,"root":["^text","done",null],"listDefs":{}}"#;
        let mut legacy = crate::story::LegacyStory::new_with_seed(json, 1).unwrap();
        let mut flat = FlatStory::new_with_seed(json, 1).unwrap();
        assert_eq!(flat.get_current_warnings(), legacy.get_current_warnings());
        assert_eq!(flat.cont().unwrap(), legacy.cont().unwrap());
        assert!(flat.get_current_warnings().is_empty());
    }

    #[test]
    fn flat_story_validates_external_bindings_before_continuing() {
        let json = include_str!(
            "../../conformance-tests/inkfiles/runtime/external-function-0-arg.ink.json"
        );
        let mut legacy = crate::story::LegacyStory::new_with_seed(json, 1).unwrap();
        let mut flat = FlatStory::new_with_seed(json, 1).unwrap();
        assert_eq!(
            flat.cont().unwrap_err().to_string(),
            legacy.cont().unwrap_err().to_string()
        );
    }
}
