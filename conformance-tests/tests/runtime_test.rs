use std::{cell::RefCell, error::Error, rc::Rc};

use bladeink::{
    story::{Story, external_functions::ExternalFunction},
    story_error::StoryError,
    value_type::ValueType,
};
use bladeink_compiler::Compiler;

mod common;

struct ExtFunc1;
struct ExtFunc3;
struct ExtFunc4;
struct ExtFunc5;
struct ExtFunc6;
struct MessageRecorder {
    message: Rc<RefCell<Option<String>>>,
}
struct MultiplyFunc;
struct TimesFunc;
struct CallCounter {
    count: Rc<RefCell<i32>>,
}

impl ExternalFunction for ExtFunc1 {
    fn call(
        &mut self,
        func_name: &str,
        args: &[ValueType],
    ) -> bladeink::story::external_functions::ExternalFunctionResult {
        println!("Calling {func_name}...");

        let x = args[0].coerce_to_int().unwrap_or_default();
        let y = args[1].coerce_to_int().unwrap_or_default();

        Ok(Some(ValueType::Int(x - y)))
    }
}

impl ExternalFunction for ExtFunc3 {
    fn call(
        &mut self,
        _: &str,
        args: &[ValueType],
    ) -> bladeink::story::external_functions::ExternalFunctionResult {
        Ok(Some(ValueType::Bool(args[0].get::<i32>().unwrap() != 1)))
    }
}

impl ExternalFunction for ExtFunc4 {
    fn call(
        &mut self,
        _: &str,
        args: &[ValueType],
    ) -> bladeink::story::external_functions::ExternalFunctionResult {
        Ok(Some(ValueType::Bool(!args[0].coerce_to_bool().unwrap())))
    }
}

// ExternalFunction for 3-arg sum: x + y + z (as int)
impl ExternalFunction for ExtFunc5 {
    fn call(
        &mut self,
        _: &str,
        args: &[ValueType],
    ) -> bladeink::story::external_functions::ExternalFunctionResult {
        let x = args[0].coerce_to_int().unwrap_or_default();
        let y = args[1].coerce_to_int().unwrap_or_default();
        let z = args[2].coerce_to_int().unwrap_or_default();
        Ok(Some(ValueType::Int(x + y + z)))
    }
}

// ExternalFunction for 3-arg sum with explicit coerce: same result
impl ExternalFunction for ExtFunc6 {
    fn call(
        &mut self,
        _: &str,
        args: &[ValueType],
    ) -> bladeink::story::external_functions::ExternalFunctionResult {
        let x = args[0].coerce_to_int().unwrap_or_default();
        let y = args[1].coerce_to_int().unwrap_or_default();
        let z = args[2].coerce_to_int().unwrap_or_default();
        Ok(Some(ValueType::Int(x + y + z)))
    }
}

impl ExternalFunction for MessageRecorder {
    fn call(
        &mut self,
        _: &str,
        args: &[ValueType],
    ) -> bladeink::story::external_functions::ExternalFunctionResult {
        *self.message.borrow_mut() = Some(format!(
            "MESSAGE: {}",
            args[0].coerce_to_string().unwrap_or_default()
        ));
        Ok(None)
    }
}

impl ExternalFunction for MultiplyFunc {
    fn call(
        &mut self,
        _: &str,
        args: &[ValueType],
    ) -> bladeink::story::external_functions::ExternalFunctionResult {
        let x = args[0].coerce_to_float().unwrap_or_default();
        let y = args[1].coerce_to_int().unwrap_or_default() as f32;
        Ok(Some(ValueType::Float(x * y)))
    }
}

impl ExternalFunction for TimesFunc {
    fn call(
        &mut self,
        _: &str,
        args: &[ValueType],
    ) -> bladeink::story::external_functions::ExternalFunctionResult {
        let times = args[0].coerce_to_int().unwrap_or_default();
        let text = args[1].coerce_to_string().unwrap_or_default();
        let mut result = String::new();
        for _ in 0..times {
            result.push_str(&text);
        }
        Ok(Some(ValueType::new(result.as_str())))
    }
}

