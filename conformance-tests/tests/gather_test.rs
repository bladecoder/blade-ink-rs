use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;

mod common;

#[test]
fn gather_basic_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/gather/gather-basic.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/gather/gather-chain.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(3, story.get_current_choices().len());
    story.choose_choice_index(1)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!(
        "I did not pause for breath but kept on running. The road could not be much further! Mackie would have the engine running, and then I'd be safe.",
        text[0]
    );
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
    let ink_source = common::get_file_string("inkfiles/gather/nested-flow.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    assert_eq!(
        "Mrs. Christie lowered her manuscript a moment. The rest of the writing group sat, open-mouthed.",
        text[1]
    );

    Ok(())
}

#[test]
fn deep_nesting_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/gather/deep-nesting.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/gather/complex-flow.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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
    let ink_source = common::get_file_string("inkfiles/gather/complex-flow.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
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

// --- Tests ported from the official Ink C# suite (../ink/tests/Tests.cs) ---

// TestGatherChoiceSameLine (Tests.cs:1017)
#[test]
fn gather_choice_same_line_test() -> Result<(), StoryError> {
    let ink = "- * hello\n- * world";
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    story.cont()?;
    assert_eq!("hello", story.get_current_choices()[0].text);

    story.choose_choice_index(0)?;
    story.cont()?;
    assert_eq!("world", story.get_current_choices()[0].text);

    Ok(())
}

// TestShouldntGatherDueToChoice (Tests.cs:1756)
#[test]
fn shouldnt_gather_due_to_choice_test() -> Result<(), StoryError> {
    let ink = "* opt\n    - - text\n    * * {false} impossible\n    * * -> END\n- gather";
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    story.continue_maximally()?;
    story.choose_choice_index(0)?;

    // Should NOT fall through to "gather"
    let output = story.continue_maximally()?;
    assert!(
        output.contains("opt"),
        "expected 'opt' in output, got: {output}"
    );
    assert!(
        output.contains("text"),
        "expected 'text' in output, got: {output}"
    );
    assert!(
        !output.contains("gather"),
        "should NOT contain 'gather', got: {output}"
    );

    Ok(())
}

#[test]
fn default_simple_gather_test() -> Result<(), StoryError> {
    let ink = "* ->\n- x\n-> DONE";
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("x\n", &story.cont()?);
    Ok(())
}
