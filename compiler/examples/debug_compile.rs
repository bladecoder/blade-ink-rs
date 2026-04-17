use bladeink_compiler::Compiler;
use std::fs;

fn main() {
    let ink = fs::read_to_string("conformance-tests/inkfiles/threads/thread-bug.ink").unwrap();
    match Compiler::new().compile(&ink) {
        Ok(json) => println!("compiled json: {}", &json),
        Err(e) => println!("compile error: {:?}", e),
    }
}
