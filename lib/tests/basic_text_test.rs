use std::fs;
use bink::{story::Story, story_error::StoryError};

mod common;

#[test]
fn oneline_test() -> Result<(), StoryError>  {
    let json_string =
        fs::read_to_string("tests/data/basictext/oneline.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    println!("{}", story.build_string_of_hierarchy());

    assert!(story.can_continue());
    let line = story.cont()?;
    println!("{}", line);
    assert_eq!("Line.", line.trim());
    assert!(!story.can_continue());

    Ok(())
}

#[test]
fn twolines_test() -> Result<(), StoryError> {
    let json_string =
        fs::read_to_string("tests/data/basictext/twolines.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    println!("{}", story.build_string_of_hierarchy());

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("Line.", text[0]);
    assert_eq!("Other line.", text[1]);

    Ok(())
}
