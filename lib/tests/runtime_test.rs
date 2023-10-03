use bink::{story::Story, value_type::{ValueType, StringValue}, story_error::StoryError};

mod common;

// TODO external functions + variable observers

#[test]
fn set_and_get_variable_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("tests/data/runtime/set-get-variables.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(10, story.get_variables_state().get("x").unwrap().get_int().unwrap());

    story.get_variables_state_mut().set("x", ValueType::Int(15))?;

    assert_eq!(15, story.get_variables_state().get("x").unwrap().get_int().unwrap());

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

    let result = story.get_variables_state_mut().set("y", ValueType::new_string("earth"));
    assert!(result.is_err());

    assert_eq!(10, story.get_variables_state().get("x").unwrap().get_int().unwrap());

    story.get_variables_state_mut().set("x", ValueType::Int(15))?;

    assert_eq!(15, story.get_variables_state().get("x").unwrap().get_int().unwrap());

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
    assert_eq!(4, story.get_state().visit_count_at_path_string("two.s2")?);
    assert_eq!(5, story.get_state().visit_count_at_path_string("two")?);

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
    let save_string = story.get_state().to_json()?;

    println!("{}", save_string);

    // recreate game and load state
    Story::new(&json_string).unwrap();
    story.get_state_mut().load_json(&save_string)?;
    
    story.choose_choice_index(0);

    common::next_all(&mut story, &mut text)?;
    assert_eq!("\"There is not a moment to lose!\" I declared.", text.get(1).unwrap());
    assert_eq!("We hurried home to Savile Row as fast as we could.", text.get(2).unwrap());

    // check that we are at the end
    assert!(!story.can_continue());
    assert_eq!(0, story.get_current_choices().len());

    Ok(())
}



