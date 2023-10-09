use assert_cmd::prelude::*;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

#[test]
fn the_intercept_test() -> Result<(), Box<dyn std::error::Error>> { 
    let mut cmd = Command::cargo_bin("binkplayer")?;

    let mut path = Path::new("inkfiles/TheIntercept.ink.json").to_path_buf();

    // Due to a bug with Cargo workspaces, for Release mode the current folder is the crate folder and for Debug mode the current folder is the root folder.
    if !path.exists() {
        path = Path::new("../").join(path);
    }

    cmd.arg(path);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());

    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();

    stdin.write_all(b"1\n2\nquit\n").unwrap();

    let output = child.wait_with_output()?;
    let output_str = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(output_str.starts_with("They are keeping me waiting."));
    assert!(output_str.contains("1. Hut 14"));
    assert!(output_str.contains("3. Wait"));
    assert!(output_str.contains("3. Divert"));

    Ok(())
}
