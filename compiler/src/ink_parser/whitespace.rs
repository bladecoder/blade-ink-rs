use super::InkParser;
use crate::{parsed_hierarchy::DebugMetadata, string_parser::{ParseSuccess, StringParser, StringParserStateElement}};

impl<'fh> InkParser<'fh> {
    pub fn end_of_line(&mut self) -> Option<ParseSuccess> {
        self.newline().or_else(|| self.end_of_file())
    }

    pub fn newline(&mut self) -> Option<ParseSuccess> {
        let _ = self.whitespace();
        self.parser.parse_newline().map(|_| ParseSuccess)
    }

    pub fn end_of_file(&mut self) -> Option<ParseSuccess> {
        let _ = self.whitespace();
        self.parser.end_of_input().then_some(ParseSuccess)
    }

    pub fn multiline_whitespace(&mut self) -> Option<ParseSuccess> {
        let mut count = 0;
        while self.newline().is_some() {
            count += 1;
        }
        (count > 0).then_some(ParseSuccess)
    }

    pub fn whitespace(&mut self) -> Option<ParseSuccess> {
        self.parser
            .parse_characters_from_string(" \t", true, -1)
            .map(|_| ParseSuccess)
    }

    pub fn any_whitespace(&mut self) -> Option<ParseSuccess> {
        let mut found = false;
        loop {
            if self.whitespace().is_some() || self.multiline_whitespace().is_some() {
                found = true;
            } else {
                break;
            }
        }
        found.then_some(ParseSuccess)
    }

    pub fn spaced<T>(&mut self, mut rule: impl FnMut(&mut StringParser) -> Option<T>) -> Option<T> {
        let _ = self.whitespace();
        let result = self.parser.parse_object(|parser| rule(parser))?;
        let _ = self.whitespace();
        Some(result)
    }

    pub fn multispaced<T>(
        &mut self,
        mut rule: impl FnMut(&mut StringParser) -> Option<T>,
    ) -> Option<T> {
        let _ = self.any_whitespace();
        let result = self.parser.parse_object(|parser| rule(parser))?;
        let _ = self.any_whitespace();
        Some(result)
    }

    pub fn create_debug_metadata(
        &self,
        start: &StringParserStateElement,
        end: &StringParserStateElement,
    ) -> DebugMetadata {
        DebugMetadata {
            start_line_number: start.line_index() + 1,
            end_line_number: end.line_index() + 1,
            start_character_number: start.character_in_line_index() + 1,
            end_character_number: end.character_in_line_index() + 1,
            file_name: self.source_name.clone(),
        }
    }
}
