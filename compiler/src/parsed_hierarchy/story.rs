use super::{ContentList, FlowArgument, FlowBase, FlowLevel, ParsedObject};

#[derive(Debug, Clone)]
pub struct Story {
    flow: FlowBase,
    source: String,
    source_filename: Option<String>,
    pub count_all_visits: bool,
    root_content: ContentList,
}

impl Story {
    pub fn new(source: &str, source_filename: Option<String>, count_all_visits: bool) -> Self {
        let flow = FlowBase::new(FlowLevel::Story, None, Vec::<FlowArgument>::new(), false);
        let mut root_content = ContentList::new();
        root_content.object_mut().set_parent_id(flow.object().id());
        Self {
            flow,
            source: source.to_owned(),
            source_filename,
            count_all_visits,
            root_content,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        self.flow.object()
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        self.flow.object_mut()
    }

    pub fn flow(&self) -> &FlowBase {
        &self.flow
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn source_filename(&self) -> Option<&str> {
        self.source_filename.as_deref()
    }

    pub fn root_content(&self) -> &ContentList {
        &self.root_content
    }

    pub fn root_content_mut(&mut self) -> &mut ContentList {
        &mut self.root_content
    }
}

#[cfg(test)]
mod tests {
    use super::Story;
    use crate::parsed_hierarchy::{Content, ContentList, DebugMetadata};

    #[test]
    fn story_sets_root_content_parent() {
        let story = Story::new("hello", Some("main.ink".to_owned()), true);
        assert_eq!(
            Some(story.object().id()),
            story.root_content().object().parent_id()
        );
    }

    #[test]
    fn content_list_trims_trailing_inline_whitespace() {
        let mut list = ContentList::new();
        list.push_text("hello \t");
        list.trim_trailing_whitespace();
        let text = match &list.content()[0] {
            Content::Text(text) => text.text(),
        };
        assert_eq!("hello", text);
    }

    #[test]
    fn object_can_hold_debug_metadata() {
        let mut story = Story::new("hello", None, true);
        story.object_mut().set_debug_metadata(DebugMetadata {
            start_line_number: 1,
            end_line_number: 1,
            start_character_number: 1,
            end_character_number: 5,
            file_name: Some("main.ink".to_owned()),
        });
        assert!(story.object().has_own_debug_metadata());
    }
}
