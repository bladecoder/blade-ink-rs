mod ast;
mod emitter;
pub mod error;
mod parser;
pub mod stats;

use std::path::{Path, PathBuf};

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
        let (expanded, line_map) =
            expand_includes(source, &file_handler, Path::new(""), source_name, 0)?;
        let parsed_story = parser::Parser::new(&expanded)
            .parse()
            .map_err(|e| remap_error(e, &line_map))?;
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
        let (expanded, line_map) =
            expand_includes(source, &file_handler, Path::new(""), source_name, 0)?;
        let parsed_story = parser::Parser::new(&expanded)
            .parse()
            .map_err(|e| remap_error(e, &line_map))?;
        emitter::story_to_json_string(&parsed_story, self.options.count_all_visits)
            .map_err(|e| remap_error(e, &line_map))
    }
}

/// Recursively expand `INCLUDE filename` directives by substituting the
/// contents returned by `file_handler`.  Depth is limited to 32 to avoid
/// infinite recursion in circular includes.
///
/// Returns the expanded source string and a line map that associates each
/// line of the expanded text (0-indexed) with the originating filename and
/// 1-based line number within that file.
fn expand_includes<F>(
    source: &str,
    file_handler: &F,
    current_dir: &Path,
    source_name: &str,
    depth: usize,
) -> Result<(String, LineMap), CompilerError>
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
    let mut line_map: LineMap = Vec::new();

    for (src_line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(filename) = trimmed.strip_prefix("INCLUDE ") {
            let filename = filename.trim();
            let include_path = normalize_include_path(current_dir, filename);
            let included = file_handler(include_path.to_string_lossy().as_ref())?;
            let next_dir = include_path.parent().unwrap_or_else(|| Path::new(""));
            let inc_name = include_path.to_string_lossy().into_owned();
            let (expanded, mut inc_map) =
                expand_includes(&included, file_handler, next_dir, &inc_name, depth + 1)?;
            result.push_str(&expanded);
            line_map.append(&mut inc_map);
            // Ensure a trailing newline after the included content
            if !result.ends_with('\n') {
                result.push('\n');
                // The extra newline belongs to the including file's INCLUDE line
                line_map.push((source_name.to_owned(), src_line_idx + 1));
            }
        } else {
            result.push_str(line);
            result.push('\n');
            line_map.push((source_name.to_owned(), src_line_idx + 1));
        }
    }
    Ok((result, line_map))
}

/// Translate a compiler error's line number (which refers to the expanded,
/// flattened source) back to the original filename and line using `line_map`.
fn remap_error(error: CompilerError, line_map: &LineMap) -> CompilerError {
    // Only remap when the error has a line number but no file yet.
    let expanded_line = match &error {
        CompilerError::InvalidSource {
            file: None,
            line: Some(l),
            ..
        }
        | CompilerError::UnsupportedFeature {
            file: None,
            line: Some(l),
            ..
        } => *l,
        _ => return error,
    };

    // line_map is 0-indexed; expanded_line is 1-based.
    if let Some((filename, orig_line)) = line_map.get(expanded_line.saturating_sub(1)) {
        error
            .with_file(filename.clone())
            .with_line_override(*orig_line)
    } else {
        error
    }
}

fn normalize_include_path(current_dir: &Path, filename: &str) -> PathBuf {
    let include_path = Path::new(filename);
    if include_path.is_absolute() || current_dir.as_os_str().is_empty() {
        include_path.to_path_buf()
    } else {
        current_dir.join(include_path)
    }
}

#[cfg(test)]
mod tests {
    use super::{Compiler, CompilerError, CompilerOptions};

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

    #[test]
    fn error_from_included_file_shows_included_filename() {
        // The main file includes "sub.ink" which has a bad divert on its line 2.
        // The error should report "sub.ink:2", not the main file.
        let main_source = "Hello.\nINCLUDE sub.ink\n";
        let sub_source = "Good line.\n->\n";

        let options = CompilerOptions {
            source_filename: Some("main.ink".to_owned()),
            ..Default::default()
        };
        let err = Compiler::with_options(options)
            .compile_with_file_handler(main_source, |name| {
                if name == "sub.ink" {
                    Ok(sub_source.to_owned())
                } else {
                    Err(CompilerError::invalid_source(format!("file not found: {name}")))
                }
            })
            .unwrap_err();
        let display = err.to_string();
        assert!(
            display.contains("sub.ink"),
            "expected 'sub.ink' in error, got: {display}"
        );
        assert!(
            display.contains(":2:") || display.contains("line 2"),
            "expected line 2 in error, got: {display}"
        );
    }
}
