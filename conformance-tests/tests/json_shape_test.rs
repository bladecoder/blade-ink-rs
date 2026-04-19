use std::{fs, path::Path};

use bladeink_compiler::{Compiler, CompilerOptions};
use serde_json::Value;

fn load_fixture(path: &str) -> String {
    let full = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(full).expect("failed to read fixture")
}

#[test]
fn loop_label_threading_min_json_shape_matches_fixture() {
    let ink = load_fixture("inkfiles/gather/loop-label-threading-min.ink");
    let expected = load_fixture("inkfiles/gather/loop-label-threading-min.ink.json");

    let compiled = Compiler::with_options(CompilerOptions {
        count_all_visits: false,
        source_filename: None,
    })
    .compile(&ink)
    .expect("compile failed");

    let compiled_json: Value = serde_json::from_str(&compiled).expect("invalid compiled json");
    let expected_json: Value = serde_json::from_str(&expected).expect("invalid expected json");

    assert_eq!(compiled_json, expected_json);
}
