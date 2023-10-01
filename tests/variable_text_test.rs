use bladeink::{story::Story, story_error::StoryError};

mod common;

#[test]
fn sequence_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/variabletext/sequence.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. \"Three!\"", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. \"Two!\"", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. \"One!\"", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. There was the white noise racket of an explosion.", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. There was the white noise racket of an explosion.", text[0]);

    Ok(())
}

#[test]
fn cycle_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/variabletext/cycle.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. \"Three!\"", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. \"Two!\"", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. \"One!\"", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. \"Three!\"", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. \"Two!\"", text[0]);

    Ok(())
}

#[test]
fn once_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/variabletext/once.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. \"Three!\"", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. \"Two!\"", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. \"One!\"", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life.", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life.", text[0]);

    Ok(())
}

#[test]
fn empty_elements_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/variabletext/empty-elements.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life.", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life.", text[0]);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The radio hissed into life. \"One!\"", text[0]);

    Ok(())
}

#[test]
fn list_in_choice_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/variabletext/list-in-choice.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("\"Hello, Master!\"", story.get_current_choices()[0].text);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("\"Hello, Monsieur!\"", story.get_current_choices()[0].text);

    story.choose_choice_index(0);
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("\"Hello, you!\"", story.get_current_choices()[0].text);

    Ok(())
}