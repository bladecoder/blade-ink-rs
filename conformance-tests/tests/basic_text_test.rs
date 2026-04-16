use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;
use std::env;

mod common;

#[test]
fn oneline_test() -> Result<(), StoryError> {
    println!("{}", env::current_dir().unwrap().to_string_lossy());

    let ink_source = common::get_file_string("inkfiles/basictext/oneline.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
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
    let ink_source = common::get_file_string("inkfiles/basictext/twolines.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("Line.", text[0]);
    assert_eq!("Other line.", text[1]);

    Ok(())
}
