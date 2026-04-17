use std::{error::Error, path::Path};

use bladeink::{story::Story, story_error::StoryError, value_type::ValueType};
use bladeink_compiler::{Compiler, CompilerError};

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

#[test]
fn read_counts_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/misc/read-counts.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!(
        "Count start: 0 0 0\n1\n2\n3\nCount end: 3 3 3\n",
        &story.continue_maximally()?
    );

    Ok(())
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
fn the_intercept_compiles_test() {
    let ink_source = common::get_file_string("inkfiles/TheIntercept.ink").unwrap();
    Compiler::new().compile(&ink_source).unwrap();
}
