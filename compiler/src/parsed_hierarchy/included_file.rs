use super::{ObjectKind, ParsedObject, Story};

#[derive(Debug, Clone)]
pub struct IncludedFile {
    object: ParsedObject,
    included_story: Option<Box<Story>>,
    filename: Option<String>,
}

impl IncludedFile {
    pub fn new(included_story: Option<Story>, filename: Option<String>) -> Self {
        let mut object = ParsedObject::new(ObjectKind::IncludedFile);
        let mut included_story = included_story.map(Box::new);
        if let Some(included_story) = included_story.as_mut() {
            included_story.object_mut().set_parent(&object);
            object.add_content_ref(included_story.object().reference());
        }
        Self {
            object,
            included_story,
            filename,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn included_story(&self) -> Option<&Story> {
        self.included_story.as_deref()
    }

    pub fn filename(&self) -> Option<&str> {
        self.filename.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::IncludedFile;
    use crate::parsed_hierarchy::Story;

    #[test]
    fn included_file_tracks_story_and_filename() {
        let story = Story::new("content", Some("included.ink".to_owned()), true);
        let included = IncludedFile::new(Some(story), Some("included.ink".to_owned()));
        assert_eq!(Some("included.ink"), included.filename());
        assert!(included.included_story().is_some());
    }
}
