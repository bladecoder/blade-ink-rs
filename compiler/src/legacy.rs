use crate::{error::CompilerError, parser::Parser};

pub fn compile(source: &str, count_all_visits: bool) -> Result<String, CompilerError> {
    let _ = count_all_visits;

    let parsed_story = Parser::new(source).parse()?;
    parsed_story.to_json_string().map_err(|error| {
        CompilerError::InvalidSource(format!("failed to serialize compiled ink: {error}"))
    })
}
