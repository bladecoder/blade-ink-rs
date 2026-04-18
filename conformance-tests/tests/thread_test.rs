use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;

mod common;

// TestKnotThreadInteraction (Tests.cs:1234)
#[test]
fn knot_thread_interaction_test() -> Result<(), StoryError> {
    let ink = r#"
-> knot
=== knot
    <- threadB
    -> tunnel ->
    THE END
    -> END

=== tunnel
    - blah blah
    * wigwag
    - ->->

=== threadB
    *   option
    -   something
        -> DONE
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    assert_eq!("blah blah\n", story.continue_maximally()?);
    assert_eq!(2, story.get_current_choices().len());
    assert!(story.get_current_choices()[0].text.contains("option"));
    assert!(story.get_current_choices()[1].text.contains("wigwag"));

    story.choose_choice_index(1)?;
    assert_eq!("wigwag\n", story.cont()?);
    assert_eq!("THE END\n", story.cont()?);

    Ok(())
}

// TestThreadDone (Tests.cs:1938)
#[test]
fn thread_done_test() -> Result<(), StoryError> {
    let ink = r#"
This is a thread example
<- example_thread
The example is now complete.

== example_thread ==
Hello.
-> DONE
World.
-> DONE
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!(
        "This is a thread example\nHello.\nThe example is now complete.\n",
        story.continue_maximally()?
    );
    Ok(())
}

// TestTopFlowTerminatorShouldntKillThreadChoices (Tests.cs:3471)
#[test]
fn top_flow_terminator_shouldnt_kill_thread_choices_test() -> Result<(), StoryError> {
    let ink = r#"
<- move
Limes

=== move
    * boop
        -> END
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;

    assert_eq!("Limes\n", story.cont()?);
    assert_eq!(1, story.get_current_choices().len());

    Ok(())
}
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
