use core::panic;
use std::{cell::RefCell, error::Error, rc::Rc};

use bladeink::{
    story::{Story, external_functions::ExternalFunction, variable_observer::VariableObserver},
    story_error::StoryError,
    value_type::ValueType,
};
use bladeink_compiler::Compiler;

mod common;

struct ExtFunc1;
struct ExtFunc2;
struct ExtFunc3;
struct ExtFunc4;
struct ExtFunc5;
struct ExtFunc6;

impl ExternalFunction for ExtFunc1 {
    fn call(&mut self, func_name: &str, args: Vec<ValueType>) -> Option<ValueType> {
        println!("Calling {func_name}...");

        let x = args[0].coerce_to_int().unwrap_or_default();
        let y = args[1].coerce_to_int().unwrap_or_default();

        Some(ValueType::Int(x - y))
    }
}

impl ExternalFunction for ExtFunc2 {
    fn call(&mut self, _: &str, _: Vec<ValueType>) -> Option<ValueType> {
        Some(ValueType::new::<&str>("Hello world"))
    }
}

impl ExternalFunction for ExtFunc3 {
    fn call(&mut self, _: &str, args: Vec<ValueType>) -> Option<ValueType> {
        Some(ValueType::Bool(args[0].get::<i32>().unwrap() != 1))
    }
}

impl ExternalFunction for ExtFunc4 {
    fn call(&mut self, _: &str, args: Vec<ValueType>) -> Option<ValueType> {
        Some(ValueType::Bool(!args[0].coerce_to_bool().unwrap()))
    }
}

// ExternalFunction for 3-arg sum: x + y + z (as int)
impl ExternalFunction for ExtFunc5 {
    fn call(&mut self, _: &str, args: Vec<ValueType>) -> Option<ValueType> {
        let x = args[0].coerce_to_int().unwrap_or_default();
        let y = args[1].coerce_to_int().unwrap_or_default();
        let z = args[2].coerce_to_int().unwrap_or_default();
        Some(ValueType::Int(x + y + z))
    }
}

// ExternalFunction for 3-arg sum with explicit coerce: same result
impl ExternalFunction for ExtFunc6 {
    fn call(&mut self, _: &str, args: Vec<ValueType>) -> Option<ValueType> {
        let x = args[0].coerce_to_int().unwrap_or_default();
        let y = args[1].coerce_to_int().unwrap_or_default();
        let z = args[2].coerce_to_int().unwrap_or_default();
        Some(ValueType::Int(x + y + z))
    }
}

#[test]
fn external_function() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/external-function-2-arg.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    story.bind_external_function("externalFunction", Rc::new(RefCell::new(ExtFunc1 {})), true)?;

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("The value is -1.", text[0]);

    Ok(())
}

#[test]
fn visit_count_bug_due_to_nested_containers_test() -> Result<(), StoryError> {
    let ink_source = r#"
- (gather) {gather}
* choice
- {gather}
"#;
    let json_string = Compiler::new().compile(ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    assert_eq!("1\n", story.continue_maximally()?);
    assert_eq!(1, story.get_current_choices().len());
    story.choose_choice_index(0)?;
    assert_eq!("choice\n1\n", story.continue_maximally()?);
    Ok(())
}

#[test]
fn visit_counts_when_choosing_test() -> Result<(), StoryError> {
    let ink_source = r#"
== TestKnot ==
this is a test
+ [Next] -> TestKnot2

== TestKnot2 ==
this is the end
-> END
"#;
    let json_string = Compiler::new().compile(ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!(0, story.get_visit_count_at_path_string("TestKnot")?);
    assert_eq!(0, story.get_visit_count_at_path_string("TestKnot2")?);

    story.choose_path_string("TestKnot", true, None)?;
    assert_eq!(1, story.get_visit_count_at_path_string("TestKnot")?);
    assert_eq!(0, story.get_visit_count_at_path_string("TestKnot2")?);

    story.cont()?;
    assert_eq!(1, story.get_visit_count_at_path_string("TestKnot")?);
    assert_eq!(0, story.get_visit_count_at_path_string("TestKnot2")?);

    story.choose_choice_index(0)?;
    assert_eq!(1, story.get_visit_count_at_path_string("TestKnot")?);
    assert_eq!(0, story.get_visit_count_at_path_string("TestKnot2")?);

    story.cont()?;
    assert_eq!(1, story.get_visit_count_at_path_string("TestKnot")?);
    assert_eq!(1, story.get_visit_count_at_path_string("TestKnot2")?);

    Ok(())
}

#[test]
fn clean_callstack_reset_on_path_choice_test() -> Result<(), StoryError> {
    let ink_source = r#"
{RunAThing()}

== function RunAThing ==
The first line.
The second line.

== SomewhereElse ==
{"somewhere else"}
->END
"#;
    let json_string = Compiler::new().compile(ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    assert_eq!("The first line.\n", story.cont()?);
    story.choose_path_string("SomewhereElse", true, None)?;
    assert_eq!("somewhere else\n", story.continue_maximally()?);
    Ok(())
}

#[test]
fn state_rollback_over_default_choice_test() -> Result<(), StoryError> {
    let ink_source = r#"
<- make_default_choice
Text.

=== make_default_choice
    *   ->
        {5}
        -> END
"#;
    let json_string = Compiler::new().compile(ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    assert_eq!("Text.\n", story.cont()?);
    assert_eq!("5\n", story.cont()?);
    Ok(())
}

#[test]
fn external_function_one_arguments() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/external-function-1-arg.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    story.bind_external_function("externalFunction", Rc::new(RefCell::new(ExtFunc3 {})), true)?;

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("The value is false.", text[0]);

    Ok(())
}

#[test]
fn external_function_coerce_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/external-function-1-arg.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    story.bind_external_function("externalFunction", Rc::new(RefCell::new(ExtFunc4 {})), true)?;

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("The value is false.", text[0]);

    Ok(())
}

#[test]
fn external_function_fallback_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/external-function-2-arg.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    story.set_allow_external_function_fallbacks(true);

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("The value is 7.", text[0]);

    Ok(())
}

struct VObserver {
    expected_value: i32,
}

impl VariableObserver for VObserver {
    fn changed(&mut self, variable_name: &str, new_value: &ValueType) {
        if !"x".eq(variable_name) {
            panic!();
        }

        if let ValueType::Int(v) = new_value {
            assert_eq!(self.expected_value, *v);
        } else {
            panic!();
        }

        self.expected_value = 10;
    }
}

#[test]
fn variable_observers_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/variable-observers.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    let observer = Rc::new(RefCell::new(VObserver { expected_value: 5 }));
    story.observe_variable("x", observer.clone())?;

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!(10, story.get_variable("x").unwrap().get::<i32>().unwrap());

    // Check that the observer's expected_value is now 10
    assert_eq!(observer.borrow().expected_value, 10);

    Ok(())
}

