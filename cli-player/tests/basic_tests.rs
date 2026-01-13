use assert_cmd::prelude::*;
use predicates::prelude::predicate; // Add methods on commands
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

#[test]
fn basic_story_test() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("binkplayer")?;

    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../conformance-tests/inkfiles/test1.ink.json");

    cmd.arg(path);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());

    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();

    stdin.write_all(b"1\n").unwrap();

    let output = child.wait_with_output()?;
    let output_str = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(output_str.contains("Test conditional choices"));
    assert!(output_str.contains("1: one"));
    assert!(output_str.ends_with("one\n"));

    Ok(())
}

#[test]
fn story_not_found_test() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("binkplayer")?;

    cmd.arg("nonexistent.ink.json");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("could not read file"));

    Ok(())
}
