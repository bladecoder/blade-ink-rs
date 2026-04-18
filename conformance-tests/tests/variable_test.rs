use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;

mod common;

#[test]
fn variable_declaration_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/variable/variable-declaration.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!(
        "\"My name is Jean Passepartout, but my friend's call me Jackie. I'm 23 years old.\"",
        text[0]
    );

    Ok(())
}

#[test]
fn var_calc_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/variable/varcalc.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The values are true and -1 and -6 and aa.", text[0]);

    Ok(())
}

#[test]
fn var_string_ink_bug_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/variable/varstringinc.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;
    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    assert_eq!("ab.", text[1]);

    Ok(())
}

#[test]
fn var_divert_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/variable/var-divert.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(1)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("Everybody dies.", text[0]);

    Ok(())
}

#[test]
fn arithmetic_test() -> Result<(), StoryError> {
    let ink = r#"
{ 2 * 3 + 5 * 6 }
{8 mod 3}
{13 % 5}
{ 7 / 3 }
{ 7 / 3.0 }
{ 10 - 2 }
{ 2 * (5-1) }
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    let result = story.continue_maximally()?;
    // Float formatting may vary; check the integer parts and the float approximation
    assert!(result.starts_with("36\n2\n3\n2\n2."), "got: {result}");
    assert!(result.contains("\n8\n8\n"), "got: {result}");
    Ok(())
}

#[test]
fn bools_test() -> Result<(), StoryError> {
    let cases: &[(&str, &str)] = &[
        ("{true}", "true\n"),
        ("{true + 1}", "2\n"),
        ("{2 + true}", "3\n"),
        ("{false + false}", "0\n"),
        ("{true + true}", "2\n"),
        ("{true == 1}", "true\n"),
        ("{not 1}", "false\n"),
        ("{not true}", "false\n"),
        ("{3 > 1}", "true\n"),
    ];
    for (ink, expected) in cases {
        let json = Compiler::new().compile(ink).unwrap();
        let mut story = Story::new(&json)?;
        assert_eq!(*expected, story.cont()?, "ink: {ink}");
    }
    Ok(())
}

#[test]
fn const_test() -> Result<(), StoryError> {
    let ink = r#"
VAR x = c

CONST c = 5

{x}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("5\n", &story.cont()?);
    Ok(())
}

#[test]
fn increment_test() -> Result<(), StoryError> {
    let ink = r#"
VAR x = 5
~ x++
{x}

~ x--
{x}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("6\n5\n", &story.continue_maximally()?);
    Ok(())
}

#[test]
fn else_branches_test() -> Result<(), StoryError> {
    let ink = r#"
VAR x = 3

{
    - x == 1: one
    - x == 2: two
    - else: other
}

{
    - x == 1: one
    - x == 2: two
    - other
}

{ x == 4:
  - The main clause
  - else: other
}

{ x == 4:
  The main clause
- else:
  other
}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("other\nother\nother\nother\n", &story.continue_maximally()?);
    Ok(())
}