#[test]
fn set_and_get_variable_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/set-get-variables.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(10, story.get_variable("x").unwrap().get::<i32>().unwrap());

    story.set_variable("x", &ValueType::Int(15))?;

    assert_eq!(15, story.get_variable("x").unwrap().get::<i32>().unwrap());

    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("OK", text[0]);

    Ok(())
}

#[test]
fn set_non_existant_variable_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/set-get-variables.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;

    let result = story.set_variable("y", &ValueType::new::<&str>("earth"));
    assert!(result.is_err());

    assert_eq!(10, story.get_variable("x").unwrap().get::<i32>().unwrap());

    story.set_variable("x", &ValueType::Int(15))?;

    assert_eq!(15, story.get_variable("x").unwrap().get::<i32>().unwrap());

    story.choose_choice_index(0)?;

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("OK", text[0]);

    Ok(())
}

#[test]
fn jump_knot_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/jump-knot.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    story.choose_path_string("two", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("Two", text.first().unwrap());

    text.clear();
    story.choose_path_string("three", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("Three", text.first().unwrap());

    text.clear();
    story.choose_path_string("one", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("One", text.first().unwrap());

    text.clear();
    story.choose_path_string("two", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("Two", text.first().unwrap());

    Ok(())
}

#[test]
fn jump_stitch_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/jump-stitch.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    story.choose_path_string("two.sthree", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("Two.3", text.first().unwrap());

    text.clear();
    story.choose_path_string("one.stwo", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("One.2", text.first().unwrap());

    text.clear();
    story.choose_path_string("one.sone", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("One.1", text.first().unwrap());

    text.clear();
    story.choose_path_string("two.stwo", true, None)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!("Two.2", text.first().unwrap());

    Ok(())
}

#[test]
fn read_visit_counts_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/read-visit-counts.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(4, story.get_visit_count_at_path_string("two.s2")?);
    assert_eq!(5, story.get_visit_count_at_path_string("two")?);

    Ok(())
}

#[test]
fn load_save_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/load-save.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!(
        "We arrived into London at 9.45pm exactly.",
        text.first().unwrap()
    );

    // save the game state
    let save_string = story.save_state()?;

    println!("{}", save_string);

    // recreate game and load state
    Story::new(&json_string).unwrap();
    story.load_state(&save_string)?;

    story.choose_choice_index(0)?;

    common::next_all(&mut story, &mut text)?;
    assert_eq!(
        "\"There is not a moment to lose!\" I declared.",
        text.get(1).unwrap()
    );
    assert_eq!(
        "We hurried home to Savile Row as fast as we could.",
        text.get(2).unwrap()
    );

    // check that we are at the end
    assert!(!story.can_continue());
    assert_eq!(0, story.get_current_choices().len());

    Ok(())
}

#[test]
fn external_function_two_arguments_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/external-function-2-arg.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    story.bind_external_function("externalFunction", Rc::new(RefCell::new(ExtFunc1 {})), true)?;

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("The value is -1.", text[0]);

    Ok(())
}

#[test]
fn external_function_two_arguments_coerce_override_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/external-function-2-arg.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    // Uses coerce_to_int explicitly for both args — same result
    story.bind_external_function("externalFunction", Rc::new(RefCell::new(ExtFunc1 {})), true)?;

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("The value is -1.", text[0]);

    Ok(())
}

#[test]
fn external_function_three_arguments_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/external-function-3-arg.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    story.bind_external_function("externalFunction", Rc::new(RefCell::new(ExtFunc5 {})), true)?;

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("The value is 6.", text[0]);

    Ok(())
}

#[test]
fn external_function_three_arguments_coerce_override_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/external-function-3-arg.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    // Uses explicit coerce_to_int for all args — same result
    story.bind_external_function("externalFunction", Rc::new(RefCell::new(ExtFunc6 {})), true)?;

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("The value is 6.", text[0]);

    Ok(())
}
