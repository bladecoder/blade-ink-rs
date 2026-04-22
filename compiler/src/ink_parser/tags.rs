use super::InkParser;
use crate::parsed_hierarchy::Tag;

impl<'fh> InkParser<'fh> {
    pub fn start_tag(&mut self) -> Option<Tag> {
        let _ = self.whitespace();
        self.parser.parse_string("#")?;
        let was_active = self.tag_active;
        self.tag_active = true;
        let _ = self.whitespace();
        Some(Tag::new(true, self.parsing_choice || was_active))
    }

    pub fn end_tag_if_necessary(&mut self) -> Option<Tag> {
        if !self.tag_active {
            return None;
        }

        self.tag_active = false;
        Some(Tag::new(false, self.parsing_choice))
    }
}
