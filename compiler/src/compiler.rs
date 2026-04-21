use std::rc::Rc;

use bladeink::story::Story as RuntimeStory;

use crate::{
    bootstrap::legacy_root::{Compiler as BootstrapCompiler, CompilerOptions as BootstrapOptions},
    error::CompilerError,
    file_handler::FileHandler,
    parsed_hierarchy::Story,
    stats, wave1,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
    Warning,
    Error,
}

pub type ErrorHandler = Rc<dyn Fn(String, ErrorType)>;

#[derive(Clone)]
pub struct CompilerOptions {
    pub source_filename: Option<String>,
    pub plugin_directories: Vec<String>,
    pub count_all_visits: bool,
    pub error_handler: Option<ErrorHandler>,
    pub file_handler: Option<Rc<dyn FileHandler>>,
}

impl std::fmt::Debug for CompilerOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompilerOptions")
            .field("source_filename", &self.source_filename)
            .field("plugin_directories", &self.plugin_directories)
            .field("count_all_visits", &self.count_all_visits)
            .field("has_error_handler", &self.error_handler.is_some())
            .field("has_file_handler", &self.file_handler.is_some())
            .finish()
    }
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            source_filename: None,
            plugin_directories: Vec::new(),
            count_all_visits: true,
            error_handler: None,
            file_handler: None,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Compiler {
    source: Option<String>,
    options: CompilerOptions,
}

impl Compiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: CompilerOptions) -> Self {
        Self {
            source: None,
            options,
        }
    }

    pub fn from_source(source: impl Into<String>) -> Self {
        Self {
            source: Some(source.into()),
            options: CompilerOptions::default(),
        }
    }

    pub fn from_source_with_options(source: impl Into<String>, options: CompilerOptions) -> Self {
        Self {
            source: Some(source.into()),
            options,
        }
    }

    pub fn options(&self) -> &CompilerOptions {
        &self.options
    }

    pub fn parse(&self) -> Result<Story, CompilerError> {
        let source = self.source.as_deref().ok_or_else(|| {
            CompilerError::unsupported_feature(
                "Compiler::parse requires the source to be provided via Compiler::from_source"
                    .to_owned(),
            )
        })?;

        Ok(Story::new(
            source,
            self.options.source_filename.clone(),
            self.options.count_all_visits,
        ))
    }

    pub fn compile_story(&self) -> Result<RuntimeStory, CompilerError> {
        let parsed = self.parse()?;
        self.compile_story_from_source(parsed.source())
    }

    pub fn compile_story_from_source(&self, source: &str) -> Result<RuntimeStory, CompilerError> {
        if let Some(story) = self.try_compile_wave1_story(source)? {
            return Ok(story);
        }

        let json = self.bootstrap().compile(source)?;
        RuntimeStory::new(&json).map_err(|err| CompilerError::invalid_source(err.to_string()))
    }

    pub fn compile_story_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<RuntimeStory, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
    {
        let json = self.compile_json_with_file_handler(source, file_handler)?;
        RuntimeStory::new(&json).map_err(|err| CompilerError::invalid_source(err.to_string()))
    }

    pub fn compile(&self, source: &str) -> Result<String, CompilerError> {
        self.compile_json(source)
    }

    pub fn compile_json(&self, source: &str) -> Result<String, CompilerError> {
        self.compile_internal(source)
    }

    pub fn compile_to_stats(&self, source: &str) -> Result<stats::Stats, CompilerError> {
        self.ensure_bootstrap_allowed("stats generation")?;
        self.bootstrap().compile_to_stats(source)
    }

    pub fn compile_to_stats_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<stats::Stats, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
    {
        self.ensure_bootstrap_allowed("stats generation with file handler")?;
        self.bootstrap()
            .compile_to_stats_with_file_handler(source, file_handler)
    }

    pub fn compile_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<String, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
    {
        self.compile_json_with_file_handler(source, file_handler)
    }

    pub fn compile_json_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<String, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
    {
        self.ensure_bootstrap_allowed("compilation with file handler")?;
        self.bootstrap()
            .compile_with_file_handler(source, file_handler)
    }

    fn compile_internal(&self, source: &str) -> Result<String, CompilerError> {
        if let Some(file_handler) = &self.options.file_handler {
            self.ensure_bootstrap_allowed("compilation with file handler")?;
            return self.compile_json_with_file_handler(source, |filename| {
                file_handler.load_ink_file_contents(filename)
            });
        }

        if let Some(story) = self.try_compile_wave1_story(source)? {
            return story
                .to_compiled_json()
                .map_err(|err| CompilerError::invalid_source(err.to_string()));
        }

        self.ensure_bootstrap_allowed("compilation fallback")?;
        self.bootstrap().compile(source)
    }

    fn bootstrap(&self) -> BootstrapCompiler {
        BootstrapCompiler::with_options(BootstrapOptions {
            count_all_visits: self.options.count_all_visits,
            source_filename: self.options.source_filename.clone(),
        })
    }

    fn try_compile_wave1_story(&self, source: &str) -> Result<Option<RuntimeStory>, CompilerError> {
        match wave1::compile(source, self.options.count_all_visits) {
            Ok(compiled) => Ok(Some(compiled.story)),
            Err(error @ CompilerError::UnsupportedFeature { .. }) => {
                if Self::bootstrap_disabled() {
                    Err(error.with_message_prefix(
                        "bootstrap fallback disabled by BLADEINK_DISABLE_BOOTSTRAP=1; ",
                    ))
                } else {
                    Ok(None)
                }
            }
            Err(error) => Err(error),
        }
    }

    fn ensure_bootstrap_allowed(&self, operation: &str) -> Result<(), CompilerError> {
        if Self::bootstrap_disabled() {
            return Err(CompilerError::unsupported_feature(format!(
                "bootstrap fallback disabled by BLADEINK_DISABLE_BOOTSTRAP=1; {operation} still requires the legacy compiler"
            )));
        }

        Ok(())
    }

    fn bootstrap_disabled() -> bool {
        std::env::var("BLADEINK_DISABLE_BOOTSTRAP")
            .map(|value| Self::bootstrap_disabled_value(&value))
            .unwrap_or(false)
    }

    fn bootstrap_disabled_value(value: &str) -> bool {
        value == "1" || value.eq_ignore_ascii_case("true")
    }
}

#[cfg(test)]
mod tests {
    use super::Compiler;

    #[test]
    fn bootstrap_disabled_value_accepts_numeric_and_boolean_true() {
        assert!(Compiler::bootstrap_disabled_value("1"));
        assert!(Compiler::bootstrap_disabled_value("true"));
        assert!(Compiler::bootstrap_disabled_value("TRUE"));
        assert!(!Compiler::bootstrap_disabled_value("0"));
        assert!(!Compiler::bootstrap_disabled_value("false"));
    }
}
