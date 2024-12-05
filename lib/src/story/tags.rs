use crate::{
    container::Container,
    control_command::{CommandType, ControlCommand},
    path::Path,
    story::Story,
    story_error::StoryError,
    value::Value,
    value_type::StringValue,
};

/// # Tags
/// Methods to read tags.
impl Story {
    /// Get any global tags associated with the story. These are defined as
    /// hash tags defined at the very top of the story.
    pub fn get_global_tags(&self) -> Result<Vec<String>, StoryError> {
        self.tags_at_start_of_flow_container_with_path_string("")
    }

    /// Gets any tags associated with a particular knot or knot.stitch.
    /// These are defined as hash tags defined at the very top of a
    /// knot or stitch.
    pub fn tags_for_content_at_path(&self, path: &str) -> Result<Vec<String>, StoryError> {
        self.tags_at_start_of_flow_container_with_path_string(path)
    }

    pub(crate) fn tags_at_start_of_flow_container_with_path_string(
        &self,
        path_string: &str,
    ) -> Result<Vec<String>, StoryError> {
        let path = Path::new_with_components_string(Some(path_string));

        // Expected to be global story, knot, or stitch
        let mut flow_container = self.content_at_path(&path).container().unwrap();

        while let Some(first_content) = flow_container.content.get(0) {
            if let Ok(container) = first_content.clone().into_any().downcast::<Container>() {
                flow_container = container;
            } else {
                break;
            }
        }

        // Any initial tag objects count as the "main tags" associated with that
        // story/knot/stitch
        let mut in_tag = false;
        let mut tags = Vec::new();

        for content in &flow_container.content {
            match content.as_ref().as_any().downcast_ref::<ControlCommand>() {
                Some(command) => match command.command_type {
                    CommandType::BeginTag => in_tag = true,
                    CommandType::EndTag => in_tag = false,
                    _ => {}
                },
                _ => {
                    if in_tag {
                        if let Some(string_value) =
                            Value::get_value::<&StringValue>(content.as_ref())
                        {
                            tags.push(string_value.string.clone());
                        } else {
                            return Err(
                                StoryError::InvalidStoryState("Tag contained non-text content. Only plain text is allowed when using globalTags or TagsAtContentPath. If you want to evaluate dynamic content, you need to use story.Continue()".to_owned()),
                            );
                        }
                    } else {
                        break;
                    }
                }
            }
        }

        Ok(tags)
    }

    /// Gets a list of tags defined with '#' in the ink source that were
    /// seen during the most recent [`cont`](Story::cont) call.
    pub fn get_current_tags(&mut self) -> Result<Vec<String>, StoryError> {
        self.if_async_we_cant("call currentTags since it's a work in progress")?;
        Ok(self.get_state_mut().get_current_tags())
    }
}
