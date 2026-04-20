use std::{collections::HashMap, rc::Rc};

use bladeink::{
    CommandType, Container, ControlCommand, Divert, Glue,
    PushPopType, RTObject,
    story::Story as RuntimeStory,
    Value,
};

use crate::{error::CompilerError, string_parser::CommentEliminator};

pub(crate) struct CompiledWave1Story {
    pub story: RuntimeStory,
}

#[derive(Debug, Clone)]
struct ParsedStory {
    root: Flow,
    knots: Vec<Knot>,
}

#[derive(Debug, Clone)]
struct Knot {
    name: String,
    body: Flow,
    stitches: Vec<Stitch>,
}

#[derive(Debug, Clone)]
struct Stitch {
    name: String,
    body: Flow,
}

#[derive(Debug, Clone, Default)]
struct Flow {
    statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
enum Statement {
    Text(TextLine),
    Divert { target: String },
}

#[derive(Debug, Clone)]
struct TextLine {
    segments: Vec<TextSegment>,
    divert_target: Option<String>,
}

#[derive(Debug, Clone)]
enum TextSegment {
    Text(String),
    Glue,
}

#[derive(Debug, Clone, Copy)]
enum Scope<'a> {
    Root,
    Knot(&'a Knot),
    Stitch(&'a Knot),
}

#[derive(Debug, Clone, Copy)]
enum Cursor {
    Root,
    Knot(usize),
    Stitch {
        knot_index: usize,
        stitch_index: usize,
    },
}

pub(crate) fn compile(
    source: &str,
    count_all_visits: bool,
) -> Result<CompiledWave1Story, CompilerError> {
    let parsed = parse_story(source)?;
    validate_story(&parsed)?;
    let root = compile_story_container(&parsed, count_all_visits);
    let story = RuntimeStory::from_compiled(root, Vec::new())
        .map_err(|err| CompilerError::invalid_source(err.to_string()))?;

    Ok(CompiledWave1Story { story })
}

fn parse_story(source: &str) -> Result<ParsedStory, CompilerError> {
    let processed = CommentEliminator::process(source);
    let mut story = ParsedStory {
        root: Flow::default(),
        knots: Vec::new(),
    };

    let mut cursor = Cursor::Root;

    for (line_index, raw_line) in processed.lines().enumerate() {
        let line_number = line_index + 1;
        let trimmed = raw_line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if let Some(name) = parse_knot_header(trimmed) {
            story.knots.push(Knot {
                name,
                body: Flow::default(),
                stitches: Vec::new(),
            });
            cursor = Cursor::Knot(story.knots.len() - 1);
            continue;
        }

        if let Some(name) = parse_stitch_header(trimmed) {
            let (Cursor::Knot(knot_index) | Cursor::Stitch { knot_index, .. }) = cursor else {
                return Err(CompilerError::unsupported_feature(
                    "wave 1 parser does not support top-level weave points yet",
                )
                .with_line(line_number));
            };

            story.knots[knot_index].stitches.push(Stitch {
                name,
                body: Flow::default(),
            });
            cursor = Cursor::Stitch {
                knot_index,
                stitch_index: story.knots[knot_index].stitches.len() - 1,
            };
            continue;
        }

        let statement = parse_statement(trimmed, line_number)?;
        current_flow_mut(&mut story, &cursor).statements.push(statement);
    }

    Ok(story)
}

fn current_flow_mut<'a>(story: &'a mut ParsedStory, cursor: &Cursor) -> &'a mut Flow {
    match *cursor {
        Cursor::Root => &mut story.root,
        Cursor::Knot(index) => &mut story.knots[index].body,
        Cursor::Stitch {
            knot_index,
            stitch_index,
        } => &mut story.knots[knot_index].stitches[stitch_index].body,
    }
}

