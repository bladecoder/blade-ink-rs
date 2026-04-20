#[derive(Debug, Clone)]
pub struct Story {
    source: String,
    source_filename: Option<String>,
    pub count_all_visits: bool,
}

impl Story {
    pub fn new(source: &str, source_filename: Option<String>, count_all_visits: bool) -> Self {
        Self {
            source: source.to_owned(),
            source_filename,
            count_all_visits,
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn source_filename(&self) -> Option<&str> {
        self.source_filename.as_deref()
    }
}
