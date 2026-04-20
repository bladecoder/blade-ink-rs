#[derive(Debug, Default, Clone)]
pub struct PluginManager {
    plugin_directories: Vec<String>,
}

impl PluginManager {
    pub fn new(plugin_directories: Vec<String>) -> Self {
        Self { plugin_directories }
    }

    pub fn plugin_directories(&self) -> &[String] {
        &self.plugin_directories
    }
}
