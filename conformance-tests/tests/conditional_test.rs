use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;

mod common;

#[test]
fn iftrue_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/conditional/iftrue.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/iffalse.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/ifelse.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/ifelse-ext.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/ifelse-ext-text1.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/ifelse-ext-text2.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/ifelse-ext-text3.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/condtext.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/condtext.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/condopt.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/condopt.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/stopping.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/cycle.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/once.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/shuffle.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/shuffle_stopping.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/shuffle_once.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/multiline.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/multiline-divert.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/conditional/multiline-choice.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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

#[test]
fn conditionals_test() -> Result<(), StoryError> {
    let ink = r#"
{false:not true|true}
{
   - 4 > 5: not true
   - 5 > 4: true
}
{ 2*2 > 3:
   - true
   - not true
}
{
   - 1 > 3: not true
   - { 2+2 == 4:
        - true
        - not true
   }
}
{ 2*3:
   - 1+7: not true
   - 9: not true
   - 1+1+1+3: true
   - 9-3: also true but not printed
}
{ true:
    great
    right?
}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!(
        "true\ntrue\ntrue\ntrue\ntrue\ngreat\nright?\n",
        &story.continue_maximally()?
    );
    Ok(())
}

#[test]
fn conditional_choices_test() -> Result<(), StoryError> {
    let ink = r#"
* { true } { false } not displayed
* { true } { true }
  { true and true }  one
* { false } not displayed
* (name) { true } two
* { true }
  { true }
  three
* { true }
  four
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    story.continue_maximally()?;
    let choices = story.get_current_choices();
    assert_eq!(4, choices.len());
    assert_eq!("one", choices[0].text);
    assert_eq!("two", choices[1].text);
    assert_eq!("three", choices[2].text);
    assert_eq!("four", choices[3].text);
    Ok(())
}

#[test]
fn divert_in_conditional_test() -> Result<(), StoryError> {
    let ink = r#"
=== intro
= top
    { main: -> done }
    -> END
= main
    -> top
= done
    -> END
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("", &story.continue_maximally()?);
    Ok(())
}

#[test]
fn conditional_choice_in_weave_test() -> Result<(), StoryError> {
    let ink = r#"
- start
 {
    - true: * [go to a stitch] -> a_stitch
 }
- gather should be seen
-> DONE

= a_stitch
    result
    -> END
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!(
        "start\ngather should be seen\n",
        &story.continue_maximally()?
    );
    assert_eq!(1, story.get_current_choices().len());
    story.choose_choice_index(0)?;
    assert_eq!("result\n", &story.cont()?);
    Ok(())
}

#[test]
fn conditional_choice_in_weave2_test() -> Result<(), StoryError> {
    let ink = r#"
- first gather
    * [option 1]
    * [option 2]
- the main gather
{false:
    * unreachable option -> END
}
- bottom gather
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("first gather\n", &story.cont()?);
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(0)?;
    assert_eq!(
        "the main gather\nbottom gather\n",
        &story.continue_maximally()?
    );
    assert_eq!(0, story.get_current_choices().len());
    Ok(())
}

#[test]
fn empty_multiline_conditional_branch_test() -> Result<(), StoryError> {
    let ink = r#"
{ 3:
    - 3:
    - 4:
        txt
}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("", &story.cont()?);
    Ok(())
}
