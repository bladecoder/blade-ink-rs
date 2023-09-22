use std::{error::Error, path::Path, fs};

use bladeink::story::Story;
use rand::Rng;

pub fn next_all(story: &mut Story, text: &mut Vec<String>) -> Result<(), String> {
    while story.can_continue() {
        let line = story.cont()?;
        print!("{line}");

        if !line.trim().is_empty() {
            text.push(line.trim().to_string());
        }
    }

    if story.has_error() {
        return Err(join_text(&story.get_current_errors()));
    }

    Ok(())
}

pub fn join_text(text: &Vec<String>) -> String {
    let mut sb = String::new();

    for s in text {
        sb.push_str(s);
    }

    sb
}


pub fn run_story(
    filename: &str,
    choice_list: Option<Vec<usize>>,
    errors: &mut Vec<String>,
) -> Result<Vec<String>, String> {
    // 1) Load story
    let json = get_json_string(filename).unwrap();

    let mut story = Story::new(&json)?;

    let mut text = Vec::new();

    let mut choice_list_index = 0;

    let mut rng = rand::thread_rng();

    while story.can_continue() || !story.get_current_choices().is_empty() {

        println!("{}", story.build_string_of_hierarchy());

        // 2) Game content, line by line
        while story.can_continue() {
            let line = story.cont()?;
            print!("{}", line);
            text.push(line);
        }

        if story.has_error() {
            for error_msg in story.get_current_errors() {
                println!("{}", error_msg);
                errors.push(error_msg.to_string());
            }
        }

        // 3) Display story.current_choices list, allow player to choose one
        let current_choices = story.get_current_choices();
        if !current_choices.is_empty() {
            let len = current_choices.len();

            for choice in current_choices {
                println!("{}", choice.text);
                text.push(format!("{}\n", choice.text));
            }

            if let Some(choice_list) = &choice_list {
                if choice_list_index < choice_list.len() {
                    story.choose_choice_index(choice_list[choice_list_index]);
                    choice_list_index += 1;
                } else {
                    let random_choice_index = rng.gen_range(0..len);
                    story.choose_choice_index(random_choice_index);
                }
            } else {
                let random_choice_index = rng.gen_range(0..len);
                story.choose_choice_index(random_choice_index);
            }
        }
    }

    Ok(text)
}

pub fn get_json_string(filename: &str) -> Result<String, Box<dyn Error>> {
    let path = Path::new(filename);
    let json = fs::read_to_string(path)?;
    Ok(json)
}

pub fn is_ended(story: &Story) -> bool {
    return !story.can_continue() && story.get_current_choices().is_empty();
}
