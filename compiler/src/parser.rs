use crate::{
    error::CompilerError,
    parsed_hierarchy::{ParsedStory, TextFragment},
};

pub struct Parser<'a> {
    source: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source }
    }

    pub fn parse(&self) -> Result<ParsedStory, CompilerError> {
        if self.source.is_empty() {
            return Err(CompilerError::InvalidSource(
                "ink source is empty; expected at least one line of text",
            ));
        }

        let mut fragments = Vec::new();

        for line in self.source.split_inclusive('\n') {
            let content = line.strip_suffix('\n').unwrap_or(line);

            if !content.is_empty() {
                fragments.push(TextFragment::Text(content.to_owned()));
            }

            if line.ends_with('\n') {
                fragments.push(TextFragment::Newline);
            }
        }

        Ok(ParsedStory::new(fragments))
    }
}
