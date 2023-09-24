use bladeink::story::Story;

mod common;

#[test]
fn tunnel_onwards_divert_override_test() -> Result<(), String>  {
    let json_string =
        common::get_json_string("examples/inkfiles/tunnels/tunnel-onwards-divert-override.ink.json").unwrap();
    let mut story = Story::new(&json_string).unwrap();

    assert_eq!("This is A\nNow in B.\n", story.continue_maximally()?);

    Ok(())
}