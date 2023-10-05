use core::panic;
use std::{cell::RefCell, rc::Rc};

use bink::{story::Story, value_type::{ValueType, StringValue}, story_error::StoryError, story_callbacks::VariableObserver};

mod common;

// TODO external functions

struct VObserver {
    expected_value: i32,
}

impl VariableObserver for VObserver {
    fn changed(&mut self, variable_name: &str, new_value: &ValueType) {
        if !"x".eq(variable_name) {
            panic!();
        }

        if let ValueType::Int(v) = new_value {
            assert_eq!(self.expected_value, *v);
        } else {
            panic!();
        }

        self.expected_value = 10;
    }
}

#[test]
fn variable_observers_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("tests/data/runtime/variable-observers.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    story.observe_variable("x", Rc::new(RefCell::new(VObserver { expected_value: 5})));

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0);
    common::next_all(&mut story, &mut text)?;

    Ok(())
}


#[test]
fn set_and_get_variable_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("tests/data/runtime/set-get-variables.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(10, story.get_variable("x").unwrap().get_int().unwrap());

    story.set_variable("x", &ValueType::Int(15))?;

    assert_eq!(15, story.get_variable("x").unwrap().get_int().unwrap());

    story.choose_choice_index(0);

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("OK", text[0]);

    Ok(())
}


#[test]
fn set_non_existant_variable_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("tests/data/runtime/set-get-variables.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    let result = story.set_variable("y", &ValueType::new_string("earth"));
    assert!(result.is_err());

    assert_eq!(10, story.get_variable("x").unwrap().get_int().unwrap());

    story.set_variable("x", &ValueType::Int(15))?;

    assert_eq!(15, story.get_variable("x").unwrap().get_int().unwrap());

    story.choose_choice_index(0);

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("OK", text[0]);

    Ok(())
}

#[test]
fn jump_knot_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("tests/data/runtime/jump-knot.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    story.choose_path_string("two", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("Two", text.get(0).unwrap());

    text.clear();
    story.choose_path_string("three", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("Three", text.get(0).unwrap());

    text.clear();
    story.choose_path_string("one", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("One", text.get(0).unwrap());

    text.clear();
    story.choose_path_string("two", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("Two", text.get(0).unwrap());

    Ok(())
}

#[test]
fn jump_stitch_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("tests/data/runtime/jump-stitch.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    story.choose_path_string("two.sthree", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("Two.3", text.get(0).unwrap());

    text.clear();
    story.choose_path_string("one.stwo", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("One.2", text.get(0).unwrap());

    text.clear();
    story.choose_path_string("one.sone", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("One.1", text.get(0).unwrap());

    text.clear();
    story.choose_path_string("two.stwo", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("Two.2", text.get(0).unwrap());

    Ok(())
}

#[test]
fn read_visit_counts_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("tests/data/runtime/read-visit-counts.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(4, story.get_visit_count_at_path_string("two.s2")?);
    assert_eq!(5, story.get_visit_count_at_path_string("two")?);

    Ok(())
}

#[test]
fn load_save_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("tests/data/runtime/load-save.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("We arrived into London at 9.45pm exactly.", text.get(0).unwrap());

    // save the game state
    let save_string = story.save_state()?;

    println!("{}", save_string);

    // recreate game and load state
    Story::new(&json_string).unwrap();
    story.load_state(&save_string)?;
    
    story.choose_choice_index(0);

    common::next_all(&mut story, &mut text)?;
    assert_eq!("\"There is not a moment to lose!\" I declared.", text.get(1).unwrap());
    assert_eq!("We hurried home to Savile Row as fast as we could.", text.get(2).unwrap());

    // check that we are at the end
    assert!(!story.can_continue());
    assert_eq!(0, story.get_current_choices().len());

    Ok(())
}



