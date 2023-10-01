use bladeink::{story::Story, story_error::StoryError};

mod common;

#[test]
fn fun_basic_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/function/func-basic.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value of x is 4.4.", text[0]);

    Ok(())
}

#[test]
fn fun_none_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/function/func-none.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value of x is 3.8.", text[0]);

    Ok(())
}

#[test]
fn fun_inline_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/function/func-inline.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value of x is 4.4.", text[0]);

    Ok(())
}

#[test]
fn setvar_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/function/setvar-func.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value is 6.", text[0]);

    Ok(())
}

#[test]
fn complex_func1_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/function/complex-func1.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The values are 6 and 10.", text[0]);

    Ok(())
}

#[test]
fn complex_func2_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/function/complex-func2.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The values are -1 and 0 and 1.", text[0]);

    Ok(())
}

#[test]
fn complex_func3_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/function/complex-func3.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("\"I will pay you 120 reales if you get the goods to their destination. The goods will take up 20 cargo spaces.\"",
    text[0]);

    Ok(())
}

#[test]
fn rnd() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/function/rnd-func.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
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
fn evaluating_function_variable_state_bug_test() -> Result<(), StoryError>  {
    let json_string =
        common::get_json_string("examples/inkfiles/function/evaluating-function-variablestate-bug.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    assert_eq!("Start\n", story.cont()?);
    assert_eq!("In tunnel.\n", story.cont()?);

    let mut output = String::new();
    let result = story.evaluate_function("function_to_evaluate", None, &mut output);

    assert_eq!("RIGHT", result?.unwrap());
    assert_eq!("End\n", story.cont()?);

    Ok(())
}