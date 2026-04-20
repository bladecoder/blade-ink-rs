use super::{ObjectKind, ParsedObject, Text};

#[derive(Debug, Clone)]
pub enum Content {
    Text(Text),
}

#[derive(Debug, Clone)]
pub struct ContentList {
    object: ParsedObject,
    content: Vec<Content>,
    dont_flatten: bool,
}

impl Default for ContentList {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentList {
    pub fn new() -> Self {
        Self {
            object: ParsedObject::new(ObjectKind::ContentList),
            content: Vec::new(),
            dont_flatten: false,
        }
    }

    pub fn object(&self) -> &ParsedObject {
        &self.object
    }

    pub fn object_mut(&mut self) -> &mut ParsedObject {
        &mut self.object
    }

    pub fn content(&self) -> &[Content] {
        &self.content
    }

    pub fn dont_flatten(&self) -> bool {
        self.dont_flatten
    }

    pub fn set_dont_flatten(&mut self, value: bool) {
        self.dont_flatten = value;
    }

    pub fn push_text(&mut self, text: impl Into<String>) {
        let mut text = Text::new(text);
        text.object_mut().set_parent_id(self.object.id());
        self.content.push(Content::Text(text));
    }

    pub fn trim_trailing_whitespace(&mut self) {
        while let Some(Content::Text(text)) = self.content.last_mut() {
            text.trim_end_inline_whitespace();
            if text.is_empty() {
                self.content.pop();
                continue;
            }
            break;
        }
    }
}
