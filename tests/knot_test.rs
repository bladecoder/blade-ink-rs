use bladeink::story::Story;

mod common;

#[test]
fn gather_basic_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/gather/gather-basic.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    let mut text: Vec<String> = Vec::new();

    common::next_all(&mut story, &mut text)?;
    story.choose_choice_index(1);

    text.clear();
    common::next_all(&mut story, &mut text)?;

    assert_eq!(3, text.len());
    assert_eq!("\"Nothing, Monsieur!\" I replied.", text[0]);
    assert_eq!("\"Very good, then.\"", text[1]);
    assert_eq!("With that Monsieur Fogg left the room.", text[2]);

    Ok(())
}