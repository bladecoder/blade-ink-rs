use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;

mod common;
#[test]
fn thread_test() -> Result<(), StoryError> {
    let ink_source = common::get_file_string("inkfiles/threads/thread-bug.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    println!("{}", story.build_string_of_hierarchy());

    assert_eq!(
        "Here is some gold. Do you want it?\n",
        story.continue_maximally()?
    );
    assert_eq!(2, story.get_current_choices().len());
    assert_eq!("No", story.get_current_choices()[0].text);
    assert_eq!("Yes", story.get_current_choices()[1].text);
    story.choose_choice_index(0)?;

    assert_eq!("No\nTry again!\n", story.continue_maximally()?);
    assert_eq!(2, story.get_current_choices().len());
    assert_eq!("No", story.get_current_choices()[0].text);
    assert_eq!("Yes", story.get_current_choices()[1].text);
    story.choose_choice_index(1)?;

    assert_eq!("Yes\nYou win!\n", story.continue_maximally()?);

    Ok(())
}

#[test]
fn read_count_across_threads_test() -> Result<(), StoryError> {
    let ink_source = r#"
-> top

= top
{top}
<- aside
{top}
-> DONE

= aside
* {false} DONE
- -> DONE
"#;
    let json_string = Compiler::new().compile(ink_source).unwrap();
    let mut story = Story::new(&json_string)?;
    assert_eq!("1\n1\n", story.continue_maximally()?);
    Ok(())
}
