use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;

mod common;

#[test]
fn no_choice_test() -> Result<(), StoryError> {
    let mut errors: Vec<String> = Vec::new();

    let text = common::run_story("inkfiles/choices/no-choice-text.ink", None, &mut errors)?;

    assert_eq!(0, errors.len());
    assert_eq!("Hello world!\nHello back!\n", common::join_text(&text));

    Ok(())
}

#[test]
fn one_test() -> Result<(), StoryError> {
    let mut errors: Vec<String> = Vec::new();

    let text = common::run_story("inkfiles/choices/one.ink", None, &mut errors)?;

    assert_eq!(0, errors.len());
    assert_eq!(
        "Hello world!\nHello back!\nHello back!\n",
        common::join_text(&text)
    );

    Ok(())
}

#[test]
fn multi_choice_test() -> Result<(), StoryError> {
    let mut errors: Vec<String> = Vec::new();

    let text = common::run_story(
        "inkfiles/choices/multi-choice.ink",
        Some(vec![0]),
        &mut errors,
    )?;

    assert_eq!(0, errors.len());
    assert_eq!(
        "Hello, world!\nHello back!\nGoodbye\nHello back!\nNice to hear from you\n",
        common::join_text(&text)
    );

    // Select second choice
    let text = common::run_story(
        "inkfiles/choices/multi-choice.ink",
        Some(vec![1]),
        &mut errors,
    )?;

    assert_eq!(0, errors.len());
    assert_eq!(
        "Hello, world!\nHello back!\nGoodbye\nGoodbye\nSee you later\n",
        common::join_text(&text)
    );

    Ok(())
}

#[test]
fn single_choice1_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/choices/single-choice.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("Hello, world!", text[0]);

    Ok(())
}

#[test]
fn single_choic2_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/choices/single-choice.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    assert_eq!("Hello back!", text[0]);
    assert_eq!("Nice to hear from you", text[1]);

    Ok(())
}

#[test]
fn suppress_choice_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/choices/suppress-choice.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(
        "Hello back!",
        story.get_current_choices().first().unwrap().text
    );
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("Nice to hear from you.", text[0]);

    Ok(())
}

#[test]
fn mixed_choice_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/choices/mixed-choice.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(
        "Hello back!",
        story.get_current_choices().first().unwrap().text
    );
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    assert_eq!("Hello right back to you!", text[0]);
    assert_eq!("Nice to hear from you.", text[1]);

    Ok(())
}

#[test]
fn varying_choice_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/choices/varying-choice.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, story.get_current_choices().len());
    assert_eq!(
        "The man with the briefcase?",
        story.get_current_choices()[0].text
    );

    Ok(())
}

#[test]
fn sticky_choice_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/choices/sticky-choice.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, story.get_current_choices().len());

    Ok(())
}

#[test]
fn fallback_choice_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/choices/fallback-choice.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());

    Ok(())
}

#[test]
fn fallback_choice2_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/choices/fallback-choice.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert!(common::is_ended(&story));

    Ok(())
}

#[test]
fn conditional_choice_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/choices/conditional-choice.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(4, story.get_current_choices().len());

    Ok(())
}

#[test]
fn label_flow_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/choices/label-flow.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, story.get_current_choices().len());
    assert_eq!(
        "\'Having a nice day?\'",
        story.get_current_choices()[0].text
    );

    Ok(())
}

#[test]
fn label_flow2_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/choices/label-flow.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(1)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, story.get_current_choices().len());
    assert_eq!("Shove him aside", story.get_current_choices()[1].text);

    Ok(())
}

#[test]
fn label_scope_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/choices/label-scope.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, story.get_current_choices().len());
    assert_eq!("Found gatherpoint", story.get_current_choices()[0].text);

    Ok(())
}

#[test]
fn divert_choice_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/choices/divert-choice.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, story.get_current_choices().len());
    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!(
        "You pull a face, and the soldier comes at you! You shove the guard to one side, but he comes back swinging.",
        text[0]
    );

    assert_eq!(1, story.get_current_choices().len());
    assert_eq!("Grapple and fight", story.get_current_choices()[0].text);

    Ok(())
}

#[test]
fn label_scope_error_test() -> Result<(), StoryError> {
    // Java test body is commented out — just verify the ink compiles
    let ink_source = common::get_file_string("inkfiles/choices/label-scope-error.ink").unwrap();
    let _json_string = Compiler::new().compile(&ink_source).unwrap();
    Ok(())
}

