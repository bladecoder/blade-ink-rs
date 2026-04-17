use bladeink::story::Story;
use bladeink_compiler::Compiler;
use std::fs;

fn main() {
    let ink =
        fs::read_to_string("conformance-tests/inkfiles/runtime/multiflow-saveloadthreads.ink")
            .unwrap();
    let json = Compiler::new().compile(&ink).unwrap();
    println!("JSON:\n{}\n", &json);
    let mut story = Story::new(&json).unwrap();

    let mut text: Vec<String> = Vec::new();
    while story.can_continue() {
        let t = story.cont().unwrap();
        print!("cont: {:?}\n", t);
        if !t.trim().is_empty() {
            text.push(t.trim().to_owned());
        }
    }
    println!(
        "text.len()={}, choices={}",
        text.len(),
        story.get_current_choices().len()
    );

    story.choose_choice_index(0).unwrap();

    text.clear();
    while story.can_continue() {
        let t = story.cont().unwrap();
        print!("cont: {:?}\n", t);
        if !t.trim().is_empty() {
            text.push(t.trim().to_owned());
        }
    }
    println!(
        "text.len()={}, choices={}",
        text.len(),
        story.get_current_choices().len()
    );
    for c in story.get_current_choices() {
        println!("  choice: {:?}", c.text);
    }
}
