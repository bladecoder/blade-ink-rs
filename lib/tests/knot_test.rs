use bink::{story::Story, story_error::StoryError};

mod common;

#[test]
fn single_line_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("tests/data/knot/single-line.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("Hello, world!", text[0]);

    Ok(())
}

#[test]
fn multi_line_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("tests/data/knot/multi-line.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(3, text.len());
    assert_eq!("Hello, world!", text[0]);
    assert_eq!("Hello?", text[1]);
    assert_eq!("Hello, are you there?", text[2]);

    Ok(())
}

#[test]
fn strip_empty_lines_test() -> Result<(), StoryError> {
    let json_string =
        common::get_json_string("tests/data/knot/strip-empty-lines.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(3, text.len());
    assert_eq!("Hello, world!", text[0]);
    assert_eq!("Hello?", text[1]);
    assert_eq!("Hello, are you there?", text[2]);

    Ok(())
}

#[test]
fn param_strings_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("tests/data/knot/param-strings.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(2)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("\"I accuse myself!\" Poirot declared.", text[0]);

    Ok(())
}

#[test]
fn param_ints_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("tests/data/knot/param-ints.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(1)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("You give 2 dollars.", text[0]);

    Ok(())
}

#[test]
fn param_floats_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("tests/data/knot/param-floats.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(1)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("You give 2.5 dollars.", text[0]);

    Ok(())
}

#[test]
fn param_vars_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("tests/data/knot/param-vars.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(1)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("You give 2 dollars.", text[0]);

    Ok(())
}

#[test]
fn param_multi_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("tests/data/knot/param-multi.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("You give 1 or 2 dollars. Hmm.", text[0]);

    Ok(())
}

#[test]
fn param_recurse_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("tests/data/knot/param-recurse.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    assert_eq!("\"The result is 120!\" you announce.", text[0]);

    Ok(())
}
