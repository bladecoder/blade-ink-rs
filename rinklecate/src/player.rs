//! Interactive story player for the command line.
//!
//! Supports both plain-text and JSON output modes (`-j`), mirroring the
//! behaviour of the blade-ink-java `CommandLinePlayer`.

use std::cell::RefCell;
use std::io::{self, BufRead, Write};
use std::rc::Rc;

use bladeink::story::{
    errors::{ErrorHandler, ErrorType},
    Story,
};

use crate::Options;

// ---------------------------------------------------------------------------
// Error handler — collects errors and warnings produced during story execution
// ---------------------------------------------------------------------------

struct StoryErrors {
    errors: Vec<String>,
    warnings: Vec<String>,
}

impl StoryErrors {
    fn new() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }))
    }
}

impl ErrorHandler for StoryErrors {
    fn error(&mut self, message: &str, error_type: ErrorType) {
        match error_type {
            ErrorType::Error => self.errors.push(message.to_owned()),
            _ => self.warnings.push(message.to_owned()),
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Build a Story from compiled JSON and run it interactively.
///
/// The error handler is registered immediately after construction so that
/// any runtime warnings during play are collected gracefully.
pub fn play_from_json(json_string: &str, opts: &Options) -> anyhow::Result<()> {
    let mut story =
        Story::new(json_string).map_err(|e| anyhow::anyhow!("Failed to load story: {e}"))?;
    story.set_allow_external_function_fallbacks(true);
    play(story, opts)
}

/// Run an already-constructed story interactively.
pub fn play(mut story: Story, opts: &Options) -> anyhow::Result<()> {
    let err_handler = StoryErrors::new();
    story.set_error_handler(err_handler.clone());
    story.set_allow_external_function_fallbacks(true);

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        // --- Evaluate story until a choice is needed or story ends ---
        evaluate_story(&mut story, &err_handler, opts)?;

        let choices = story.get_current_choices();

        if choices.is_empty() {
            if opts.keep_open_after_story_finish {
                if opts.json_output {
                    println!("{{\"end\": true}}");
                } else {
                    println!("--- End of story ---");
                }
            }
            break;
        }

        // --- Present choices ---
        if opts.json_output {
            print_choices_json(&choices);
        } else {
            println!();
            for (i, c) in choices.iter().enumerate() {
                println!("{}: {}", i + 1, c.text);
                if !c.tags.is_empty() {
                    println!("# tags: {}", c.tags.join(", "));
                }
            }
        }

        // --- Read user input ---
        loop {
            if opts.json_output {
                print!("{{\"needInput\": true}}");
            } else {
                print!("?> ");
            }
            io::stdout().flush()?;

            let raw = match lines.next() {
                Some(Ok(line)) => line,
                Some(Err(e)) => anyhow::bail!("Input error: {e}"),
                None => {
                    // stdin closed
                    if opts.json_output {
                        println!("{{\"close\": true}}");
                    } else {
                        println!("<User input stream closed.>");
                    }
                    return Ok(());
                }
            };

            let trimmed = raw.trim();

            if trimmed.is_empty() {
                continue;
            }

            match parse_input(trimmed) {
                InputResult::Choice(idx) => {
                    if idx >= choices.len() {
                        if !opts.json_output {
                            eprintln!("Choice out of range");
                        }
                        continue;
                    }
                    story.choose_choice_index(idx)?;
                    break;
                }
                InputResult::Divert(path) => {
                    if let Err(e) = story.choose_path_string(&path, true, None) {
                        if opts.json_output {
                            println!(
                                "{{\"issues\": [\"Error diverting to '{}': {}\"]}}",
                                path,
                                e.to_string().replace('"', "\\\"")
                            );
                        } else {
                            eprintln!("<error diverting to '{path}': {e}>");
                        }
                    }
                    break;
                }
                InputResult::Help => {
                    let msg = "Type a choice number or a divert (e.g. '-> myKnot'), 'quit' to exit";
                    if opts.json_output {
                        println!("{{\"cmdOutput\": \"{}\"}}", msg.replace('"', "\\\""));
                    } else {
                        println!("{msg}");
                    }
                }
                InputResult::Exit => {
                    return Ok(());
                }
                InputResult::Unknown => {
                    if !opts.json_output {
                        eprintln!("Unexpected input. Type 'help' or a choice number.");
                    }
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Evaluate story content (text + tags + errors)
// ---------------------------------------------------------------------------

fn evaluate_story(
    story: &mut Story,
    err_handler: &Rc<RefCell<StoryErrors>>,
    opts: &Options,
) -> anyhow::Result<()> {
    while story.can_continue() {
        let text = story.cont()?;
        let tags = story.get_current_tags()?;

        if opts.json_output {
            println!("{{\"text\": \"{}\"}}", escape_json_string(&text));
        } else {
            print!("{text}");
        }

        if !tags.is_empty() {
            if opts.json_output {
                let tag_strings: Vec<String> = tags
                    .iter()
                    .map(|t| format!("\"{}\"", escape_json_string(t)))
                    .collect();
                println!("{{\"tags\": [{}]}}", tag_strings.join(", "));
            } else {
                println!("# tags: {}", tags.join(", "));
            }
        }

        // Flush accumulated errors/warnings
        flush_messages(err_handler, opts);
    }

    Ok(())
}

fn flush_messages(err_handler: &Rc<RefCell<StoryErrors>>, opts: &Options) {
    let mut h = err_handler.borrow_mut();
    if h.errors.is_empty() && h.warnings.is_empty() {
        return;
    }
    if opts.json_output {
        let all: Vec<String> = h
            .warnings
            .iter()
            .chain(h.errors.iter())
            .map(|s| format!("\"{}\"", escape_json_string(s)))
            .collect();
        println!("{{\"issues\": [{}]}}", all.join(", "));
    } else {
        for msg in &h.warnings {
            eprintln!("{msg}");
        }
        for msg in &h.errors {
            eprintln!("{msg}");
        }
    }
    h.errors.clear();
    h.warnings.clear();
}

// ---------------------------------------------------------------------------
// JSON helpers
// ---------------------------------------------------------------------------

fn print_choices_json(choices: &[std::rc::Rc<bladeink::choice::Choice>]) {
    let mut parts = Vec::new();
    for c in choices {
        if c.tags.is_empty() {
            parts.push(format!("{{\"text\": \"{}\"}}", escape_json_string(&c.text)));
        } else {
            let tag_strings: Vec<String> = c
                .tags
                .iter()
                .map(|t| format!("\"{}\"", escape_json_string(t)))
                .collect();
            parts.push(format!(
                "{{\"text\": \"{}\", \"tags\": [{}], \"tag_count\": {}}}",
                escape_json_string(&c.text),
                tag_strings.join(", "),
                c.tags.len()
            ));
        }
    }
    println!("{{\"choices\": [{}]}}", parts.join(", "));
}

/// Escape a string for inclusion inside a JSON string value.
fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Input parsing
// ---------------------------------------------------------------------------

enum InputResult {
    Choice(usize),
    Divert(String),
    Help,
    Exit,
    Unknown,
}

fn parse_input(input: &str) -> InputResult {
    let lower = input.to_lowercase();

    if lower == "quit" || lower == "exit" {
        return InputResult::Exit;
    }

    if lower == "help" {
        return InputResult::Help;
    }

    // Divert: "-> knot_name"
    let words: Vec<&str> = input.split_whitespace().collect();
    if words.len() == 2 && words[0] == "->" {
        return InputResult::Divert(words[1].to_owned());
    }

    // Choice number (1-based)
    if let Ok(n) = input.trim().parse::<usize>()
        && n >= 1 {
            return InputResult::Choice(n - 1);
        }

    InputResult::Unknown
}
