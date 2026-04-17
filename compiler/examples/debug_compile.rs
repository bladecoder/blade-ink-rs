use bladeink::story::Story;
use bladeink_compiler::Compiler;
use std::fs;

fn main() {
    let ink = fs::read_to_string("conformance-tests/inkfiles/misc/issue15.ink").unwrap();
    let json = Compiler::new().compile(&ink).unwrap();
    println!("json: {}", &json);
    let mut story = Story::new(&json).unwrap();
    let mut n = 0;
    while story.can_continue() && n < 10 {
        let line = story.cont().unwrap();
        println!("line: {:?}", line);
        n += 1;
    }
}
