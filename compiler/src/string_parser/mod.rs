#[derive(Debug, Default, Clone)]
pub struct StringParserState {
    pub line_index: usize,
    pub character_index: usize,
    pub character_in_line_index: usize,
}

#[derive(Debug, Clone)]
pub struct StringParser {
    input: String,
    state: StringParserState,
}

impl StringParser {
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            state: StringParserState::default(),
        }
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn state(&self) -> &StringParserState {
        &self.state
    }
}
