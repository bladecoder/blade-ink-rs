pub mod error;
mod parsed_hierarchy;
mod parser;

pub use error::CompilerError;

#[derive(Debug, Clone)]
pub struct CompilerOptions {
    pub count_all_visits: bool,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            count_all_visits: true,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Compiler {
    options: CompilerOptions,
}

impl Compiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: CompilerOptions) -> Self {
        Self { options }
    }

    pub fn compile(&self, source: &str) -> Result<String, CompilerError> {
        let _ = self.options.count_all_visits;
        let parsed_story = parser::Parser::new(source).parse()?;
        parsed_story
            .to_json_string()
            .map_err(|_| CompilerError::InvalidSource("failed to serialize compiled ink"))
    }
}

#[cfg(test)]
mod tests {
    use super::Compiler;
    use serde_json::Value;

    #[test]
    fn compiles_single_line_text_story() {
        let compiled = Compiler::new().compile("Line.\n").unwrap();
        let actual: Value = serde_json::from_str(&compiled).unwrap();
        let expected: Value = serde_json::from_str(
            r##"{"inkVersion":21,"root":[["^Line.","\n",["done",{"#n":"g-0"}],null],"done",null],"listDefs":{}}"##,
        )
        .unwrap();

        assert_eq!(expected, actual);
    }

    #[test]
    fn compiles_two_line_text_story() {
        let compiled = Compiler::new().compile("Line.\nOther line.\n").unwrap();
        let actual: Value = serde_json::from_str(&compiled).unwrap();
        let expected: Value = serde_json::from_str(
            r##"{"inkVersion":21,"root":[["^Line.","\n","^Other line.","\n",["done",{"#n":"g-0"}],null],"done",null],"listDefs":{}}"##,
        )
        .unwrap();

        assert_eq!(expected, actual);
    }
}
