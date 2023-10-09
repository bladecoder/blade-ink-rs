use bink::{story::Story, story_error::StoryError};

mod common;

#[test]
fn simple_glue_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("tests/data/glue/simple-glue.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("Some content with glue.", text[0]);

    Ok(())
}

#[test]
fn glue_with_divert_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("tests/data/glue/glue-with-divert.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!(
        "We hurried home to Savile Row as fast as we could.",
        text[0]
    );

    Ok(())
}

#[test]
fn has_left_right_glue_matching_test() -> Result<(), StoryError> {
    let json_string =
        common::get_json_string("tests/data/glue/left-right-glue-matching.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    assert_eq!("A line.", text[0]);
    assert_eq!("Another line.", text[1]);

    Ok(())
}

#[test]
fn bugfix1_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("tests/data/glue/testbugfix1.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    assert_eq!("A", text[0]);
    assert_eq!("C", text[1]);

    Ok(())
}

#[test]
fn bugfix2_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("tests/data/glue/testbugfix2.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    //assert_eq!("A", text[0]);
    assert_eq!("X", text[1]);

    Ok(())
}
