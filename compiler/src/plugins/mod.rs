use std::rc::Rc;

use bladeink::story::Story as RuntimeStory;

use crate::parsed_hierarchy::Story as ParsedStory;

pub trait Plugin {
    fn pre_parse(&self, story_content: &mut String);

    fn post_parse(&self, parsed_story: &mut ParsedStory);

    fn post_export(&self, parsed_story: &ParsedStory, runtime_story: &mut RuntimeStory);
}

#[derive(Default, Clone)]
pub struct PluginManager {
    plugin_directories: Vec<String>,
    plugins: Vec<Rc<dyn Plugin>>,
}

impl PluginManager {
    pub fn new(plugin_directories: Vec<String>) -> Self {
        Self {
            plugin_directories,
            plugins: Vec::new(),
        }
    }

    pub fn with_plugins(plugin_directories: Vec<String>, plugins: Vec<Rc<dyn Plugin>>) -> Self {
        Self {
            plugin_directories,
            plugins,
        }
    }

    pub fn plugin_directories(&self) -> &[String] {
        &self.plugin_directories
    }

    pub fn plugins(&self) -> &[Rc<dyn Plugin>] {
        &self.plugins
    }

    pub fn pre_parse(&self, mut story_content: String) -> String {
        for plugin in &self.plugins {
            plugin.pre_parse(&mut story_content);
        }
        story_content
    }

    pub fn post_parse(&self, mut parsed_story: ParsedStory) -> ParsedStory {
        for plugin in &self.plugins {
            plugin.post_parse(&mut parsed_story);
        }
        parsed_story
    }

    pub fn post_export(
        &self,
        parsed_story: &ParsedStory,
        mut runtime_story: RuntimeStory,
    ) -> RuntimeStory {
        for plugin in &self.plugins {
            plugin.post_export(parsed_story, &mut runtime_story);
        }
        runtime_story
    }
}

impl std::fmt::Debug for PluginManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginManager")
            .field("plugin_directories", &self.plugin_directories)
            .field("plugin_count", &self.plugins.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::{Plugin, PluginManager};
    use crate::parsed_hierarchy::Story as ParsedStory;
    use bladeink::story::Story as RuntimeStory;

    #[derive(Default)]
    struct PrefixPlugin {
        pre_parse_called: Cell<bool>,
    }

    impl Plugin for PrefixPlugin {
        fn pre_parse(&self, story_content: &mut String) {
            self.pre_parse_called.set(true);
            story_content.insert_str(0, "prefix ");
        }

        fn post_parse(&self, parsed_story: &mut ParsedStory) {
            parsed_story.root_content_mut().push_text("plugin");
        }

        fn post_export(&self, _parsed_story: &ParsedStory, _runtime_story: &mut RuntimeStory) {}
    }

    #[test]
    fn plugin_manager_runs_pre_parse_hooks_in_order() {
        let plugin = Rc::new(PrefixPlugin::default());
        let manager = PluginManager::with_plugins(Vec::new(), vec![plugin.clone()]);
        assert_eq!("prefix story", manager.pre_parse("story".to_owned()));
        assert!(plugin.pre_parse_called.get());
    }

    #[test]
    fn plugin_manager_runs_post_parse_hooks() {
        let plugin = Rc::new(PrefixPlugin::default());
        let manager = PluginManager::with_plugins(Vec::new(), vec![plugin]);
        let parsed = ParsedStory::new("", None, true);
        let parsed = manager.post_parse(parsed);
        assert_eq!(1, parsed.root_content().content().len());
    }
}
