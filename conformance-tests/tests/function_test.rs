use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;

mod common;

#[test]
fn fun_basic_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/func-basic.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value of x is 4.4.", text[0]);

    Ok(())
}

#[test]
fn fun_none_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/func-none.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value of x is 3.8.", text[0]);

    Ok(())
}

#[test]
fn fun_inline_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/func-inline.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value of x is 4.4.", text[0]);

    Ok(())
}

#[test]
fn setvar_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/setvar-func.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value is 6.", text[0]);

    Ok(())
}

#[test]
fn complex_func1_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/complex-func1.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The values are 6 and 10.", text[0]);

    Ok(())
}

#[test]
fn complex_func2_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/complex-func2.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The values are -1 and 0 and 1.", text[0]);

    Ok(())
}

#[test]
fn complex_func3_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/complex-func3.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!(
        "\"I will pay you 120 reales if you get the goods to their destination. The goods will take up 20 cargo spaces.\"",
        text[0]
    );

    Ok(())
}

#[test]
fn rnd() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/function/rnd-func.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(4, text.len());
    assert_eq!("Rolling dice 1: 1.", text[0]);
    assert_eq!("Rolling dice 2: 4.", text[1]);
    assert_eq!("Rolling dice 3: 4.", text[2]);
    assert_eq!("Rolling dice 4: 1.", text[3]);

    Ok(())
}

#[test]
fn evaluating_function_variable_state_bug_test() -> Result<(), StoryError> {
    let ink_source =
        common::get_file_string("inkfiles/function/evaluating-function-variablestate-bug.ink")
            .unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!("Start\n", story.cont()?);
    assert_eq!("In tunnel.\n", story.cont()?);

    let mut output = String::new();
    let result = story.evaluate_function("function_to_evaluate", None, &mut output);

    assert_eq!("RIGHT", result?.unwrap().get::<&str>().unwrap());

    assert_eq!("End\n", story.cont()?);

    Ok(())
}

// TestFactorialByReference (Tests.cs:906)
#[test]
fn factorial_by_reference_test() -> Result<(), StoryError> {
    let ink = r#"
VAR result = 0
~ factorialByRef(result, 5)
{ result }

== function factorialByRef(ref r, n) ==
{ r == 0:
    ~ r = 1
}
{ n > 1:
    ~ r = r * n
    ~ factorialByRef(r, n-1)
}
~ return
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    assert_eq!("120\n", story.continue_maximally()?);

    Ok(())
}

// TestFactorialRecursive (Tests.cs:930)
#[test]
fn factorial_recursive_test() -> Result<(), StoryError> {
    let ink = r#"
{ factorial(5) }

== function factorial(n) ==
{ n == 1:
    ~ return 1
- else:
    ~ return (n * factorial(n-1))
}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    assert_eq!("120\n", story.continue_maximally()?);

    Ok(())
}

// TestFunctionCallRestrictions (Tests.cs:949)
#[test]
fn function_call_restrictions_test() {
    let call_knot_as_function = r#"
~ aKnot()

== function myFunc ==
~ return

== aKnot ==
-> END
"#;
    let err = Compiler::new().compile(call_knot_as_function).unwrap_err();
    assert!(
        err.message().contains("function") || err.message().contains("call"),
        "expected function-call restriction error, got: {}",
        err.message()
    );

    let divert_to_function = r#"
-> myFunc

== function myFunc ==
~ return
"#;
    let err = Compiler::new().compile(divert_to_function).unwrap_err();
    assert!(
        err.message().contains("function") || err.message().contains("divert"),
        "expected divert-to-function restriction error, got: {}",
        err.message()
    );
}
