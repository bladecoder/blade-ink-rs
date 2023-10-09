use core::fmt;

#[derive(Debug)]
pub enum StoryError {
    InvalidStoryState(String),
    BadJson(String),
    BadArgument(String),
}
impl StoryError {
    pub(crate) fn get_message(&self) -> &str {
        match self {
            StoryError::InvalidStoryState(msg)
            | StoryError::BadJson(msg)
            | StoryError::BadArgument(msg) => msg.as_str(),
        }
    }
}

impl std::error::Error for StoryError {}

impl fmt::Display for StoryError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            StoryError::InvalidStoryState(desc) => write!(f, "Invalid story state: {}", desc),
            StoryError::BadJson(desc) => write!(f, "Error parsing JSON: {}", desc),
            StoryError::BadArgument(arg) => write!(f, "Bad argument: {}", arg),
        }
    }
}
