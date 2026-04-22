#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParsedPath {
    components: Vec<String>,
    dotted: String,
}

impl std::fmt::Display for ParsedPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ParsedPath {
    pub fn new(components: Vec<String>) -> Self {
        let dotted = components.join(".");
        Self { components, dotted }
    }

    pub fn from_dotted(path: impl Into<String>) -> Self {
        let dotted = path.into();
        let components = if dotted.is_empty() {
            Vec::new()
        } else {
            dotted.split('.').map(ToOwned::to_owned).collect()
        };
        Self { components, dotted }
    }

    pub fn components(&self) -> &[String] {
        &self.components
    }

    pub fn first_component(&self) -> Option<&str> {
        self.components.first().map(String::as_str)
    }

    pub fn number_of_components(&self) -> usize {
        self.components.len()
    }

    pub fn as_str(&self) -> &str {
        &self.dotted
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    pub fn resolve_from_story(
        &self,
        story: &crate::parsed_hierarchy::Story,
    ) -> Option<crate::parsed_hierarchy::ParsedObjectRef> {
        story.resolve_target_ref(self.as_str())
    }
}

impl From<Vec<String>> for ParsedPath {
    fn from(value: Vec<String>) -> Self {
        Self::new(value)
    }
}

impl From<String> for ParsedPath {
    fn from(value: String) -> Self {
        Self::from_dotted(value)
    }
}

impl From<&str> for ParsedPath {
    fn from(value: &str) -> Self {
        Self::from_dotted(value)
    }
}
