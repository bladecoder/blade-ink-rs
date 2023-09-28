use bladeink::story::Story;

mod common;

#[test]
fn operations_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/misc/operations.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    assert_eq!("neg:-3\nmod:1\npow:27\nfloor:3\nceiling:4\nint:3\nfloat:1\n", &story.continue_maximally()?);

    Ok(())
}

/**
 * Issue: https://github.com/bladecoder/blade-ink/issues/15
 */
#[test]
fn issue15_test() -> Result<(), String>  {
    let json_string =
    common::get_json_string("examples/inkfiles/misc/issue15.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    assert_eq!("This is a test\n", story.cont()?);

    while story.can_continue() {
        // println!(story.buildStringOfHierarchy());
        let line = &story.cont()?;

        if line.starts_with("SET_X:") {
            story.get_variables_state_mut().set_int("x", 100);
        } else {
            assert_eq!("X is set\n", line);
        }
    }

    Ok(())   
}