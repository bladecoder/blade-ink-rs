mod ast;
mod consts;
mod emitter;
pub mod error;
mod includes;
mod inline;
mod parser;
pub mod stats;
mod validator;

pub use error::CompilerError;

/// Maps each line of the expanded source (0-indexed) to its origin:
/// the source filename and the 1-based line number within that file.
pub type LineMap = Vec<(String, usize)>;

#[derive(Debug, Clone)]
pub struct CompilerOptions {
    pub count_all_visits: bool,
    /// Optional filename used in error messages (e.g. `"main.ink"`).
    pub source_filename: Option<String>,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            count_all_visits: true,
            source_filename: None,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Compiler {
    options: CompilerOptions,
}

impl Compiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: CompilerOptions) -> Self {
        Self { options }
    }

    pub fn compile(&self, source: &str) -> Result<String, CompilerError> {
        self.compile_with_file_handler(source, |filename| {
            Err(CompilerError::unsupported_feature(format!(
                "INCLUDE directive found for '{}', but no file handler was provided. \
                 Use Compiler::compile_with_file_handler to resolve includes.",
                filename
            )))
        })
    }

    /// Parse the ink source and return story statistics without emitting JSON.
    ///
    /// Useful for the `-s` flag of `rinklecate`.
    pub fn compile_to_stats(&self, source: &str) -> Result<stats::Stats, CompilerError> {
        self.compile_to_stats_with_file_handler(source, |filename| {
            Err(CompilerError::unsupported_feature(format!(
                "INCLUDE directive found for '{}', but no file handler was provided.",
                filename
            )))
        })
    }

    /// Parse the ink source (resolving INCLUDEs via `file_handler`) and return story statistics.
    pub fn compile_to_stats_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<stats::Stats, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
    {
        let source_name = self
            .options
            .source_filename
            .as_deref()
            .unwrap_or("<source>");
        let parsed_story = includes::parse_story_with_includes(source, &file_handler, source_name)?;
        Ok(stats::Stats::generate(&parsed_story))
    }

    ///
    /// `file_handler` receives the filename from each `INCLUDE` directive and
    /// must return the full contents of that file as a `String`.  The handler
    /// is called recursively for any `INCLUDE` directives found inside included
    /// files.
    pub fn compile_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<String, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
    {
        let source_name = self
            .options
            .source_filename
            .as_deref()
            .unwrap_or("<source>");
        let parsed_story = includes::parse_story_with_includes(source, &file_handler, source_name)?;
        let parsed_story = consts::resolve(parsed_story);
        validator::validate(&parsed_story)?;
        emitter::story_to_json_string(&parsed_story, self.options.count_all_visits)
            .map_err(|e| CompilerError::invalid_source(e.to_string()))
    }
}
