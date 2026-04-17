use crate::error::CompilerError;

use super::comment_eliminator::CommentEliminator;

#[derive(Default)]
pub struct InkParser;

impl InkParser {
    pub fn parse(source: &str) -> Result<(), CompilerError> {
        let _preprocessed = CommentEliminator::process(source);
        Err(CompilerError::UnsupportedFeature(
            "ported InkParser is not implemented yet".to_owned(),
        ))
    }
}
