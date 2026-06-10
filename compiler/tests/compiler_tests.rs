use bladeink::story::Story;
use serde_json::Value;

use bladeink_compiler::{Compiler, CompilerError, CompilerOptions};

fn json_has_assignment_token(value: &Value, key: &str, var_name: &str) -> bool {
    match value {
        Value::Object(map) => {
            map.get(key).and_then(Value::as_str) == Some(var_name)
                && map.get("re").and_then(Value::as_bool) == Some(true)
                || map
                    .values()
                    .any(|child| json_has_assignment_token(child, key, var_name))
        }
        Value::Array(items) => items
            .iter()
            .any(|child| json_has_assignment_token(child, key, var_name)),
        _ => false,
    }
}

#[test]
fn error_includes_line_number() {
    // VAR with a bad assignment — error should reference line 3
    let source = "Hello.\nWorld.\nVAR x ==\n";
    let err = Compiler::new().compile(source).unwrap_err();
    let display = err.to_string();
    assert!(
        display.contains("line 3") || display.contains(":3:"),
        "expected line 3 in error, got: {display}"
    );
}

#[test]
fn error_includes_filename_when_set() {
    let source = "VAR x ==\n";
    let options = CompilerOptions {
        source_filename: Some("story.ink".to_owned()),
        ..Default::default()
    };
    let err = Compiler::with_options(options).compile(source).unwrap_err();
    let display = err.to_string();
    assert!(
        display.starts_with("story.ink"),
        "expected filename in error, got: {display}"
    );
}

#[test]
fn error_from_included_file_shows_included_filename() {
    // The main file includes "sub.ink" which has a bad divert on its line 2.
    // The error should report "sub.ink:2", not the main file.
    let main_source = "Hello.\nINCLUDE sub.ink\n";
    let sub_source = "Good line.\n->\n";

    let options = CompilerOptions {
        source_filename: Some("main.ink".to_owned()),
        ..Default::default()
    };
    let err = Compiler::with_options(options)
        .compile_with_file_handler(main_source, |name| {
            if name == "sub.ink" {
                Ok(sub_source.to_owned())
            } else {
                Err(CompilerError::invalid_source(format!(
                    "file not found: {name}"
                )))
            }
        })
        .unwrap_err();
    let display = err.to_string();
    assert!(
        display.contains("sub.ink"),
        "expected 'sub.ink' in error, got: {display}"
    );
    assert!(
        display.contains(":2:") || display.contains("line 2"),
        "expected line 2 in error, got: {display}"
    );
}

#[test]
fn mixed_tabs_and_spaces_keep_choice_body_scope() {
    let ink = r#"
-> start

== start ==
	- (opts)
 		* [Think]
 			Thinking.
			-> opts
 		* [Wait]
	- -> END
"#;

    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json).unwrap();

    story.continue_maximally().unwrap();
    assert_eq!(2, story.get_current_choices().len());

    story.choose_choice_index(0).unwrap();
    let text = story.continue_maximally().unwrap();
    assert!(text.contains("Thinking."), "got: {text:?}");

    let choices = story.get_current_choices();
    assert_eq!(1, choices.len());
    assert_eq!("Wait", choices[0].text);
}

#[test]
fn nested_anonymous_gather_divert_to_stitch_test() {
    let ink = r#"
-> start

== start ==
- Intro.
    * Hut 14[]. Entered.
    More intro.
    End intro.

- (opts)
    {|Idle.|}
    * [Think]
        Thinking.
        -> opts
    * [Plan]
        Planning.
        * * [Subplan]
            Subplanning.
    * [Wait]
- -> waited

= waited
Done.
-> END
"#;

    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json).unwrap();

    story.continue_maximally().unwrap();
    story.choose_choice_index(0).unwrap();
    story.continue_maximally().unwrap();

    let choices = story.get_current_choices();
    assert_eq!(3, choices.len());
    assert_eq!("Wait", choices[2].text);

    story.choose_choice_index(2).unwrap();
    assert_eq!("Done.\n", story.continue_maximally().unwrap());
}

#[test]
fn condition_resolves_nested_choice_label_test() {
    let ink = r#"
-> start

== start ==
* [Plan]
    * * (delay) [Delay]
        Delayed.
        -> END
* [Wait]
- -> waited

= waited
* {not start.delay} [Available]
    Available.
    -> END
    * [Fallback]
    Fallback.
    -> END
"#;

    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json).unwrap();

    story.continue_maximally().unwrap();
    story.choose_choice_index(1).unwrap();
    story.continue_maximally().unwrap();

    let choices = story.get_current_choices();
    assert_eq!(2, choices.len());
    assert_eq!("Available", choices[0].text);
    assert_eq!("Fallback", choices[1].text);
}

#[test]
fn choice_body_can_start_with_nested_labeled_gather_test() {
    let ink = r#"
VAR teacup = false

-> start

== start ==
* [Enter]
- Middle.
    * [Proceed]
- (silence) Silence.
- (drinkit) Prompt.
    * {teacup} [Drink] -> drinkfromcup
    * {teacup} [Put the cup down]
        Put down.
        ~ teacup = false
        -> whatsinit
    * {not teacup} [Take the cup]
        - - (drinkfromcup) Took the cup.
            ~ teacup = true
    * {not teacup} [Don't take it]
        Refused.
        - - (whatsinit) Why?
- After.
    * (target) [Direct]
        Target.
        -> END
    * [Alias] -> target
"#;

    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json).unwrap();

    story.continue_maximally().unwrap();
    story.choose_choice_index(0).unwrap();
    assert_eq!("Middle.\n", story.continue_maximally().unwrap());
    story.choose_choice_index(0).unwrap();
    assert_eq!("Silence.\nPrompt.\n", story.continue_maximally().unwrap());
    let choices = story.get_current_choices();
    assert_eq!(2, choices.len());
    assert_eq!("Take the cup", choices[0].text);
    assert_eq!("Don't take it", choices[1].text);

    story.choose_choice_index(0).unwrap();
    assert_eq!(
        "Took the cup.\nAfter.\n",
        story.continue_maximally().unwrap()
    );
    let choices = story.get_current_choices();
    assert_eq!(2, choices.len());
    assert_eq!("Direct", choices[0].text);
    assert_eq!("Alias", choices[1].text);

    story.choose_choice_index(1).unwrap();
    assert_eq!("\nTarget.\n", story.continue_maximally().unwrap());
}

#[test]
fn ref_parameter_assignment_uses_temp_frame() {
    let ink = r#"
=== function lower(ref x)
    ~ x = x - 1
"#;

    let json = Compiler::new().compile(ink).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();

    assert!(
        json_has_assignment_token(&value, "temp=", "x"),
        "expected ref parameter assignment to emit temp= with re:true, got: {json}"
    );
    assert!(
        !json_has_assignment_token(&value, "VAR=", "x"),
        "ref parameter assignment should not emit VAR= with re:true, got: {json}"
    );
}
