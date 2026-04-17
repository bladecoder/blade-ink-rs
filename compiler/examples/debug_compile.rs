use bladeink_compiler::Compiler;
use std::fs;

fn main() {
    let ink = fs::read_to_string(
        "conformance-tests/inkfiles/function/evaluating-function-variablestate-bug.ink",
    )
    .unwrap();
    let json = Compiler::new().compile(&ink).unwrap();
    println!("JSON:\n{}\n", &json);
}
