#![cfg(feature = "binary-image")]

use std::{fs, path::Path};

use bladeink::{image::compile_json_to_image, story::Story, value_type::ValueType};
use bladeink_compiler::Compiler;
use rand::{RngExt, SeedableRng, rngs::StdRng};

fn pair(json: &str, seed: i32) -> (Story, Story) {
    let bytes = compile_json_to_image(json.as_bytes()).unwrap();
    let image = Box::leak(bytes.into_boxed_slice());
    (
        Story::new_with_seed(json, seed).unwrap(),
        Story::new_from_image_with_seed(image, seed).unwrap(),
    )
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("inkfiles")
        .join(name);
    fs::read_to_string(path).unwrap()
}

fn compiled_fixtures() -> Vec<std::path::PathBuf> {
    fn visit(dir: &Path, paths: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, paths);
            } else if path.to_string_lossy().ends_with(".ink.json") {
                paths.push(path);
            }
        }
    }
    let mut paths = Vec::new();
    visit(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("inkfiles"),
        &mut paths,
    );
    paths.sort();
    paths
}

fn choices(story: &Story) -> Vec<(String, Vec<String>)> {
    story
        .get_current_choices()
        .iter()
        .map(|choice| (choice.text.clone(), choice.tags.clone()))
        .collect()
}

fn compare(left: &Story, right: &Story, context: &str) {
    assert_eq!(left.can_continue(), right.can_continue(), "{context}");
    assert_eq!(
        left.get_current_text().unwrap(),
        right.get_current_text().unwrap(),
        "{context}"
    );
    assert_eq!(
        left.get_current_tags().unwrap(),
        right.get_current_tags().unwrap(),
        "{context}"
    );
    assert_eq!(choices(left), choices(right), "{context}");
    assert_eq!(
        left.get_current_path(),
        right.get_current_path(),
        "{context}"
    );
    assert_eq!(
        left.get_current_errors(),
        right.get_current_errors(),
        "{context}"
    );
    assert_eq!(
        left.get_current_warnings(),
        right.get_current_warnings(),
        "{context}"
    );
}

fn play(json: &str, seed: i32, max_steps: usize, require_end: bool, variables: &[&str]) -> usize {
    let (mut left, mut right) = pair(json, seed);
    let mut rng = StdRng::seed_from_u64(0);
    let mut choices_made = 0;
    let mut cross_loaded = false;
    for step in 0..max_steps {
        let context = format!("step {step}, choices {choices_made}");
        compare(&left, &right, &context);
        for name in variables {
            assert!(
                left.get_variable(name) == right.get_variable(name),
                "{context}: variable {name}"
            );
        }
        if left.can_continue() {
            assert_eq!(left.cont().unwrap(), right.cont().unwrap(), "{context}");
            compare(&left, &right, &context);
        } else if !choices(&left).is_empty() {
            if !cross_loaded {
                let json_state = left.save_state().unwrap();
                let image_state = right.save_state().unwrap();
                let (mut restored_json, mut restored_image) = pair(json, seed);
                restored_json.load_state(&image_state).unwrap();
                restored_image.load_state(&json_state).unwrap();
                compare(&restored_json, &restored_image, &context);
                left = restored_json;
                right = restored_image;
                cross_loaded = true;
            }
            let index = rng.random_range(0..choices(&left).len());
            left.choose_choice_index(index).unwrap();
            right.choose_choice_index(index).unwrap();
            choices_made += 1;
        } else {
            return choices_made;
        }
    }
    assert!(
        !require_end,
        "story did not end within {max_steps} steps after {choices_made} choices"
    );
    choices_made
}