#[test]
fn nested_choice_test() -> Result<(), StoryError> {
    // Choices at level 2 (**) must appear only after the parent level-1 choice (*)
    // is selected, not mixed in with level-1 choices from the start.
    // Sequence: choose option1 (index 0) → then suboption1 (index 0)
    let mut errors: Vec<String> = Vec::new();
    let text = common::run_story(
        "inkfiles/choices/nested-choice.ink",
        Some(vec![0, 0]),
        &mut errors,
    )?;

    assert_eq!(0, errors.len());

    // First choice point must expose exactly one option (option1).
    // option2 is a fresh * choice that only appears after the gather.
    // The full text after both choices should be:
    // option1 (chosen) → suboption1 (chosen) → "text suboption1." → "done sub." → option2 (chosen randomly)
    let joined = common::join_text(&text);
    assert!(
        joined.contains("text suboption1."),
        "expected suboption body text, got: {joined}"
    );
    assert!(
        joined.contains("done sub."),
        "expected gather text after sub-choices, got: {joined}"
    );
    // Use "text option2." as a proxy that option2 was reachable and chosen (not "option2" which
    // is a substring of "suboption2" and could appear before "done sub.")
    assert!(
        joined.contains("text option2."),
        "expected option2 body text to appear after gather, got: {joined}"
    );
    // "text option2." must come AFTER "done sub."
    let pos_done = joined.find("done sub.").unwrap();
    let pos_option2 = joined.find("text option2.").unwrap();
    assert!(
        pos_option2 > pos_done,
        "option2 should appear after 'done sub.' gather, got: {joined}"
    );

    Ok(())
}

// --- Tests ported from the official Ink C# suite (../ink/tests/Tests.cs) ---

// TestChoiceDivertsToDone (Tests.cs:277)
#[test]
fn choice_diverts_to_done_test() -> Result<(), StoryError> {
    let ink = "* choice -> DONE";
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    story.cont()?;

    assert_eq!(1, story.get_current_choices().len());
    story.choose_choice_index(0)?;

    assert_eq!("choice", story.cont()?.trim());

    Ok(())
}

// TestChoiceWithBracketsOnly (Tests.cs:290)
#[test]
fn choice_with_brackets_only_test() -> Result<(), StoryError> {
    let ink = "*   [Option]\n    Text";
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    story.cont()?;

    assert_eq!(1, story.get_current_choices().len());
    assert_eq!("Option", story.get_current_choices()[0].text);

    story.choose_choice_index(0)?;

    assert_eq!("Text\n", story.cont()?);

    Ok(())
}

// TestOnceOnlyChoicesCanLinkBackToSelf (Tests.cs:1453)
#[test]
fn once_only_choices_can_link_back_to_self_test() -> Result<(), StoryError> {
    let ink = r#"
-> opts
= opts
*   (firstOpt) [First choice]   ->  opts
*   {firstOpt} [Second choice]  ->  opts
* -> end

- (end)
    -> END
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    story.continue_maximally()?;

    assert_eq!(1, story.get_current_choices().len());
    assert_eq!("First choice", story.get_current_choices()[0].text);

    story.choose_choice_index(0)?;
    story.continue_maximally()?;

    assert_eq!(1, story.get_current_choices().len());
    assert_eq!("Second choice", story.get_current_choices()[0].text);

    story.choose_choice_index(0)?;
    story.continue_maximally()?;

    assert_eq!(0, story.get_current_choices().len());

    Ok(())
}

// TestOnceOnlyChoicesWithOwnContent (Tests.cs:1484)
#[test]
fn once_only_choices_with_own_content_test() -> Result<(), StoryError> {
    let ink = r#"
VAR times = 3
-> home

== home ==
~ times = times - 1
{times >= 0:-> eat}
I've finished eating now.
-> END

== eat ==
This is the {first|second|third} time.
 * Eat ice-cream[]
 * Drink coke[]
 * Munch cookies[]
-
-> home
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    story.continue_maximally()?;
    assert_eq!(3, story.get_current_choices().len());

    story.choose_choice_index(0)?;
    story.continue_maximally()?;
    assert_eq!(2, story.get_current_choices().len());

    story.choose_choice_index(0)?;
    story.continue_maximally()?;
    assert_eq!(1, story.get_current_choices().len());

    story.choose_choice_index(0)?;
    story.continue_maximally()?;
    assert_eq!(0, story.get_current_choices().len());

    Ok(())
}

// TestDefaultChoices (Tests.cs:524)
#[test]
fn default_choices_test() -> Result<(), StoryError> {
    let ink = r#"
 - (start)
 * [Choice 1]
 * [Choice 2]
 * {false} Impossible choice
 * -> default
 - After choice
 -> start

== default ==
This is default.
-> DONE
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    assert_eq!("", story.cont()?);
    assert_eq!(2, story.get_current_choices().len());

    story.choose_choice_index(0)?;
    assert_eq!("After choice\n", story.cont()?);

    assert_eq!(1, story.get_current_choices().len());

    story.choose_choice_index(0)?;
    assert_eq!(
        "After choice\nThis is default.\n",
        story.continue_maximally()?
    );

    Ok(())
}

// TestVariousDefaultChoices (Tests.cs:3082)
#[test]
fn various_default_choices_test() -> Result<(), StoryError> {
    let ink = r#"
* -> hello
Unreachable
- (hello) 1
* ->
   - - 2
- 3
-> END
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("1\n2\n3\n", story.continue_maximally()?);
    Ok(())
}
