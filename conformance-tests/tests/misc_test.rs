use std::{error::Error, path::Path};

use bladeink::{story::Story, story_error::StoryError, value_type::ValueType};
use bladeink_compiler::{Compiler, CompilerError, CompilerOptions};

mod common;

#[test]
fn operations_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/misc/operations.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!(
        "neg:-3\nmod:1\npow:27\nfloor:3\nceiling:4\nint:3\nfloat:1\n",
        &story.continue_maximally()?
    );

    Ok(())
}

// TestDisallowEmptyDiverts (Tests.cs:1009)
#[test]
fn disallow_empty_diverts_test() {
    let err = Compiler::new().compile("->").unwrap_err();
    assert!(
        err.message().contains("divert"),
        "expected divert-related error, got: {}",
        err.message()
    );
}

// TestDivertNotFoundError (Tests.cs:583)
#[test]
fn divert_not_found_error_test() {
    let ink = r#"
-> knot

== knot ==
Knot.
-> next
"#;
    let err = Compiler::new().compile(ink).unwrap_err();
    assert!(
        err.message().contains("not found"),
        "expected 'not found' error, got: {}",
        err.message()
    );
}

// TestTempGlobalConflict (Tests.cs:2445)
#[test]
fn temp_global_conflict_test() -> Result<(), StoryError> {
    let ink = r#"
-> outer
=== outer
~ temp x = 0
~ f(x)
{x}
-> DONE

=== function f(ref x)
~temp local = 0
~x=x
{setTo3(local)}

=== function setTo3(ref x)
~x = 3
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("0\n", story.continue_maximally()?);
    Ok(())
}

// TestTempNotAllowedCrossStitch (Tests.cs:3447)
#[test]
fn temp_not_allowed_cross_stitch_test() {
    let ink = r#"
-> knot.stitch

== knot (y) ==
~temp x = 5
-> END

= stitch
{x} {y}
-> END
"#;
    let err = Compiler::new().compile(ink).unwrap_err();
    assert!(
        err.message().contains("x") || err.message().contains("y"),
        "expected unresolved variable error, got: {}",
        err.message()
    );
}

#[test]
fn turns_since_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/misc/turns-since.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!("0\n0\n", &story.continue_maximally()?);
    story.choose_choice_index(0)?;
    assert_eq!("1\n", &story.continue_maximally()?);

    Ok(())
}

/**
 * Issue: https://github.com/bladecoder/blade-ink/issues/15
 */
#[test]
fn issue15_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/misc/issue15.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!("This is a test\n", story.cont()?);

    while story.can_continue() {
        // println!(story.buildStringOfHierarchy());
        let line = &story.cont()?;

        if line.starts_with("SET_X:") {
            story.set_variable("x", &ValueType::Int(100))?;
        } else {
            assert_eq!("X is set\n", line);
        }
    }

    Ok(())
}

#[test]
fn newlines_with_string_eval_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/misc/newlines_with_string_eval.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!("A\nB\nA\n3\nB\n", &story.continue_maximally()?);

    Ok(())
}

#[test]
fn min_max_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/misc/min-max.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!(
        "min_int:3\nmax_int:5\nmin_float:1.5\nmax_float:2.5\nmin_neg:-1\nmax_neg:1\n",
        &story.continue_maximally()?
    );

    Ok(())
}

#[test]
fn choice_count_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/misc/choice-count.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    story.continue_maximally()?;
    // 4 choices: A, B, C, plus the conditional one which passes because CHOICE_COUNT() == 3
    // when it is evaluated (3 choices already generated before it)
    assert_eq!(4, story.get_current_choices().len());
    // Choose the conditional choice (index 3: "All three available")
    story.choose_choice_index(3)?;
    story.continue_maximally()?;

    Ok(())
}

#[test]
fn turns_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/misc/turns.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!("Turn: 0\n", &story.continue_maximally()?);
    story.choose_choice_index(0)?;
    assert_eq!("Turn: 1\n", &story.continue_maximally()?);

    Ok(())
}

/// Issue: https://github.com/bladecoder/blade-ink/issues/escape-hash
#[test]
fn escape_hash_compiles_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/misc/escape-hash.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!("Bug with escape character #\n", story.cont()?);

    Ok(())
}

#[test]
fn i18n() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/misc/i18n.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!("áéíóú ñ\n", story.cont()?);
    assert_eq!("你好\n", story.cont()?);
    let current_tags = story.get_current_tags()?;
    assert_eq!(1, current_tags.len());
    assert_eq!("áé", current_tags[0]);
    assert_eq!("你好世界\n", story.cont()?);

    Ok(())
}

