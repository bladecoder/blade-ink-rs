use bladeink::story::Story;

mod common;

#[test]
fn thread_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/threads/thread-bug.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    println!("{}", story.build_string_of_hierarchy());
    
    assert_eq!("Here is some gold. Do you want it?\n", story.continue_maximally()?);
    assert_eq!(2, story.get_current_choices().len());
    assert_eq!("No", story.get_current_choices()[0].text);
    assert_eq!("Yes", story.get_current_choices()[1].text);
    story.choose_choice_index(0);
    
    assert_eq!("No\nTry again!\n", story.continue_maximally()?);
    assert_eq!(2, story.get_current_choices().len());
    assert_eq!("No", story.get_current_choices()[0].text);
    assert_eq!("Yes", story.get_current_choices()[1].text);
    story.choose_choice_index(1);

    assert_eq!("Yes\nYou win!\n", story.continue_maximally()?);


    Ok(())
}

#[test]
fn thread_test_bug() -> Result<(), String>  {    
    let json_string =
        common::get_json_string("examples/inkfiles/threads/thread-bug.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    println!("{}", story.build_string_of_hierarchy());
    
    assert_eq!("Here is some gold. Do you want it?\n", story.continue_maximally()?);
    assert_eq!(2, story.get_current_choices().len());
    assert_eq!("No", story.get_current_choices()[0].text);
    assert_eq!("Yes", story.get_current_choices()[1].text);

    let save_string = story.get_state().to_json();
    println!("{}", save_string);
    let mut story = Story::new(&json_string).unwrap();
    story.get_state_mut().load_json(&save_string)?;

    story.choose_choice_index(0);
    
    assert_eq!("No\nTry again!\n", story.continue_maximally()?);
    assert_eq!(2, story.get_current_choices().len());
    assert_eq!("No", story.get_current_choices()[0].text);
    assert_eq!("Yes", story.get_current_choices()[1].text);
    story.choose_choice_index(1);

    assert_eq!("Yes\nYou win!\n", story.continue_maximally()?);


    Ok(())
}