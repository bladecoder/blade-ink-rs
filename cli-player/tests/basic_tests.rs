use assert_cmd::prelude::*;
use predicates::prelude::predicate; // Add methods on commands
use std::process::{Command, Stdio};
use std::io::Write;

#[test]
fn basic_story_test() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("binkplayer")?;

    cmd.arg("tests/data/test1.ink.json");
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());

    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();

    stdin.write_all(b"1\n").unwrap();
    
    let output = child.wait_with_output()?;
    let output_str = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(output_str.starts_with("Test conditional choices"));
    assert!(output_str.contains("1. one"));
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