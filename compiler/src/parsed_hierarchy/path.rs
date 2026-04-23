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

    pub(crate) fn runtime_target_cache(
        &self,
        story: &crate::parsed_hierarchy::Story,
    ) -> Option<std::rc::Rc<crate::parsed_hierarchy::ParsedRuntimeCache>> {
        self.resolve_from_story(story)
            .and_then(|target_ref| story.runtime_target_cache_for_ref(target_ref))
            .or_else(|| story.runtime_target_cache_for_path(self.as_str()))
    }

    pub(crate) fn runtime_path(
        &self,
        story: &crate::parsed_hierarchy::Story,
    ) -> Option<String> {
        self.runtime_target_cache(story)
            .and_then(|cache| cache.runtime_path())
            .map(|path| path.to_string())
    }

    pub(crate) fn count_runtime_path(
        &self,
        story: &crate::parsed_hierarchy::Story,
    ) -> Option<String> {
        self.runtime_target_cache(story)
            .and_then(|cache| cache.container_for_counting())
            .map(|container| bladeink::path_of(container.as_ref()).to_string())
    }

    pub(crate) fn named_count_runtime_path(
        &self,
        named_paths: &std::collections::HashMap<String, String>,
    ) -> Option<String> {
        let name = self.as_str();
        named_paths.get(name).cloned().or_else(|| {
            let mut parts: Vec<&str> = name.split('.').collect();
            if parts.len() < 2 {
                return None;
            }
            let last = parts.pop()?;
            Some(format!("{}.0.{last}", parts.join(".")))
        })
    }

    pub(crate) fn output_count_runtime_path(
        &self,
        scope: crate::runtime_export::Scope<'_>,
        _story: &crate::parsed_hierarchy::Story,
        named_paths: Option<&std::collections::HashMap<String, String>>,
    ) -> Option<String> {
        if let Some(named_paths) = named_paths
            && let Some(path) = self.named_count_runtime_path(named_paths)
        {
            return Some(path);
        }

        let crate::runtime_export::Scope::Flow(flow) = scope else {
            return None;
        };

        (flow.flow().identifier() == Some(self.as_str())).then_some(".^".to_owned())
    }

    pub(crate) fn condition_count_runtime_path(
        &self,
        scope: crate::runtime_export::Scope<'_>,
        story: &crate::parsed_hierarchy::Story,
        named_paths: &std::collections::HashMap<String, String>,
    ) -> Option<String> {
        if let Some(path) = self.named_count_runtime_path(named_paths) {
            return Some(path);
        }

        if story
            .parsed_flows()
            .iter()
            .any(|flow| flow.flow().identifier() == Some(self.as_str()))
        {
            return Some(self.as_str().to_owned());
        }

        let crate::runtime_export::Scope::Flow(flow) = scope else {
            return None;
        };

        flow.children()
            .iter()
            .any(|child| child.flow().identifier() == Some(self.as_str()))
            .then_some(format!("{}.{}", flow.flow().identifier().unwrap_or_default(), self.as_str()))
    }

    pub(crate) fn resolve_variable_divert_name(
        &self,
        scope: crate::runtime_export::Scope<'_>,
        story: &crate::parsed_hierarchy::Story,
        named_paths: Option<&std::collections::HashMap<String, String>>,
    ) -> Option<String> {
        let target = self.as_str();
        if target.contains('.') || named_paths.is_some_and(|paths| paths.contains_key(target)) {
            return None;
        }

        if story
            .global_initializers()
            .iter()
            .any(|(name, _)| name == target)
            && !story
                .parsed_flows()
                .iter()
                .any(|flow| flow.flow().identifier() == Some(target))
        {
            return Some(target.to_owned());
        }

        let crate::runtime_export::Scope::Flow(flow) = scope else {
            return None;
        };

        flow.flow()
            .arguments()
            .iter()
            .any(|arg| arg.identifier == target && arg.is_divert_target)
            .then_some(target.to_owned())
    }

    pub(crate) fn resolve_runtime_path_in_scope(
        &self,
        scope: crate::runtime_export::Scope<'_>,
        story: &crate::parsed_hierarchy::Story,
        named_paths: Option<&std::collections::HashMap<String, String>>,
    ) -> String {
        let target = self.as_str();
        if let Some(named_paths) = named_paths
            && let Some(path) = named_paths.get(target)
        {
            return path.clone();
        }

        if target.contains('.') {
            if let Some(path) = story.resolve_explicit_weave_target_path(target) {
                return path;
            }
            if let Some(expanded) = story.expand_weave_point_runtime_path(target, named_paths) {
                return expanded;
            }
            return target.to_owned();
        }

        let crate::runtime_export::Scope::Flow(flow) = scope else {
            return story
                .resolve_unique_nested_flow_target_path(target)
                .unwrap_or_else(|| target.to_owned());
        };

        if flow
            .children()
            .iter()
            .any(|child| child.flow().identifier() == Some(target))
        {
            return format!("{}.{}", flow.flow().identifier().unwrap_or_default(), target);
        }

        if let Some(resolved) = story.resolve_sibling_or_ancestor_flow_target_path(target, flow) {
            return resolved;
        }

        if story
            .parsed_flows()
            .iter()
            .any(|candidate| candidate.flow().identifier() == Some(target))
        {
            return target.to_owned();
        }

        target.to_owned()
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
