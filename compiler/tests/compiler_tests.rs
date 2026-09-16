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

fn choice_texts(story: &Story) -> Vec<String> {
    story
        .get_current_choices()
        .iter()
        .map(|c| c.text.clone())
        .collect()
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

#[test]
fn sibling_labeled_gathers_share_the_same_weave_container() {
    let ink = r#"
-> dialogue

=== dialogue ===
Start.
- (options)
+ [First]
+ [Second]
- (response)
Response.
-> options
"#;

    let json = Compiler::new().compile(ink).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();
    let dialogue = value["root"][2]["dialogue"].as_array().unwrap();
    let weave = dialogue[0]
        .as_array()
        .expect("labeled gathers should be wrapped in an anonymous weave container");
    let options = weave
        .iter()
        .find_map(|item| {
            let array = item.as_array()?;
            let terminator = array.last()?.as_object()?;
            (terminator.get("#n").and_then(Value::as_str) == Some("options")).then_some(terminator)
        })
        .expect("missing options gather");
    let weave_named = weave
        .last()
        .and_then(Value::as_object)
        .expect("missing weave named content");

    assert!(
        weave_named.contains_key("response"),
        "response gather should be a sibling of options: {json}"
    );
    assert!(
        !options.contains_key("response"),
        "response gather must not be nested inside options: {json}"
    );
}

#[test]
fn external_as_conditional_test_emits_external_call() {
    let ink = "EXTERNAL has_key()\n{has_key(): Unlocked.|Locked.}\n-> DONE\n";

    let json = Compiler::new().compile(ink).unwrap();

    assert!(
        json.contains(r#"{"x()":"has_key"}"#),
        "conditional test on an EXTERNAL should emit an external call: {json}"
    );
    assert!(
        !json.contains(r#"{"f()":"has_key"}"#),
        "conditional test on an EXTERNAL must not emit an internal call: {json}"
    );
}

#[test]
fn function_as_conditional_test_still_emits_internal_call() {
    let ink = "== function has_key() ==\n~ return true\n\
               === main ===\n{has_key(): Unlocked.|Locked.}\n-> DONE\n";

    let json = Compiler::new().compile(ink).unwrap();

    assert!(
        json.contains(r#"{"f()":"has_key"}"#),
        "conditional test on an ink function should emit an internal call: {json}"
    );
}

#[test]
fn external_conditional_test_selects_the_branch_at_runtime() {
    let ink = "EXTERNAL has_key()\n{has_key(): Unlocked.|Locked.}\n-> DONE\n";
    let json = Compiler::new().compile(ink).unwrap();

    for (value, expected) in [(true, "Unlocked."), (false, "Locked.")] {
        let mut story = Story::new(&json).unwrap();
        story
            .bind_external_function(
                "has_key",
                move |_: &str, _: &[bladeink::value_type::ValueType]| {
                    Ok(Some(bladeink::value_type::ValueType::Bool(value)))
                },
                false,
            )
            .unwrap();
        assert_eq!(story.cont().unwrap().trim(), expected);
    }
}

#[test]
fn external_conditional_test_keeps_gated_and_sibling_choices() {
    let ink = "EXTERNAL show_extra()\n\
               -> main\n\
               === main ===\n\
               <- base_choices\n\
               { show_extra(): <- extra_choices }\n\
               + [Leave]\n    -> DONE\n\
               -> DONE\n\
               == base_choices ==\n+ [Stay]\n    -> DONE\n\
               == extra_choices ==\n+ [Open the safe]\n    -> DONE\n";
    let json = Compiler::new().compile(ink).unwrap();

    let mut story = Story::new(&json).unwrap();
    story
        .bind_external_function(
            "show_extra",
            |_: &str, _: &[bladeink::value_type::ValueType]| {
                Ok(Some(bladeink::value_type::ValueType::Bool(true)))
            },
            false,
        )
        .unwrap();
    while story.can_continue() {
        story.cont().unwrap();
    }

    let labels: Vec<String> = story
        .get_current_choices()
        .iter()
        .map(|choice| choice.text.clone())
        .collect();
    assert_eq!(labels, vec!["Stay", "Open the safe", "Leave"]);
}

#[test]
fn column_zero_choice_body_keeps_siblings() {
    let ink = r#"
-> k.main
== k ==
= main
* (a) [A]
x
-> k.main
* (b) [B]
  y
  -> k.main
+ [C]
  z
  -> DONE
"#;

    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json).unwrap();

    story.continue_maximally().unwrap();
    assert_eq!(vec!["A", "B", "C"], choice_texts(&story));

    story.choose_choice_index(0).unwrap();
    let text = story.continue_maximally().unwrap();
    assert!(text.contains('x'), "got: {text:?}");
    assert_eq!(vec!["B", "C"], choice_texts(&story));
}

#[test]
fn indented_choice_marker_stays_sibling() {
    let ink = r#"
-> k.main
== k ==
= main
* (a) [A]
  x
  -> k.main
  * (b) [B]
  y
  -> k.main
+ [C]
  z
  -> DONE
"#;

    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json).unwrap();

    story.continue_maximally().unwrap();
    assert_eq!(vec!["A", "B", "C"], choice_texts(&story));

    story.choose_choice_index(1).unwrap();
    let text = story.continue_maximally().unwrap();
    assert!(text.contains('y'), "got: {text:?}");
    assert_eq!(vec!["A", "C"], choice_texts(&story));
}

#[test]
fn column_zero_nested_weave_by_marker_count() {
    let ink = r#"
-> k
== k ==
* [A]
* * [Sub1]
s1
* * [Sub2]
s2
- - subs gathered
after
* [B]
b
- done
-> END
"#;

    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json).unwrap();

    story.continue_maximally().unwrap();
    assert_eq!(vec!["A", "B"], choice_texts(&story));

    story.choose_choice_index(0).unwrap();
    story.continue_maximally().unwrap();
    assert_eq!(vec!["Sub1", "Sub2"], choice_texts(&story));

    story.choose_choice_index(1).unwrap();
    let text = story.continue_maximally().unwrap();
    assert!(
        text.contains("s2") && text.contains("subs gathered") && text.contains("after"),
        "got: {text:?}"
    );
    assert!(text.contains("done"), "got: {text:?}");
}

#[test]
fn choice_body_stops_at_conditional_closing_brace() {
    let ink = r#"
-> start
== start ==
{ true:
* [Heads]
Heads it is.
-> END
- else:
* [Tails]
Tails it is.
-> END
}
"#;

    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json).unwrap();

    story.continue_maximally().unwrap();
    assert_eq!(vec!["Heads"], choice_texts(&story));

    story.choose_choice_index(0).unwrap();
    let text = story.continue_maximally().unwrap();
    assert!(text.contains("Heads it is."), "got: {text:?}");
}
