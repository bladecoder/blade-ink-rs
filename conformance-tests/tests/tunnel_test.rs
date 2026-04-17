use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;

mod common;

#[test]
fn tunnel_onwards_divert_override_test() -> Result<(), StoryError> {
    let ink_source =
        common::get_file_string("inkfiles/tunnels/tunnel-onwards-divert-override.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!("This is A\nNow in B.\n", story.continue_maximally()?);

    Ok(())
}

#[test]
fn sequence_tunnel_test() -> Result<(), StoryError> {
    // Regression: a tunnel divert inside a once-only sequence {! ->knot->}
    // was failing to compile with "expected divert target after '->'".
    let ink_source = common::get_file_string("inkfiles/tunnels/sequence-tunnel.ink").unwrap();
    let json_string = Compiler::new().compile(&ink_source).unwrap();
    let mut story = Story::new(&json_string)?;

    assert_eq!("Hello from tunnel.\nDone.\n", story.continue_maximally()?);

    Ok(())
}
