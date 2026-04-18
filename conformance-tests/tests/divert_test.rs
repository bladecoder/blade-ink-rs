use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;

mod common;

#[test]
fn simple_divert_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/divert/simple-divert.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("We arrived into London at 9.45pm exactly.", text[0]);
    assert_eq!(
        "We hurried home to Savile Row as fast as we could.",
        text[1]
    );

    Ok(())
}

#[test]
fn invisible_divert_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/divert/invisible-divert.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
fn divert_on_choice_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/divert/divert-on-choice.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("You open the gate, and step out onto the path.", text[0]);

    Ok(())
}

#[test]
fn complex_branching1_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/divert/complex-branching.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    assert_eq!("\"There is not a moment to lose!\" I declared.", text[0]);
    assert_eq!(
        "We hurried home to Savile Row as fast as we could.",
        text[1]
    );

    Ok(())
}

#[test]
fn complex_branching2_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/divert/complex-branching.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(1)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(3, text.len());
    assert_eq!(
        "\"Monsieur, let us savour this moment!\" I declared.",
        text[0]
    );
    assert_eq!(
        "My master clouted me firmly around the head and dragged me out of the door.",
        text[1]
    );
    assert_eq!(
        "He insisted that we hurried home to Savile Row as fast as we could.",
        text[2]
    );

    Ok(())
}

#[test]
fn divert_to_weave_points_test() -> Result<(), StoryError> {
    let ink = r#"
-> knot.stitch.gather

== knot ==
= stitch
- hello
    * (choice) test
        choice content
- (gather)
  gather

  {stopping:
    - -> knot.stitch.choice
    - second time round
  }

-> END
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!(
        "gather\ntest\nchoice content\ngather\nsecond time round\n",
        &story.continue_maximally()?
    );
    Ok(())
}
