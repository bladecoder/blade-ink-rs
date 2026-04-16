use crate::{error::CompilerError, parsed_hierarchy::ParsedStory};

pub struct Parser<'a> {
    source: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source }
    }

    pub fn parse(&self) -> Result<ParsedStory, CompilerError> {
        let _ = self.source;
        Ok(ParsedStory)
    }
}