fn parse_statement(line: &str, line_number: usize) -> Result<Statement, CompilerError> {
    if line.starts_with("->") {
        let target = parse_divert_target(&line[2..], line_number)?;
        return Ok(Statement::Divert { target });
    }

    if matches!(
        line.chars().next(),
        Some('*' | '+' | '-' | '~' | '{' | '#')
    ) || line.starts_with("INCLUDE ")
        || line.starts_with("EXTERNAL ")
        || line.starts_with("LIST ")
        || line.starts_with("CONST ")
        || line.starts_with("VAR ")
        || line.starts_with("== function")
        || line.starts_with("=== function")
        || line.contains('#')
    {
        return Err(CompilerError::unsupported_feature(format!(
            "wave 1 parser does not support this syntax yet: {line}"
        ))
        .with_line(line_number));
    }

    if line.contains('[')
        || line.contains(']')
        || line.contains('{')
        || line.contains('}')
        || line.contains('<')
            && !line.contains("<>")
        || line.contains('>')
            && !line.contains("->")
            && !line.contains("<>")
    {
        return Err(CompilerError::unsupported_feature(format!(
            "wave 1 parser does not support this syntax yet: {line}"
        ))
        .with_line(line_number));
    }

    let (text_part, divert_target) = split_inline_divert(line, line_number)?;
    let segments = parse_text_segments(text_part, line_number)?;

    if segments.is_empty() && divert_target.is_none() {
        return Err(CompilerError::invalid_source("expected content").with_line(line_number));
    }

    Ok(Statement::Text(TextLine {
        segments,
        divert_target,
    }))
}

fn parse_knot_header(line: &str) -> Option<String> {
    if !line.starts_with("==") {
        return None;
    }

    let name = line.trim_matches('=').trim();
    if name.is_empty() || name.contains('(') || name.contains(')') || name.contains("function") {
        return None;
    }

    Some(name.to_owned())
}

fn parse_stitch_header(line: &str) -> Option<String> {
    if !line.starts_with('=') || line.starts_with("==") {
        return None;
    }

    let name = line.trim_start_matches('=').trim();
    if name.is_empty() || name.contains('(') || name.contains(')') {
        return None;
    }

    Some(name.to_owned())
}

fn parse_divert_target(raw: &str, line_number: usize) -> Result<String, CompilerError> {
    let target = raw.trim();
    if target.is_empty() {
        return Err(CompilerError::unsupported_feature(
            "wave 1 parser only supports fully parsed divert targets",
        )
        .with_line(line_number));
    }

    if target.contains(' ') || target.contains('\t') {
        return Err(CompilerError::unsupported_feature(format!(
            "wave 1 parser only supports simple divert targets: {target}"
        ))
        .with_line(line_number));
    }

    Ok(target.to_owned())
}

fn split_inline_divert(
    line: &str,
    line_number: usize,
) -> Result<(&str, Option<String>), CompilerError> {
    let Some(index) = line.rfind("->") else {
        return Ok((line, None));
    };

    if index == 0 {
        return Ok((line, None));
    }

    let before = &line[..index];
    let after = parse_divert_target(&line[index + 2..], line_number)?;
    Ok((before, Some(after)))
}

fn parse_text_segments(line: &str, line_number: usize) -> Result<Vec<TextSegment>, CompilerError> {
    let mut segments = Vec::new();
    let parts: Vec<&str> = line.split("<>").collect();

    if line.contains("<<") || line.contains(">>") {
        return Err(CompilerError::unsupported_feature(format!(
            "wave 1 parser does not support this syntax yet: {line}"
        ))
        .with_line(line_number));
    }

    for (index, part) in parts.iter().enumerate() {
        if !part.is_empty() {
            segments.push(TextSegment::Text((*part).to_owned()));
        }

        if index + 1 < parts.len() {
            segments.push(TextSegment::Glue);
        }
    }

    Ok(segments)
}

fn validate_story(parsed: &ParsedStory) -> Result<(), CompilerError> {
    validate_flow(parsed, &parsed.root, Scope::Root)?;

    for knot in &parsed.knots {
        validate_flow(parsed, &knot.body, Scope::Knot(knot))?;
        for stitch in &knot.stitches {
            validate_flow(parsed, &stitch.body, Scope::Stitch(knot))?;
        }
    }

    Ok(())
}

