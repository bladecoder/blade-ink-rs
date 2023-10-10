use bladeink::{story::Story, story_error::StoryError};

mod common;

#[test]
fn gather_basic_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/gather/gather-basic.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(1)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(3, text.len());
    assert_eq!("\"Nothing, Monsieur!\" I replied.", text[0]);
    assert_eq!("\"Very good, then.\"", text[1]);
    assert_eq!("With that Monsieur Fogg left the room.", text[2]);

    Ok(())
}

#[test]
fn gather_chain_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/gather/gather-chain.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(3, story.get_current_choices().len());
    story.choose_choice_index(1)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!(                "I did not pause for breath but kept on running. The road could not be much further! Mackie would have the engine running, and then I'd be safe.",
    text[0]);
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!(
        "I reached the road and looked about. And would you believe it?",
        text[0]
    );
    assert_eq!(
        "The road was empty. Mackie was nowhere to be seen.",
        text[1]
    );

    Ok(())
}

#[test]
fn nested_flow_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/gather/nested-flow.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(2)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("\"Myself!\"", text[0]);
    assert_eq!("Mrs. Christie lowered her manuscript a moment. The rest of the writing group sat, open-mouthed.", text[1]);

    Ok(())
}

#[test]
fn deep_nesting_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/gather/deep-nesting.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("\"...Tell us a tale Captain!\"", text[0]);
    assert_eq!("To a man, the crew began to yawn.", text[1]);

    Ok(())
}

#[test]
fn complex_flow1_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/gather/complex-flow.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(1)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!(
        "... but I said nothing and we passed the day in silence.",
        text[0]
    );

    Ok(())
}

#[test]
fn complex_flow2_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/gather/complex-flow.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(3, text.len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    story.choose_choice_index(1)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    story.choose_choice_index(1)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(3, text.len());

    Ok(())
}
