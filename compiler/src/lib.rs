mod ast;
mod emitter;
pub mod error;
mod parser;

pub use error::CompilerError;

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

    /// Compile ink source that may contain `INCLUDE` directives.
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
        let _ = self.options.count_all_visits;
        let expanded = expand_includes(source, &file_handler, 0)?;
        let parsed_story = parser::Parser::new(&expanded).parse().map_err(|e| {
            match &self.options.source_filename {
                Some(filename) => e.with_file(filename.clone()),
                None => e,
            }
        })?;
        emitter::story_to_json_string(&parsed_story).map_err(|e| {
            match &self.options.source_filename {
                Some(filename) => e.with_file(filename.clone()),
                None => e,
            }
        })
    }
}

/// Recursively expand `INCLUDE filename` directives by substituting the
/// contents returned by `file_handler`.  Depth is limited to 32 to avoid
/// infinite recursion in circular includes.
fn expand_includes<F>(source: &str, file_handler: &F, depth: usize) -> Result<String, CompilerError>
where
    F: Fn(&str) -> Result<String, CompilerError>,
{
    const MAX_DEPTH: usize = 32;
    if depth > MAX_DEPTH {
        return Err(CompilerError::invalid_source(
            "INCLUDE recursion depth exceeded 32; possible circular include".to_owned(),
        ));
    }

    let mut result = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(filename) = trimmed.strip_prefix("INCLUDE ") {
            let filename = filename.trim();
            let included = file_handler(filename)?;
            let expanded = expand_includes(&included, file_handler, depth + 1)?;
            result.push_str(&expanded);
            // Ensure a trailing newline after the included content
            if !result.ends_with('\n') {
                result.push('\n');
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{Compiler, CompilerOptions};

    #[test]
    fn error_includes_line_number() {
        // VAR with a bad assignment — error should reference line 3
        let source = "Hello.\nWorld.\nVAR x ==\n";
        let err = Compiler::new().compile(source).unwrap_err();
        let display = err.to_string();
        assert!(
            display.contains("line 3") || display.contains(":3:"),
            "expected line 3 in error, got: {display}"
        );
    }

    #[test]
    fn error_includes_filename_when_set() {
        let source = "VAR x ==\n";
        let options = CompilerOptions {
            source_filename: Some("story.ink".to_owned()),
            ..Default::default()
        };
        let err = Compiler::with_options(options).compile(source).unwrap_err();
        let display = err.to_string();
        assert!(
            display.starts_with("story.ink"),
            "expected filename in error, got: {display}"
        );
    }
}
