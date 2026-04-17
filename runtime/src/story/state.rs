use crate::{
    path::Path, story::Story, story_error::StoryError, story_state::StoryState,
    value_type::ValueType,
};

/// # State
/// Methods to read and write story state.
impl Story {
    #[inline]
    pub(crate) fn get_state(&self) -> &StoryState {
        &self.state
    }

    #[inline]
    pub(crate) fn get_state_mut(&mut self) -> &mut StoryState {
        &mut self.state
    }

    pub(crate) fn reset_globals(&mut self) -> Result<(), StoryError> {
        if self
            .main_content_container
            .named_content
            .contains_key("global decl")
        {
            let original_pointer = self.get_state().get_current_pointer().clone();

            self.choose_path(
                &Path::new_with_components_string(Some("global decl")),
                false,
            )?;

            // Continue, but without validating external bindings,
            // since we may be doing this reset at initialisation time.
            self.continue_internal(0.0)?;

            self.get_state().set_current_pointer(original_pointer);
        }

        self.get_state_mut()
            .variables_state
            .snapshot_default_globals();

        Ok(())
    }

    /// Set the value of a named global ink variable.
    /// The types available are the standard ink types.
    pub fn set_variable(
        &mut self,
        variable_name: &str,
        value_type: &ValueType,
    ) -> Result<(), StoryError> {
        let notify_observers = self
            .get_state_mut()
            .variables_state
            .set(variable_name, value_type.clone())?;

        if notify_observers {
            self.notify_variable_changed(variable_name, value_type);
        }

        Ok(())
    }

    /// Get the value of a named global ink variable.
    /// The types available are the standard ink types.
    pub fn get_variable(&self, variable_name: &str) -> Option<ValueType> {
        self.get_state().variables_state.get(variable_name)
    }

    pub(crate) fn restore_state_snapshot(&mut self) {
        // Patched state had temporarily hijacked our
        // VariablesState and set its own callstack on it,
        // so we need to restore that.
        // If we're in the middle of saving, we may also
        // need to give the VariablesState the old patch.
        self.state_snapshot_at_last_new_line
            .as_mut()
            .unwrap()
            .restore_after_patch(); // unwrap: state_snapshot_at_last_new_line checked Some in previous fn

        self.state = self.state_snapshot_at_last_new_line.take().unwrap();

        // If save completed while the above snapshot was
        // active, we need to apply any changes made since
        // the save was started but before the snapshot was made.
        if !self.async_saving {
            self.get_state_mut().apply_any_patch();
        }
    }

    pub(crate) fn state_snapshot(&mut self) {
        // tmp_state contains the new state and current state is stored in snapshot
        let mut tmp_state = self.state.copy_and_start_patching(false);
        std::mem::swap(&mut tmp_state, &mut self.state);
        self.state_snapshot_at_last_new_line = Some(tmp_state);
    }

    pub(crate) fn discard_snapshot(&mut self) {
        // Normally we want to integrate the patch
        // into the main global/counts dictionaries.
        // However, if we're in the middle of async
        // saving, we simply stay in a "patching" state,
        // albeit with the newer cloned patch.

        if !self.async_saving {
            self.get_state_mut().apply_any_patch();
        }

        // No longer need the snapshot.
        self.state_snapshot_at_last_new_line = None;
    }

    /// Exports the current state to JSON format, in order to save the game.
    pub fn save_state(&self) -> Result<String, StoryError> {
        self.get_state().to_json()
    }

    /// Loads a previously saved state in JSON format.
    pub fn load_state(&mut self, json_state: &str) -> Result<(), StoryError> {
        self.get_state_mut().load_json(json_state)
    }

    /// Reset the Story back to its initial state as it was when it was first constructed.
    pub fn reset_state(&mut self) -> Result<(), StoryError> {
        self.if_async_we_cant("ResetState")?;

        self.state = StoryState::new(
            self.main_content_container.clone(),
            self.list_definitions.clone(),
        );

        self.reset_globals()?;

        Ok(())
    }
}
