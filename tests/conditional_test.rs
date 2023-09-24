use bladeink::story::Story;

mod common;

#[test]
fn iftrue_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/conditional/iftrue.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    println!("{}", story.build_string_of_hierarchy());

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;
    
    assert_eq!(1, text.len());
    assert_eq!("The value is 1.", text[0]);

    Ok(())
}