#[test]
fn every_compiled_fixture_opens_as_an_image() {
    let paths = compiled_fixtures();
    assert_eq!(121, paths.len());
    for path in paths {
        let json = fs::read_to_string(&path).unwrap();
        let bytes = compile_json_to_image(json.as_bytes()).unwrap();
        let image = Box::leak(bytes.into_boxed_slice());
        Story::new_from_image_validated_with_seed(image, 1)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}

#[test]
fn compiled_corpus_observable_execution_matches() {
    let paths = compiled_fixtures();
    assert_eq!(121, paths.len());
    for path in paths {
        let json = fs::read_to_string(&path).unwrap();
        let (mut left, mut right) = pair(&json, 17);
        let mut rng = StdRng::seed_from_u64(17);
        for step in 0..200 {
            let context = format!("{} at step {step}", path.display());
            compare(&left, &right, &context);
            if left.can_continue() {
                let left_result = left.cont();
                let right_result = right.cont();
                match (left_result, right_result) {
                    (Ok(left_text), Ok(right_text)) => {
                        assert_eq!(left_text, right_text, "{context}");
                    }
                    (Err(left_error), Err(right_error)) => {
                        assert_eq!(left_error.to_string(), right_error.to_string(), "{context}");
                        break;
                    }
                    (left_result, right_result) => {
                        panic!(
                            "{context}: mismatched continuation: {left_result:?} vs {right_result:?}"
                        );
                    }
                }
            } else {
                let choices = choices(&left);
                if choices.is_empty() {
                    break;
                }
                let index = rng.random_range(0..choices.len());
                left.choose_choice_index(index).unwrap();
                right.choose_choice_index(index).unwrap();
            }
        }
    }
}

#[test]
fn intercept_complete_playthrough_matches() {
    let choices_made = play(
        &fixture("TheIntercept.ink.json"),
        123,
        10_000,
        true,
        &["forceful", "evasive", "teacup", "gotcomponent"],
    );
    assert!(choices_made > 5);
}

#[test]
fn relative_paths_variable_diverts_and_dynamic_content_match() {
    for name in [
        "divert/complex-branching.ink.json",
        "variable/var-divert.ink.json",
        "variabletext/sequence.ink.json",
        "choices/nested-choice.ink.json",
    ] {
        let variables: &[&str] = if name == "variable/var-divert.ink.json" {
            &["current_epilogue"]
        } else {
            &[]
        };
        play(
            &fixture(name),
            27,
            2_000,
            name != "variabletext/sequence.ink.json",
            variables,
        );
    }

    for ink in [
        "-> car\n== car ==\n= pickup_flask\nFlask found.\n-> END\n",
        "-> first\n== knot ==\n= first\nFirst.\n-> second\n= second\nSecond.\n-> END\n",
        "VAR x = true\n-> knot\n== knot ==\n{x: -> go}\nDone.\n-> END\n= go\nGoing.\n-> END\n",
    ] {
        let json = Compiler::new().compile(ink).unwrap();
        play(&json, 27, 100, true, &[]);
    }

    play(
        &fixture("tags/tagsInChoiceDynamic.ink.json"),
        27,
        100,
        true,
        &[],
    );
    play(
        &fixture("tags/tagsDynamicContent.ink.json"),
        27,
        100,
        true,
        &[],
    );
}

#[test]
fn tags_variables_lists_functions_flows_and_paths_match() {
    let (mut left, mut right) = pair(&fixture("tags/tags.ink.json"), 9);
    assert_eq!(
        left.get_global_tags().unwrap(),
        right.get_global_tags().unwrap()
    );
    for path in ["knot", "knot.stitch"] {
        assert_eq!(
            left.tags_for_content_at_path(path).unwrap(),
            right.tags_for_content_at_path(path).unwrap()
        );
    }
    compare(&left, &right, "tags initial");
    assert_eq!(left.cont().unwrap(), right.cont().unwrap());
    compare(&left, &right, "tags continued");
    left.choose_path_string("knot", false, None).unwrap();
    right.choose_path_string("knot", false, None).unwrap();
    assert_eq!(left.cont().unwrap(), right.cont().unwrap());
    compare(&left, &right, "tags path");

    let (mut left, mut right) = pair(&fixture("runtime/set-get-variables.ink.json"), 9);
    for name in ["x", "y"] {
        assert!(left.get_variable(name) == right.get_variable(name));
    }
    left.set_variable("x", &ValueType::Int(42)).unwrap();
    right.set_variable("x", &ValueType::Int(42)).unwrap();
    assert!(left.get_variable("x") == right.get_variable("x"));

    let (left, right) = pair(&fixture("lists/basic-operations.ink.json"), 9);
    assert_eq!(
        left.list_from_origin("list").unwrap().get_origin_names(),
        right.list_from_origin("list").unwrap().get_origin_names()
    );
    assert_eq!(
        left.list_from_item("list.a").unwrap().items,
        right.list_from_item("list.a").unwrap().items
    );

    let (mut left, mut right) = pair(
        &fixture("function/evaluating-function-variablestate-bug.ink.json"),
        9,
    );
    for _ in 0..2 {
        assert_eq!(left.cont().unwrap(), right.cont().unwrap());
    }
    let (mut left_text, mut right_text) = (String::new(), String::new());
    let left_value = left
        .evaluate_function("function_to_evaluate", None, &mut left_text)
        .unwrap();
    let right_value = right
        .evaluate_function("function_to_evaluate", None, &mut right_text)
        .unwrap();
    assert!(left_value == right_value);
    assert_eq!(left_text, right_text);
    compare(&left, &right, "function");

    let (mut left, mut right) = pair(&fixture("runtime/multiflow-basics.ink.json"), 9);
    for (flow, path) in [
        ("First", "knot1"),
        ("Second", "knot2"),
        ("First", ""),
        ("Second", ""),
    ] {
        left.switch_flow(flow).unwrap();
        right.switch_flow(flow).unwrap();
        if !path.is_empty() {
            left.choose_path_string(path, true, None).unwrap();
            right.choose_path_string(path, true, None).unwrap();
        }
        compare(&left, &right, flow);
        assert_eq!(left.cont().unwrap(), right.cont().unwrap(), "{flow}");
    }
}

#[test]
fn save_states_load_across_backends() {
    let json = fixture("runtime/multiflow-saveloadthreads.ink.json");
    let (mut left, mut right) = pair(&json, 9);
    assert_eq!(left.cont().unwrap(), right.cont().unwrap());
    for (flow, path) in [("Blue Flow", "blue"), ("Red Flow", "red")] {
        left.switch_flow(flow).unwrap();
        right.switch_flow(flow).unwrap();
        left.choose_path_string(path, true, None).unwrap();
        right.choose_path_string(path, true, None).unwrap();
        assert_eq!(left.cont().unwrap(), right.cont().unwrap());
    }
    compare(&left, &right, "before save");
    let json_state = left.save_state().unwrap();
    let image_state = right.save_state().unwrap();
    let (mut restored_json, mut restored_image) = pair(&json, 9);
    restored_json.load_state(&image_state).unwrap();
    restored_image.load_state(&json_state).unwrap();
    compare(&restored_json, &restored_image, "after cross load");
    for choice in [0, 1] {
        restored_json.load_state(&image_state).unwrap();
        restored_image.load_state(&json_state).unwrap();
        restored_json.choose_choice_index(choice).unwrap();
        restored_image.choose_choice_index(choice).unwrap();
        assert_eq!(
            restored_json.continue_maximally().unwrap(),
            restored_image.continue_maximally().unwrap()
        );
        compare(&restored_json, &restored_image, "after choice");
    }
}
