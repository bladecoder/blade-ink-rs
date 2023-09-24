use bladeink::story::Story;

mod common;

#[test]
fn variable_declaration_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/variable/variable-declaration.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(1, text.len());
    assert_eq!("\"My name is Jean Passepartout, but my friend's call me Jackie. I'm 23 years old.\"", text[0]);

    Ok(())
}