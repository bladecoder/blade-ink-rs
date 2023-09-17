
mod test_utils;

#[test]
fn no_choice_test() -> Result<(), String>  {
    let mut errors:Vec<String> = Vec::new();

    let text = test_utils::run_story("examples/inkfiles/choices/no-choice-text.ink.json", None, &mut errors)?;

    assert_eq!(0, errors.len());
    assert_eq!("Hello world!\nHello back!\n", test_utils::join_text(&text));

    Ok(())
}
