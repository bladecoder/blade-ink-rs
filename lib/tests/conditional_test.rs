use bink::{story::Story, story_error::StoryError};

mod common;

#[test]
fn iftrue_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/conditional/iftrue.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value is 1.", text[0]);

    Ok(())
}

#[test]
fn iffalse_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/conditional/iffalse.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value is 3.", text[0]);

    Ok(())
}

#[test]
fn ifelse_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/conditional/ifelse.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value is 1.", text[0]);

    Ok(())
}

#[test]
fn ifelse_ext_test() -> Result<(), StoryError> {
    let json_string =
        common::get_json_string("inkfiles/conditional/ifelse-ext.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value is -1.", text[0]);

    Ok(())
}

#[test]
fn ifelse_ext_text1_test() -> Result<(), StoryError> {
    let json_string =
        common::get_json_string("inkfiles/conditional/ifelse-ext-text1.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("This is text 1.", text[0]);
    assert_eq!(1, story.get_current_choices().len());
    story.choose_choice_index(0)?;

    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("This is the end.", text[1]);

    Ok(())
}

#[test]
fn ifelse_ext_text2_test() -> Result<(), StoryError> {
    let json_string =
        common::get_json_string("inkfiles/conditional/ifelse-ext-text2.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("This is text 2.", text[0]);
    assert_eq!(1, story.get_current_choices().len());
    story.choose_choice_index(0)?;

    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("This is the end.", text[1]);

    Ok(())
}

#[test]
fn ifelse_ext_text3_test() -> Result<(), StoryError> {
    let json_string =
        common::get_json_string("inkfiles/conditional/ifelse-ext-text3.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("This is text 3.", text[0]);
    assert_eq!(1, story.get_current_choices().len());
    story.choose_choice_index(0)?;

    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("This is the end.", text[1]);

    Ok(())
}

#[test]
fn cond_text1_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/conditional/condtext.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(3, text.len());
    assert_eq!(
        "I stared at Monsieur Fogg. \"But surely you are not serious?\" I demanded.",
        text[1]
    );

    Ok(())
}

#[test]
fn cond_text2_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/conditional/condtext.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(1)?;
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    assert_eq!(
        "I stared at Monsieur Fogg. \"But there must be a reason for this trip,\" I observed.",
        text[0]
    );

    Ok(())
}

#[test]
fn cond_opt1_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/conditional/condopt.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, story.get_current_choices().len());

    Ok(())
}

#[test]
fn cond_opt2_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/conditional/condopt.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(1)?;
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, story.get_current_choices().len());

    Ok(())
}

#[test]
fn stopping_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/conditional/stopping.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("I entered the casino.", text[0]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("I entered the casino again.", text[0]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("Once more, I went inside.", text[0]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("Once more, I went inside.", text[0]);
    story.choose_choice_index(0)?;

    Ok(())
}

#[test]
fn cycle_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/conditional/cycle.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("I held my breath.", text[0]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("I waited impatiently.", text[0]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("I paused.", text[0]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("I held my breath.", text[0]);
    story.choose_choice_index(0)?;

    Ok(())
}

#[test]
fn once_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/conditional/once.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("Would my luck hold?", text[0]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("Could I win the hand?", text[0]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(0, text.len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(0, text.len());

    Ok(())
}

#[test]
fn shuffle_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/conditional/shuffle.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());

    // No check of the result, as that is random

    Ok(())
}

#[test]
fn shuffle_stopping() -> Result<(), StoryError> {
    let json_string =
        common::get_json_string("inkfiles/conditional/shuffle_stopping.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("final", text[0]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("final", text[0]);

    // No check of the result, as that is random

    Ok(())
}

#[test]
fn shuffle_once() -> Result<(), StoryError> {
    let json_string =
        common::get_json_string("inkfiles/conditional/shuffle_once.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(0, text.len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(0, text.len());

    // No check of the result, as that is random

    Ok(())
}

#[test]
fn multiline_test() -> Result<(), StoryError> {
    let json_string = common::get_json_string("inkfiles/conditional/multiline.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("At the table, I drew a card. Ace of Hearts.", text[0]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("I drew a card. 2 of Diamonds.", text[0]);
    assert_eq!("\"Should I hit you again,\" the croupier asks.", text[1]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("I drew a card. King of Spades.", text[0]);
    assert_eq!("\"You lose,\" he crowed.", text[1]);

    Ok(())
}

#[test]
fn multiline_divert_test() -> Result<(), StoryError> {
    let json_string =
        common::get_json_string("inkfiles/conditional/multiline-divert.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("At the table, I drew a card. Ace of Hearts.", text[0]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("I drew a card. 2 of Diamonds.", text[0]);
    assert_eq!("\"Should I hit you again,\" the croupier asks.", text[1]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("I drew a card. King of Spades.", text[0]);
    assert_eq!("\"You lose,\" he crowed.", text[1]);

    Ok(())
}

#[test]
fn multiline_choice_test() -> Result<(), StoryError> {
    let json_string =
        common::get_json_string("inkfiles/conditional/multiline-choice.ink.json").unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("At the table, I drew a card. Ace of Hearts.", text[0]);
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("I left the table.", text[0]);

    Ok(())
}