impl ExternalFunction for CallCounter {
    fn call(
        &mut self,
        _: &str,
        _: &[ValueType],
    ) -> bladeink::story::external_functions::ExternalFunctionResult {
        *self.count.borrow_mut() += 1;
        Ok(None)
    }
}

#[test]
fn external_function() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/external-function-2-arg.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    story.bind_external_function_handler("externalFunction", ExtFunc1, true)?;

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

// TestExternalBinding (Tests.cs:823)
#[test]
fn external_binding_test() -> Result<(), Box<dyn Error>> {
    let ink = r#"
EXTERNAL message(x)
EXTERNAL multiply(x,y)
EXTERNAL times(i,str)
~ message("hello world")
{multiply(5.0, 3)}
{times(3, "knock ")}
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    let message = Rc::new(RefCell::new(None));

    story.bind_external_function_handler(
        "message",
        MessageRecorder {
            message: message.clone(),
        },
        true,
    )?;
    story.bind_external_function_handler("multiply", MultiplyFunc, true)?;
    story.bind_external_function_handler("times", TimesFunc, true)?;

    assert_eq!("15\n", story.cont()?);
    assert_eq!("knock knock knock\n", story.cont()?);
    assert_eq!(
        Some("MESSAGE: hello world".to_owned()),
        message.borrow().clone()
    );

    Ok(())
}

// TestLookupSafeOrNot (Tests.cs:863)
#[test]
fn lookup_safe_or_not_test() -> Result<(), Box<dyn Error>> {
    let ink = r#"
EXTERNAL myAction()

One
~ myAction()
Two
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    let safe_count = Rc::new(RefCell::new(0));

    story.bind_external_function_handler(
        "myAction",
        CallCounter {
            count: safe_count.clone(),
        },
        true,
    )?;
    story.continue_maximally()?;
    assert_eq!(2, *safe_count.borrow());

    story.reset_state()?;
    story.unbind_external_function("myAction")?;

    let unsafe_count = Rc::new(RefCell::new(0));
    story.bind_external_function_handler(
        "myAction",
        CallCounter {
            count: unsafe_count.clone(),
        },
        false,
    )?;
    story.continue_maximally()?;
    assert_eq!(1, *unsafe_count.borrow());

    let glue_ink = r#"
EXTERNAL myAction()

One 
~ myAction()
<> Two
"#;
    let json = Compiler::new().compile(glue_ink).unwrap();
    let mut story = Story::new(&json)?;
    story.bind_external_function_handler(
        "myAction",
        CallCounter {
            count: Rc::new(RefCell::new(0)),
        },
        false,
    )?;
    assert_eq!("One\nTwo\n", story.continue_maximally()?);

    Ok(())
}

#[test]
fn external_function_one_arguments() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/external-function-1-arg.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    story.bind_external_function_handler("externalFunction", ExtFunc3, true)?;

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

    story.bind_external_function_handler("externalFunction", ExtFunc4, true)?;

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

#[test]
fn variable_observers_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/variable-observers.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    let expected_value = Rc::new(RefCell::new(5));
    let observed_value = expected_value.clone();
    story.observe_variable("x", move |variable_name, new_value| {
        assert_eq!("x", variable_name);
        assert_eq!(Some(*observed_value.borrow()), new_value.get::<i32>());
        *observed_value.borrow_mut() = 10;
        Ok(())
    })?;

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(0)?;
    common::next_all(&mut story, &mut text)?;
    assert_eq!(10, story.get_variable("x").unwrap().get::<i32>().unwrap());

    assert_eq!(*expected_value.borrow(), 10);

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
fn external_function_closure_and_error_test() -> Result<(), Box<dyn Error>> {
    let json = Compiler::new()
        .compile("EXTERNAL increment(x)\n{increment(4)}")
        .unwrap();
    let mut story = Story::new(&json)?;
    let calls = Rc::new(RefCell::new(0));
    let callback_calls = calls.clone();
    story.bind_external_function(
        "increment",
        move |_, args| {
            *callback_calls.borrow_mut() += 1;
            Ok(Some(ValueType::Int(
                args[0].coerce_to_int().map_err(|error| {
                    bladeink::story::external_functions::ExternalFunctionError::new(
                        error.to_string(),
                    )
                })? + 1,
            )))
        },
        true,
    )?;

    assert_eq!("5\n", story.cont()?);
    assert_eq!(1, *calls.borrow());

    let json = Compiler::new()
        .compile("EXTERNAL fail()\n~ fail()")
        .unwrap();
    let mut story = Story::new(&json)?;
    story.bind_external_function(
        "fail",
        |_, _| {
            Err(
                bladeink::story::external_functions::ExternalFunctionError::new(
                    "game service unavailable",
                ),
            )
        },
        true,
    )?;

    assert!(matches!(
        story.cont(),
        Err(StoryError::ExternalFunctionFailed { function_name, error })
            if function_name == "fail" && error.to_string() == "game service unavailable"
    ));
    Ok(())
}

