use crate::{story::Story, story_error::StoryError};

/// # Flow
/// Methods to work with flows and the call-stack.
impl Story {
    pub(crate) fn reset_callstack(&mut self) -> Result<(), StoryError> {
        self.if_async_we_cant("ResetCallstack")?;

        self.get_state_mut().force_end();

        Ok(())
    }

    /// Changes from the current flow to the specified one.
    pub fn switch_flow(&mut self, flow_name: &str) -> Result<(), StoryError> {
        self.if_async_we_cant("switch flow")?;

        if self.async_saving {
            return Err(StoryError::InvalidStoryState(format!(
                "Story is already in background saving mode, can't switch flow to {}",
                flow_name
            )));
        }

        self.get_state_mut().switch_flow_internal(flow_name);

        Ok(())
    }

    /// Removes the specified flow from the story.
    pub fn remove_flow(&mut self, flow_name: &str) -> Result<(), StoryError> {
        self.get_state_mut().remove_flow_internal(flow_name)
    }

    /// Switches to the default flow, keeping the current flow around for
    /// later.
    pub fn switch_to_default_flow(&mut self) {
        self.get_state_mut().switch_to_default_flow_internal();
    }
}
