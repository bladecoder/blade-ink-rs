use crate::story_error::StoryError;

#[derive(PartialEq, Clone, Copy, Eq, Hash, Debug)]
pub enum PushPopType {
    Tunnel,
    Function,
    FunctionEvaluationFromGame
}

impl PushPopType {
    pub(crate) fn from_value(value: usize) -> Result<PushPopType, StoryError> {
        match value {
            0 => Ok(PushPopType::Tunnel),
            1 => Ok(PushPopType::Function),
            2 => Ok(PushPopType::FunctionEvaluationFromGame),
            _ => Err(StoryError::BadJson("Unexpected PushPopType value".to_owned()))
        }
    }
}