#[test]
fn include_basic_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/include/main.ink")?;
    let base_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("inkfiles/include");

    let json_string = Compiler::new()
        .compile_with_file_handler(&ink_source, |filename| {
            let path = base_dir.join(filename);
            std::fs::read_to_string(&path).map_err(|e| {
                CompilerError::invalid_source(format!(
                    "Failed to read included file '{}': {}",
                    filename, e
                ))
            })
        })
        .unwrap();

    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    assert_eq!("This is included.", text[0]);
    assert_eq!("This is main.", text[1]);

    Ok(())
}

#[test]
fn include_nested_relative_paths_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/include/nested/main.ink")?;
    let base_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("inkfiles/include/nested");

    let json_string = Compiler::new()
        .compile_with_file_handler(&ink_source, |filename| {
            let path = base_dir.join(filename);
            std::fs::read_to_string(&path).map_err(|e| {
                CompilerError::invalid_source(format!(
                    "Failed to read included file '{}': {}",
                    filename, e
                ))
            })
        })
        .unwrap();

    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(3, text.len());
    assert_eq!("Leaf content.", text[0]);
    assert_eq!("Scene content.", text[1]);
    assert_eq!("Main content.", text[2]);

    Ok(())
}

#[test]
fn count_all_visits_option_changes_compiled_container_flags() {
    let ink_source = "=== knot ===\nHello\n-> END\n";

    let json_without_count_all_visits = Compiler::with_options(CompilerOptions {
        count_all_visits: false,
        source_filename: None,
    })
    .compile(ink_source)
    .unwrap();

    let json_with_count_all_visits = Compiler::with_options(CompilerOptions {
        count_all_visits: true,
        source_filename: None,
    })
    .compile(ink_source)
    .unwrap();

    assert!(
        !json_without_count_all_visits.contains("\"knot\":[\"^Hello\",\"\\n\",\"end\",{\"#f\":1}]"),
        "unexpected visit-count flags when count_all_visits=false: {json_without_count_all_visits}"
    );
    assert!(
        json_with_count_all_visits.contains("\"knot\":[\"^Hello\",\"\\n\",\"end\",{\"#f\":1}]"),
        "missing visit-count flags when count_all_visits=true: {json_with_count_all_visits}"
    );
}

#[test]
fn the_intercept_compiles_test() {
    let ink_source = common::get_file_string("inkfiles/TheIntercept.ink").unwrap();
    Compiler::new().compile(&ink_source).unwrap();
}

// --- Tests ported from the official Ink C# suite (../ink/tests/Tests.cs) ---

// TestReadCountAcrossCallstack (Tests.cs:1651)
#[test]
fn read_count_across_callstack_test() -> Result<(), StoryError> {
    let ink = r#"
-> first

== first ==
1) Seen first {first} times.
-> second ->
2) Seen first {first} times.
-> DONE

== second ==
In second.
->->
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    assert_eq!(
        "1) Seen first 1 times.\nIn second.\n2) Seen first 1 times.\n",
        &story.continue_maximally()?
    );

    Ok(())
}

#[test]
fn hello_world_test() -> Result<(), StoryError> {
    let ink = "Hello world";
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("Hello world\n", &story.cont()?);
    Ok(())
}

#[test]
fn empty_test() -> Result<(), StoryError> {
    let ink = "";
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    // Empty story: currentText should be empty (no content to output)
    let text = story.continue_maximally()?;
    assert_eq!("", text.as_str());
    Ok(())
}

#[test]
fn end_test() -> Result<(), StoryError> {
    let ink = r#"
hello
-> END
world
-> END
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("hello\n", &story.continue_maximally()?);
    Ok(())
}

#[test]
fn end2_test() -> Result<(), StoryError> {
    let ink = r#"
-> test

== test ==
hello
-> END
world
-> END
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("hello\n", &story.continue_maximally()?);
    Ok(())
}

#[test]
fn whitespace_test() -> Result<(), StoryError> {
    let ink = r#"
-> firstKnot
=== firstKnot
    Hello!
    -> anotherKnot

=== anotherKnot
    World.
    -> END
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("Hello!\nWorld.\n", &story.continue_maximally()?);
    Ok(())
}

#[test]
fn escape_character_test() -> Result<(), StoryError> {
    // \| is the escaped pipe character in Ink
    let ink = "{true:this is a '\\|' character|this isn't}";
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("this is a '|' character\n", &story.continue_maximally()?);
    Ok(())
}

#[test]
fn trivial_condition_test() -> Result<(), StoryError> {
    let ink = r#"
{
- false:
   beep
}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    story.cont()?;
    Ok(())
}
