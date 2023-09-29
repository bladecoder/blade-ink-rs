use bladeink::story::Story;

mod common;

#[test]
fn basics_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/runtime/multiflow-basics.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    story.switch_flow("First");
    story.choose_path_string("knot1", true, None)?;
    assert_eq!("knot 1 line 1\n", story.cont()?);

    story.switch_flow("Second");
    story.choose_path_string("knot2", true, None)?;
    assert_eq!("knot 2 line 1\n", story.cont()?);

    story.switch_flow("First");
    assert_eq!("knot 1 line 2\n", story.cont()?);

    story.switch_flow("Second");
    assert_eq!("knot 2 line 2\n", story.cont()?);

    Ok(())
}

#[test]
fn multiflow_save_load_threads() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/runtime/multiflow-saveloadthreads.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    // Default flow
    assert_eq!("Default line 1\n", story.cont()?);

    story.switch_flow("Blue Flow");
    story.choose_path_string("blue", true, None)?;
    assert_eq!("Hello I'm blue\n", story.cont()?);

    story.switch_flow("Red Flow");
    story.choose_path_string("red", true, None)?;
    assert_eq!("Hello I'm red\n", story.cont()?);

    // Test existing state remains after switch (blue)
    story.switch_flow("Blue Flow");
    assert_eq!("Hello I'm blue\n", story.get_current_text());
    assert_eq!("Thread 1 blue choice", story.get_current_choices()[0].text);

    // Test existing state remains after switch (red)
    story.switch_flow("Red Flow");
    assert_eq!("Hello I'm red\n", story.get_current_text());
    assert_eq!("Thread 1 red choice", story.get_current_choices()[0].text);

    // Save/load test
    // let saved = story.getState().toJson();

    Ok(())
}
