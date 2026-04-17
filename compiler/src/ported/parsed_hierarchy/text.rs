#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub text: String,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}
