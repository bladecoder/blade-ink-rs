#[derive(Debug, Clone, Default)]
pub struct InkParser {
    source_name: Option<String>,
}

impl InkParser {
    pub fn new(source_name: Option<String>) -> Self {
        Self { source_name }
    }

    pub fn source_name(&self) -> Option<&str> {
        self.source_name.as_deref()
    }
}
