use bladeink::story::Story;

mod common;

#[test]
fn list_basic_operations_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/lists/basic-operations.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    assert_eq!("b, d\na, b, c, e\nb, c\nfalse\ntrue\ntrue\n", &story.continue_maximally()?);

    Ok(())
}