#[test]
fn variable_observer_handles_and_errors_test() -> Result<(), Box<dyn Error>> {
    let json = Compiler::new()
        .compile("VAR x = 1\nVAR y = 2\n-> DONE")
        .unwrap();
    let mut story = Story::new(&json)?;
    let notifications = Rc::new(RefCell::new(Vec::new()));
    let callback_notifications = notifications.clone();
    let handle = story.observe_variables(&["x", "y"], move |name, value| {
        callback_notifications
            .borrow_mut()
            .push((name.to_owned(), value.get::<i32>().unwrap()));
        Ok(())
    })?;

    story.set_variable("x", &ValueType::Int(1))?;
    story.set_variable("x", &ValueType::Int(3))?;
    story.set_variable("y", &ValueType::Int(4))?;
    assert_eq!(
        vec![("x".to_owned(), 3), ("y".to_owned(), 4)],
        *notifications.borrow()
    );
    assert!(story.remove_variable_observer(handle)?);
    assert!(!story.remove_variable_observer(handle)?);
    story.set_variable("x", &ValueType::Int(5))?;
    assert_eq!(2, notifications.borrow().len());

    let notification_count = Rc::new(RefCell::new(0));
    let callback_count = notification_count.clone();
    assert!(
        story
            .observe_variables(&["x", "missing"], move |_, _| {
                *callback_count.borrow_mut() += 1;
                Ok(())
            })
            .is_err()
    );
    story.set_variable("x", &ValueType::Int(6))?;
    assert_eq!(0, *notification_count.borrow());

    story.observe_variable("x", |_, _| {
        Err(bladeink::story::variable_observer::VariableObserverError::new("save failed"))
    })?;
    assert!(matches!(
        story.set_variable("x", &ValueType::Int(7)),
        Err(StoryError::VariableObserverFailed { variable_name, error })
            if variable_name == "x" && error.to_string() == "save failed"
    ));
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

// TestReadCountDotSeparatedPath (Tests.cs:1689)
#[test]
fn read_count_dot_separated_path_test() -> Result<(), StoryError> {
    let ink = r#"
-> hi ->
-> hi ->
-> hi ->

{ hi.stitch_to_count }

== hi ==
= stitch_to_count
hi
->->
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    assert_eq!("hi\nhi\nhi\n3\n", story.continue_maximally()?);

    Ok(())
}

#[test]
fn external_function_two_arguments_test() -> Result<(), Box<dyn Error>> {
    let ink_source = common::get_file_string("inkfiles/runtime/external-function-2-arg.ink")?;
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    let mut text: Vec<String> = Vec::new();

    story.bind_external_function_handler("externalFunction", ExtFunc1, true)?;

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
    story.bind_external_function_handler("externalFunction", ExtFunc1, true)?;

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

    story.bind_external_function_handler("externalFunction", ExtFunc5, true)?;

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
    story.bind_external_function_handler("externalFunction", ExtFunc6, true)?;

    common::next_all(&mut story, &mut text)?;
    assert_eq!(1, text.len());
    assert_eq!("The value is 6.", text[0]);

    Ok(())
}
