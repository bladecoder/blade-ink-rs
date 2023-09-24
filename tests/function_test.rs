use bladeink::story::Story;

mod common;

#[test]
fn fun_basic_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/function/func-basic.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("The value of x is 4.4.", text[0]);

    Ok(())
}