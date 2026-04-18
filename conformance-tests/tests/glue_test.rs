use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;

mod common;

#[test]
fn simple_glue_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/glue/simple-glue.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("Some content with glue.", text[0]);

    Ok(())
}

#[test]
fn glue_with_divert_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/glue/glue-with-divert.ink").unwrap();
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
fn has_left_right_glue_matching_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/glue/left-right-glue-matching.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    assert_eq!("A line.", text[0]);
    assert_eq!("Another line.", text[1]);

    Ok(())
}

#[test]
fn bugfix1_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/glue/testbugfix1.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    assert_eq!("A", text[0]);
    assert_eq!("C", text[1]);

    Ok(())
}

#[test]
fn bugfix2_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/glue/testbugfix2.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    assert_eq!(2, text.len());
    //assert_eq!("A", text[0]);
    assert_eq!("X", text[1]);

    Ok(())
}

// TestImplicitInlineGlue (Tests.cs:1088)
#[test]
fn implicit_inline_glue_test() -> Result<(), StoryError> {
    let ink = r#"
I have {five()} eggs.

== function five ==
{false:
    Don't print this
}
five
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    assert_eq!("I have five eggs.\n", story.cont()?);

    Ok(())
}

// TestImplicitInlineGlueB (Tests.cs:1104)
#[test]
fn implicit_inline_glue_b_test() -> Result<(), StoryError> {
    let ink = r#"
A {f():B} 
X

=== function f() ===
{true:
    ~ return false
}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    assert_eq!("A\nX\n", story.continue_maximally()?);

    Ok(())
}

// TestImplicitInlineGlueC (Tests.cs:1120)
#[test]
fn implicit_inline_glue_c_test() -> Result<(), StoryError> {
    let ink = r#"
A
{f():X}
C

=== function f()
{true:
    ~ return false
}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    assert_eq!("A\nC\n", story.continue_maximally()?);

    Ok(())
}
