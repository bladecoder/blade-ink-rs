mod ast;
mod emitter;
pub mod error;
mod parser;
pub mod stats;
mod validator;

use std::path::{Path, PathBuf};

use ast::ParsedStory;

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
        let parsed_story =
            parse_story_with_includes(source, &file_handler, Path::new(""), source_name, 0)?;
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
        let parsed_story =
            parse_story_with_includes(source, &file_handler, Path::new(""), source_name, 0)?;
        validator::validate(&parsed_story)?;
        emitter::story_to_json_string(&parsed_story, self.options.count_all_visits)
            .map_err(|e| CompilerError::invalid_source(e.to_string()))
    }
}

/// Parse an ink source file, resolving `INCLUDE` directives by parsing each
/// included file independently and merging the resulting ASTs.
///
/// Each included file is parsed on its own — its knots/stitches are independent
/// of the including file, which matches the behaviour of the official inklecate
/// compiler (files are not simply concatenated).
///
/// The `root` nodes (content before the first knot) from every file — the main
/// file and all includes — are concatenated in include order to form the story's
/// root sequence.  Knots and other declarations from all files are merged into
/// shared collections.
fn parse_story_with_includes<F>(
    source: &str,
    file_handler: &F,
    current_dir: &Path,
    source_name: &str,
    depth: usize,
) -> Result<ParsedStory, CompilerError>
where
    F: Fn(&str) -> Result<String, CompilerError>,
{
    const MAX_DEPTH: usize = 32;
    if depth > MAX_DEPTH {
        return Err(CompilerError::invalid_source(
            "INCLUDE recursion depth exceeded 32; possible circular include".to_owned(),
        ));
    }

    // Split source into segments: either an INCLUDE directive or a block of
    // plain ink lines.  We parse each segment separately so that knots in an
    // included file never "bleed" into the parsing context of the parent.
    let mut segments: Vec<SegmentKind> = Vec::new();
    let mut current_lines: Vec<&str> = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(filename) = trimmed.strip_prefix("INCLUDE ") {
            // Flush accumulated plain lines as one segment
            if !current_lines.is_empty() {
                segments.push(SegmentKind::Ink(current_lines.join("\n") + "\n"));
                current_lines.clear();
            }
            segments.push(SegmentKind::Include(filename.trim().to_owned()));
        } else {
            current_lines.push(line);
        }
    }
    if !current_lines.is_empty() {
        segments.push(SegmentKind::Ink(current_lines.join("\n") + "\n"));
    }

    // Accumulate everything into a merged story
    let mut merged = ParsedStory::default();

    for segment in segments {
        match segment {
            SegmentKind::Ink(text) => {
                // Parse this segment in isolation.  If it's only whitespace /
                // comments the parser may reject it as "empty" — skip gracefully.
                let text_trimmed = text.lines().filter(|l| !l.trim().is_empty()).count();
                if text_trimmed == 0 {
                    continue;
                }
                let partial = parser::Parser::new(&text)
                    .parse()
                    .map_err(|e| e.with_file(source_name.to_owned()))?;
                merge_stories(&mut merged, partial);
            }
            SegmentKind::Include(filename) => {
                let include_path = normalize_include_path(current_dir, &filename);
                let included = file_handler(include_path.to_string_lossy().as_ref())?;
                let next_dir = include_path.parent().unwrap_or_else(|| Path::new(""));
                let inc_name = include_path.to_string_lossy().into_owned();
                let inc_story = parse_story_with_includes(
                    &included,
                    file_handler,
                    next_dir,
                    &inc_name,
                    depth + 1,
                )?;
                merge_stories(&mut merged, inc_story);
            }
        }
    }

    Ok(merged)
}

enum SegmentKind {
    Ink(String),
    Include(String),
}

/// Merge `src` into `dst`:
/// - root nodes are appended in order
/// - flows, globals, list_declarations, external_functions are extended
fn merge_stories(dst: &mut ParsedStory, src: ParsedStory) {
    dst.root.extend(src.root);
    dst.flows.extend(src.flows);
    dst.globals.extend(src.globals);
    dst.list_declarations.extend(src.list_declarations);
    dst.external_functions.extend(src.external_functions);
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
                    Err(CompilerError::invalid_source(format!(
                        "file not found: {name}"
                    )))
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
