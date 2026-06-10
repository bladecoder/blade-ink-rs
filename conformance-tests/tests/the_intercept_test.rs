use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;
use rand::{RngExt, SeedableRng, rngs::StdRng};

mod common;

#[derive(Debug, PartialEq)]
struct StorySnapshot {
    text: String,
    can_continue: bool,
    choices: Vec<String>,
    errors: Vec<String>,
    warnings: Vec<String>,
}

fn snapshot(story: &Story, text: String) -> StorySnapshot {
    StorySnapshot {
        text,
        can_continue: story.can_continue(),
        choices: story
            .get_current_choices()
            .iter()
            .map(|choice| choice.text.clone())
            .collect(),
        errors: story.get_current_errors().to_vec(),
        warnings: story.get_current_warnings().to_vec(),
    }
}

fn pointer_context(story: &Story) -> String {
    let hierarchy = story.build_string_of_hierarchy();
    let lines: Vec<_> = hierarchy.lines().collect();
    let Some(pointer) = lines.iter().position(|line| line.contains("<---")) else {
        return format!("no current pointer; path: {:?}", story.get_current_path());
    };
    lines[pointer.saturating_sub(4)..(pointer + 6).min(lines.len())].join("\n")
}

#[test]
fn the_intercept_compiles_test() {
    let ink_source = common::get_file_string("inkfiles/TheIntercept.ink").unwrap();
    Compiler::new().compile(&ink_source).unwrap();
}

#[test]
fn the_intercept_runtime_choices_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/TheIntercept.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    story.continue_maximally()?;

    let first_choices: Vec<String> = story
        .get_current_choices()
        .iter()
        .map(|choice| choice.text.clone())
        .collect();
    assert_eq!(vec!["Hut 14".to_string()], first_choices);

    story.choose_choice_index(0)?;
    story.continue_maximally()?;

    let second_choices: Vec<String> = story
        .get_current_choices()
        .iter()
        .map(|choice| choice.text.clone())
        .collect();
    assert_eq!(3, second_choices.len());
    assert_eq!("Think", second_choices[0]);
    assert_eq!("Plan", second_choices[1]);
    assert_eq!("Wait", second_choices[2]);

    story.choose_choice_index(1)?;
    story.continue_maximally()?;

    let third_choices: Vec<String> = story
        .get_current_choices()
        .iter()
        .map(|choice| choice.text.clone())
        .collect();
    assert_eq!(3, third_choices.len());
    assert!(
        third_choices[0].starts_with("Co") && third_choices[0].contains("operate"),
        "unexpected first choice text: {}",
        third_choices[0]
    );
    assert_eq!("Dissemble", third_choices[1]);
    assert_eq!("Divert", third_choices[2]);

    Ok(())
}

#[test]
fn the_intercept_random_playthrough_test() {
    let ink_source = common::get_file_string("inkfiles/TheIntercept.ink").unwrap();
    let compiled_json = Compiler::new().compile(&ink_source).unwrap();
    let reference_json = common::get_json_string("inkfiles/TheIntercept.ink.json").unwrap();

    let mut reference_story = Story::new(&reference_json).unwrap();
    let mut compiled_story = Story::new(&compiled_json).unwrap();
    let mut rng = StdRng::seed_from_u64(0);
    let mut choice_history = Vec::new();
    let mut choice_text_history = Vec::new();

    for step in 0..10_000 {
        let reference_text = if reference_story.can_continue() {
            reference_story.cont().unwrap()
        } else {
            String::new()
        };
        let compiled_text = if compiled_story.can_continue() {
            compiled_story.cont().unwrap()
        } else {
            String::new()
        };
        let reference = snapshot(&reference_story, reference_text);
        let compiled = snapshot(&compiled_story, compiled_text);

        assert_eq!(
            reference,
            compiled,
            "stories diverged at step {step} after choices {choice_history:?} \
             ({choice_text_history:?})\nreference pointer:\n{}\ncompiled pointer:\n{}",
            pointer_context(&reference_story),
            pointer_context(&compiled_story)
        );
        assert!(
            reference.errors.is_empty(),
            "runtime errors at step {step} after choices {choice_history:?}: {:?}",
            reference.errors
        );

        if reference.can_continue {
            continue;
        }

        if reference.choices.is_empty() {
            return;
        }

        let choice_index = rng.random_range(0..reference.choices.len());
        choice_history.push(choice_index);
        choice_text_history.push(reference.choices[choice_index].clone());

        reference_story.choose_choice_index(choice_index).unwrap();
        compiled_story.choose_choice_index(choice_index).unwrap();
    }

    panic!("story did not end after 10,000 steps; choices: {choice_history:?}");
}
