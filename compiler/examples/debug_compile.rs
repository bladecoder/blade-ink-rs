use bladeink_compiler::Compiler;

fn main() {
    let ink = r#"+ one #one [two #two] three #three -> END"#;
    let json = Compiler::new().compile(ink).unwrap();
    println!("{}", serde_json::to_string_pretty(&serde_json::from_str::<serde_json::Value>(&json).unwrap()).unwrap());
}
