use std::path::{Path, PathBuf};

use crate::{
    ast::{Node, ParsedStory},
    error::CompilerError,
    parser::Parser,
};

const MAX_INCLUDE_DEPTH: usize = 32;

pub(crate) fn parse_story_with_includes<F>(
    source: &str,
    file_handler: &F,
    source_name: &str,
) -> Result<ParsedStory, CompilerError>
where
    F: Fn(&str) -> Result<String, CompilerError>,
{
    parse_recursive(source, file_handler, Path::new(""), source_name, 0)
}

fn parse_recursive<F>(
    source: &str,
    file_handler: &F,
    current_dir: &Path,
    source_name: &str,
    depth: usize,
) -> Result<ParsedStory, CompilerError>
where
    F: Fn(&str) -> Result<String, CompilerError>,
{
    if depth > MAX_INCLUDE_DEPTH {
        return Err(CompilerError::invalid_source(
            "INCLUDE recursion depth exceeded 32; possible circular include".to_owned(),
        ));
    }

    let preprocessed;
    let source = if source.contains("/*") {
        preprocessed = strip_block_comments(source);
        preprocessed.as_str()
    } else {
        source
    };

    let mut segments = Vec::new();
    let mut current_lines = Vec::new();
    for line in source.lines() {
        if let Some(filename) = line.trim().strip_prefix("INCLUDE ") {
            push_ink_segment(&mut segments, &mut current_lines);
            segments.push(Segment::Include(filename.trim().to_owned()));
        } else {
            current_lines.push(line);
        }
    }
    push_ink_segment(&mut segments, &mut current_lines);

    let mut merged = ParsedStory::default();
    for segment in segments {
        match segment {
            Segment::Ink(text) => {
                if text.lines().all(|line| line.trim().is_empty()) {
                    continue;
                }
                let partial = Parser::new(&text)
                    .parse()
                    .map_err(|error| error.with_file(source_name.to_owned()))?;
                merge_stories(&mut merged, partial);
            }
            Segment::Include(filename) => {
                let include_path = normalize_include_path(current_dir, &filename);
                let included = file_handler(include_path.to_string_lossy().as_ref())?;
                let next_dir = include_path.parent().unwrap_or_else(|| Path::new(""));
                let include_name = include_path.to_string_lossy();
                let included_story =
                    parse_recursive(&included, file_handler, next_dir, &include_name, depth + 1)?;
                merged.root.push(Node::Newline);
                merge_stories(&mut merged, included_story);
            }
        }
    }

    Ok(merged)
}

enum Segment {
    Ink(String),
    Include(String),
}

fn push_ink_segment(segments: &mut Vec<Segment>, lines: &mut Vec<&str>) {
    if !lines.is_empty() {
        segments.push(Segment::Ink(lines.join("\n") + "\n"));
        lines.clear();
    }
}

fn merge_stories(destination: &mut ParsedStory, source: ParsedStory) {
    destination.root.extend(source.root);
    destination.flows.extend(source.flows);
    destination.globals.extend(source.globals);
    destination
        .list_declarations
        .extend(source.list_declarations);
    destination
        .external_functions
        .extend(source.external_functions);
    destination.consts.extend(source.consts);
}

fn normalize_include_path(current_dir: &Path, filename: &str) -> PathBuf {
    let include_path = Path::new(filename);
    if include_path.is_absolute() || current_dir.as_os_str().is_empty() {
        include_path.to_path_buf()
    } else {
        current_dir.join(include_path)
    }
}

/// Strip block comments while preserving line positions for diagnostics.
fn strip_block_comments(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_block = false;

    while let Some(ch) = chars.next() {
        if in_block {
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block = false;
            } else if ch == '\n' {
                result.push('\n');
            }
        } else if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_block = true;
        } else {
            result.push(ch);
        }
    }

    result
}