fn validate_flow(parsed: &ParsedStory, flow: &Flow, scope: Scope<'_>) -> Result<(), CompilerError> {
    for statement in &flow.statements {
        match statement {
            Statement::Divert { target } => {
                if !target_exists(parsed, scope, target) {
                    return Err(CompilerError::unsupported_feature(format!(
                        "wave 1 parser cannot validate target yet: {target}"
                    )));
                }
            }
            Statement::Text(text) => {
                if let Some(target) = &text.divert_target
                    && !target_exists(parsed, scope, target)
                {
                    return Err(CompilerError::unsupported_feature(format!(
                        "wave 1 parser cannot validate target yet: {target}"
                    )));
                }
            }
        }
    }

    Ok(())
}

fn target_exists(parsed: &ParsedStory, scope: Scope<'_>, target: &str) -> bool {
    if matches!(target, "END" | "DONE") {
        return true;
    }

    if let Some((knot_name, stitch_name)) = target.split_once('.') {
        return parsed
            .knots
            .iter()
            .find(|knot| knot.name == knot_name)
            .map(|knot| knot.stitches.iter().any(|stitch| stitch.name == stitch_name))
            .unwrap_or(false);
    }

    match scope {
        Scope::Root => parsed.knots.iter().any(|knot| knot.name == target),
        Scope::Knot(knot) | Scope::Stitch(knot) => {
            knot.stitches.iter().any(|stitch| stitch.name == target)
                || parsed.knots.iter().any(|candidate| candidate.name == target)
        }
    }
}

fn compile_story_container(parsed: &ParsedStory, count_all_visits: bool) -> Rc<Container> {
    let named_content = parsed
        .knots
        .iter()
        .map(|knot| {
            let container = compile_knot(parsed, knot, count_all_visits);
            (knot.name.clone(), container)
        })
        .collect();

    let mut content = compile_flow(parsed, &parsed.root, Scope::Root);
    if !flow_has_terminal_statement(&parsed.root) {
        content.push(done_command());
    }

    Container::new(None, 0, content, named_content)
}

fn compile_knot(parsed: &ParsedStory, knot: &Knot, count_all_visits: bool) -> Rc<Container> {
    let named_content = knot
        .stitches
        .iter()
        .map(|stitch| {
            let container = compile_stitch(parsed, knot, stitch, count_all_visits);
            (stitch.name.clone(), container)
        })
        .collect();

    let mut content = if knot.body.statements.is_empty() && !knot.stitches.is_empty() {
        vec![divert_object(&format!("{}.{}", knot.name, knot.stitches[0].name))]
    } else {
        compile_flow(parsed, &knot.body, Scope::Knot(knot))
    };
    if knot.body.statements.is_empty() && !knot.stitches.is_empty() {
        // Auto-entering the first stitch is already terminal for this container.
    } else if !flow_has_terminal_statement(&knot.body) {
        content.push(done_command());
    }

    Container::new(
        Some(knot.name.clone()),
        visit_count_flags(count_all_visits),
        content,
        named_content,
    )
}

fn compile_stitch(
    parsed: &ParsedStory,
    knot: &Knot,
    stitch: &Stitch,
    count_all_visits: bool,
) -> Rc<Container> {
    let mut content = compile_flow(parsed, &stitch.body, Scope::Stitch(knot));
    if !flow_has_terminal_statement(&stitch.body) {
        content.push(done_command());
    }
    Container::new(
        Some(stitch.name.clone()),
        visit_count_flags(count_all_visits),
        content,
        HashMap::new(),
    )
}

