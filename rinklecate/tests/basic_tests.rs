use assert_cmd::prelude::*;
use predicates::prelude::predicate;
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

/// Play mode from a pre-compiled .ink.json — mirrors binkplayer's basic_story_test.
#[test]
fn basic_story_test() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("rinklecate")?;

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

/// Compile-then-play from a .ink source file.
#[test]
fn compile_and_play_test() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("rinklecate")?;

    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../conformance-tests/inkfiles/test1.ink");

    cmd.args(["-p", path.to_str().unwrap()]);
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

/// Non-existent input file should fail with an informative message.
#[test]
fn story_not_found_test() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("rinklecate")?;

    cmd.arg("nonexistent.ink.json");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Could not open file"));

    Ok(())
}

#[test]
fn json_issues_escape_backslashes_test() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("rinklecate")?;
    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("blade-ink-rs-{unique}"));
    fs::create_dir_all(&temp_dir)?;
    let source_path = temp_dir.join("main.ink");
    fs::write(&source_path, "INCLUDE missing\\\\story.ink\n")?;

    let output = cmd.args(["-j", source_path.to_str().unwrap()]).output()?;
    let stdout = String::from_utf8(output.stdout)?;

    assert!(!output.status.success());
    assert!(stdout.contains("{\"compile-success\": false}"));
    let mut parsed_lines = Vec::new();
    for line in stdout.lines() {
        parsed_lines.push(serde_json::from_str::<serde_json::Value>(line)?);
    }
    let issues = parsed_lines[1]["issues"].as_array().unwrap();
    let issue = issues[0].as_str().unwrap();
    assert!(issue.contains(r#"missing\\story.ink"#));

    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

#[test]
fn image_mode_compiles_ink_and_compiled_json() -> Result<(), Box<dyn std::error::Error>> {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("blade-ink-image-{unique}"));
    fs::create_dir_all(&temp_dir)?;
    let source = temp_dir.join("story.ink");
    let image_from_ink = temp_dir.join("from-ink.inkb");
    let image_from_json = temp_dir.join("from-json.inkb");
    let compiled_json = temp_dir.join("story.ink.json");
    let default_image = temp_dir.join("story.inkb");
    fs::write(&source, "Hello world.\n-> END\n")?;

    Command::cargo_bin("rinklecate")?
        .args([
            "-o",
            compiled_json.to_str().unwrap(),
            source.to_str().unwrap(),
        ])
        .assert()
        .success();
    Command::cargo_bin("rinklecate")?
        .args([
            "--image",
            "-o",
            image_from_ink.to_str().unwrap(),
            source.to_str().unwrap(),
        ])
        .assert()
        .success();
    Command::cargo_bin("rinklecate")?
        .args(["--image", compiled_json.to_str().unwrap()])
        .assert()
        .success();
    Command::cargo_bin("rinklecate")?
        .args([
            "--image",
            "-o",
            image_from_json.to_str().unwrap(),
            compiled_json.to_str().unwrap(),
        ])
        .assert()
        .success();

    let from_ink = fs::read(image_from_ink)?;
    let from_json = fs::read(image_from_json)?;
    assert_eq!(&from_ink[..8], bladeink::image::MAGIC);
    assert_eq!(from_ink, from_json);
    assert_eq!(from_json, fs::read(default_image)?);
    assert_eq!(
        from_json,
        bladeink::image::compile_json_to_image(fs::read(compiled_json)?.as_slice())?
    );

    fs::remove_dir_all(temp_dir)?;
    Ok(())
}

#[test]
fn image_mode_rejects_play_and_stats() -> Result<(), Box<dyn std::error::Error>> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../conformance-tests/inkfiles/test1.ink");
    for conflicting in ["-p", "-s"] {
        Command::cargo_bin("rinklecate")?
            .args(["--image", conflicting, path.to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains("--image cannot be combined"));
    }
    Ok(())
}
