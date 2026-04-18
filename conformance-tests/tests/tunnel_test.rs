use bladeink::{story::Story, story_error::StoryError};
use bladeink_compiler::Compiler;

mod common;

// TestBasicTunnel (Tests.cs:104)
#[test]
fn basic_tunnel_test() -> Result<(), StoryError> {
    let ink = r#"
-> f ->
<> world

== f ==
Hello
->->
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!("Hello world\n", story.cont()?);
    Ok(())
}

// TestComplexTunnels (Tests.cs:369)
#[test]
fn complex_tunnels_test() -> Result<(), StoryError> {
    let ink = r#"
-> one (1) -> two (2) ->
three (3)

== one(num) ==
one ({num})
-> oneAndAHalf (1.5) ->
->->

== oneAndAHalf(num) ==
one and a half ({num})
->->

== two (num) ==
two ({num})
->->
"#;
    let json = Compiler::new().compile(ink).unwrap();
    let mut story = Story::new(&json)?;
    assert_eq!(
        "one (1)\none and a half (1.5)\ntwo (2)\nthree (3)\n",
        story.continue_maximally()?
    );
    Ok(())
}

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