fn compile_flow(parsed: &ParsedStory, flow: &Flow, scope: Scope<'_>) -> Vec<Rc<dyn RTObject>> {
    let mut content = Vec::new();

    for statement in &flow.statements {
        match statement {
            Statement::Divert { target } => content.push(compile_divert(parsed, scope, target)),
            Statement::Text(text) => {
                for segment in &text.segments {
                    match segment {
                        TextSegment::Text(value) => {
                            content.push(Rc::new(Value::new(value.as_str())) as Rc<dyn RTObject>);
                        }
                        TextSegment::Glue => {
                            content.push(Rc::new(Glue::new()) as Rc<dyn RTObject>);
                        }
                    }
                }

                if let Some(target) = &text.divert_target {
                    if matches!(target.as_str(), "END" | "DONE") {
                        content.push(Rc::new(Value::new("\n")) as Rc<dyn RTObject>);
                    }
                    content.push(compile_divert(parsed, scope, target));
                } else {
                    content.push(Rc::new(Value::new("\n")) as Rc<dyn RTObject>);
                }
            }
        }
    }

    content
}

fn compile_divert(parsed: &ParsedStory, scope: Scope<'_>, target: &str) -> Rc<dyn RTObject> {
    match target {
        "END" => Rc::new(ControlCommand::new(CommandType::End)),
        "DONE" => Rc::new(ControlCommand::new(CommandType::Done)),
        _ => Rc::new(Divert::new(
            false,
            PushPopType::Tunnel,
            false,
            0,
            false,
            None,
            Some(&resolve_target(parsed, scope, target)),
        )),
    }
}

fn resolve_target(parsed: &ParsedStory, scope: Scope<'_>, target: &str) -> String {
    if target.contains('.') {
        return target.to_owned();
    }

    match scope {
        Scope::Root => target.to_owned(),
        Scope::Knot(knot) | Scope::Stitch(knot) => {
            if knot.stitches.iter().any(|stitch| stitch.name == target) {
                return format!("{}.{}", knot.name, target);
            }

            if parsed.knots.iter().any(|candidate| candidate.name == target) {
                return target.to_owned();
            }

            target.to_owned()
        }
    }
}

fn divert_object(target: &str) -> Rc<dyn RTObject> {
    Rc::new(Divert::new(
        false,
        PushPopType::Tunnel,
        false,
        0,
        false,
        None,
        Some(target),
    ))
}

fn done_command() -> Rc<dyn RTObject> {
    Rc::new(ControlCommand::new(CommandType::Done))
}

fn flow_has_terminal_statement(flow: &Flow) -> bool {
    match flow.statements.last() {
        Some(Statement::Divert { .. }) => true,
        Some(Statement::Text(text)) => text.divert_target.is_some(),
        None => false,
    }
}

fn visit_count_flags(count_all_visits: bool) -> i32 {
    if count_all_visits { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::compile;
    use crate::CompilerError;

    #[test]
    fn compiles_basic_text_with_glue() {
        let compiled = compile("Some <>\ncontent <>\nwith glue.\n", true).expect("compile");
        let mut story = compiled.story;
        assert_eq!("Some content with glue.\n", story.continue_maximally().expect("continue"));
    }

    #[test]
    fn compiles_inline_divert_to_end() {
        let ink = "We hurried home to Savile Row -> as_fast_as_we_could\n\n=== as_fast_as_we_could ===\nas fast as we could. -> END\n";
        let compiled = compile(ink, true).expect("compile");
        let mut story = compiled.story;
        assert_eq!(
            "We hurried home to Savile Row as fast as we could.\n",
            story.continue_maximally().expect("continue")
        );
    }

    #[test]
    fn compiles_simple_knot_and_stitch_resolution() {
        let ink = "-> travel\n\n=== travel ===\n-> first\n\n= first\nI settled my master.\n-> END\n";
        let compiled = compile(ink, true).expect("compile");
        let mut story = compiled.story;
        assert_eq!("I settled my master.\n", story.continue_maximally().expect("continue"));
    }

    #[test]
    fn rejects_choices_for_bootstrap_fallback() {
        let result = compile("* [Choice]\n", true);
        assert!(matches!(
            result,
            Err(CompilerError::UnsupportedFeature { .. })
        ));
    }
}
