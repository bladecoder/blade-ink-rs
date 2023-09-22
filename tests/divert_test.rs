use bladeink::story::Story;

mod common;

#[test]
fn simple_divert_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/divert/simple-divert.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();
    println!("{}", story.build_string_of_hierarchy());

    let mut text: Vec<String> = Vec::new();
    common::next_all(&mut story, &mut text)?;
    assert_eq!(2, text.len());
    assert_eq!("We arrived into London at 9.45pm exactly.", text[0]);
    assert_eq!("We hurried home to Savile Row as fast as we could.", text[1]);

    Ok(())
}
