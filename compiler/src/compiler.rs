use std::rc::Rc;

use bladeink::story::Story as RuntimeStory;

use crate::{
    error::CompilerError,
    file_handler::FileHandler,
    ink_parser::InkParser,
    parsed_hierarchy::Story,
    runtime_export, stats,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
    Warning,
    Error,
}

pub type ErrorHandler = Rc<dyn Fn(String, ErrorType)>;

#[derive(Clone)]
pub struct CompilerOptions {
    pub source_filename: Option<String>,
    pub plugin_directories: Vec<String>,
    pub count_all_visits: bool,
    pub error_handler: Option<ErrorHandler>,
    pub file_handler: Option<Rc<dyn FileHandler>>,
}

impl std::fmt::Debug for CompilerOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompilerOptions")
            .field("source_filename", &self.source_filename)
            .field("plugin_directories", &self.plugin_directories)
            .field("count_all_visits", &self.count_all_visits)
            .field("has_error_handler", &self.error_handler.is_some())
            .field("has_file_handler", &self.file_handler.is_some())
            .finish()
    }
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            source_filename: None,
            plugin_directories: Vec::new(),
            count_all_visits: true,
            error_handler: None,
            file_handler: None,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Compiler {
    source: Option<String>,
    options: CompilerOptions,
}

impl Compiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: CompilerOptions) -> Self {
        Self {
            source: None,
            options,
        }
    }

    pub fn from_source(source: impl Into<String>) -> Self {
        Self {
            source: Some(source.into()),
            options: CompilerOptions::default(),
        }
    }

    pub fn from_source_with_options(source: impl Into<String>, options: CompilerOptions) -> Self {
        Self {
            source: Some(source.into()),
            options,
        }
    }

    pub fn options(&self) -> &CompilerOptions {
        &self.options
    }

    pub fn parse(&self) -> Result<Story, CompilerError> {
        let source = self.source.as_deref().ok_or_else(|| {
            CompilerError::invalid_source(
                "Compiler::parse requires the source to be provided via Compiler::from_source",
            )
        })?;

        let parser = InkParser::new(source, self.options.source_filename.clone());
        if let Some(file_handler) = &self.options.file_handler {
            let file_handler = file_handler.clone();
            return parser
                .parse_story_with_file_handler(self.options.count_all_visits, move |filename| {
                    file_handler.load_ink_file_contents(filename)
                });
        }

        parser.parse_story(self.options.count_all_visits)
    }

    pub fn compile_story(&self) -> Result<RuntimeStory, CompilerError> {
        let parsed = self.parse()?;
        runtime_export::export_story(&parsed)
    }

    pub fn compile_story_from_source(&self, source: &str) -> Result<RuntimeStory, CompilerError> {
        let parsed = InkParser::new(source, self.options.source_filename.clone())
            .parse_story(self.options.count_all_visits)?;
        runtime_export::export_story(&parsed)
    }

    pub fn compile_story_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<RuntimeStory, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError> + 'static,
    {
        let json = self.compile_json_with_file_handler(source, file_handler)?;
        RuntimeStory::new(&json).map_err(|err| CompilerError::invalid_source(err.to_string()))
    }

    pub fn compile(&self, source: &str) -> Result<String, CompilerError> {
        self.compile_json(source)
    }

    pub fn compile_json(&self, source: &str) -> Result<String, CompilerError> {
        self.compile_internal(source)
    }

    pub fn compile_to_stats(&self, source: &str) -> Result<stats::Stats, CompilerError> {
        let parsed = InkParser::new(source, self.options.source_filename.clone())
            .parse_story(self.options.count_all_visits)?;
        Ok(stats::Stats::generate_from_parsed(&parsed))
    }

    pub fn compile_to_stats_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<stats::Stats, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError> + 'static,
    {
        let parsed = InkParser::new(source, self.options.source_filename.clone())
            .parse_story_with_file_handler(self.options.count_all_visits, file_handler)?;
        Ok(stats::Stats::generate_from_parsed(&parsed))
    }

    pub fn compile_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<String, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError> + 'static,
    {
        self.compile_json_with_file_handler(source, file_handler)
    }

    pub fn compile_json_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<String, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError> + 'static,
    {
        let parsed = InkParser::new(source, self.options.source_filename.clone())
            .parse_story_with_file_handler(self.options.count_all_visits, file_handler)?;
        let story = runtime_export::export_story(&parsed)?;
        story
            .to_compiled_json()
            .map_err(|err| CompilerError::invalid_source(err.to_string()))
    }

    fn compile_internal(&self, source: &str) -> Result<String, CompilerError> {
        if let Some(file_handler) = &self.options.file_handler {
            let file_handler = file_handler.clone();
            let parsed = InkParser::new(source, self.options.source_filename.clone())
                .parse_story_with_file_handler(self.options.count_all_visits, move |filename| {
                    file_handler.load_ink_file_contents(filename)
                })?;
            let story = runtime_export::export_story(&parsed)?;
            return story
                .to_compiled_json()
                .map_err(|err| CompilerError::invalid_source(err.to_string()));
        }

        let parsed = InkParser::new(source, self.options.source_filename.clone())
            .parse_story(self.options.count_all_visits)?;
        let story = runtime_export::export_story(&parsed)?;
        story
            .to_compiled_json()
            .map_err(|err| CompilerError::invalid_source(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::Compiler;
    use crate::{
        CompilerOptions,
        file_handler::FileHandler,
        parsed_hierarchy::{ObjectKind, ParsedNodeKind},
        CompilerError,
    };
    use bladeink::story::Story as RuntimeStory;

    #[test]
    fn parse_returns_populated_hierarchy_for_full_story_shapes() {
        let ink = r#"VAR score = 1
LIST mood = happy, (sad = 5)
EXTERNAL log(x)
Root text
* [Choice] selected
    -> done
- (join)
Done
=== function helper(x)
~ return x
=== knot
= stitch
Stitch text
"#;

        let parsed = Compiler::from_source(ink).parse().expect("parse");
        assert_eq!(1, parsed.global_declarations().len());
        assert_eq!(1, parsed.list_definitions().len());
        assert_eq!(1, parsed.external_declarations().len());
        assert!(
            parsed
                .root_nodes()
                .iter()
                .any(|node| node.kind() == ParsedNodeKind::Choice)
        );
        assert!(
            parsed
                .root_nodes()
                .iter()
                .any(|node| node.kind() == ParsedNodeKind::GatherLabel)
        );
        assert_eq!(2, parsed.parsed_flows().len());
        assert!(parsed.parsed_flows()[0].flow().is_function());
        assert_eq!(1, parsed.parsed_flows()[1].children().len());

        let index = parsed.object_index();
        let choice = parsed
            .object()
            .find(&index, ObjectKind::Choice)
            .expect("choice in index");
        assert_eq!(Some(parsed.object().reference()), index.story_for(choice));
    }

    #[derive(Debug)]
    struct InlineIncludeHandler;

    impl FileHandler for InlineIncludeHandler {
        fn resolve_ink_filename(&self, include_name: &str) -> String {
            include_name.to_owned()
        }

        fn load_ink_file_contents(&self, full_filename: &str) -> Result<String, CompilerError> {
            match full_filename {
                "inc.ink" => Ok("=== included\nIncluded text\n".to_owned()),
                other => Err(CompilerError::invalid_source(format!("missing {other}"))),
            }
        }
    }

    #[test]
    fn parse_resolves_includes_with_file_handler() {
        let options = CompilerOptions {
            file_handler: Some(Rc::new(InlineIncludeHandler)),
            ..Default::default()
        };
        let parsed = Compiler::from_source_with_options("INCLUDE inc.ink\nMain\n", options)
            .parse()
            .expect("parse with include");

        assert!(
            parsed
                .root_nodes()
                .iter()
                .any(|node| node.kind() == ParsedNodeKind::Newline)
        );
        assert!(
            parsed
                .parsed_flows()
                .iter()
                .any(|flow| flow.flow().identifier() == Some("included"))
        );
    }

    #[test]
    fn runtime_export_serializes_with_runtime_story_writer() {
        let ink = "LIST mood = happy, (sad)\nVAR score = 1\nScore: {score}\n{mood}\n";
        let story = Compiler::new()
            .compile_story_from_source(ink)
            .expect("runtime export");

        let json = story
            .to_compiled_json()
            .expect("runtime serialization");
        let mut story = RuntimeStory::new(&json).expect("runtime reads its own JSON");

        assert_eq!("Score: 1\nsad\n", story.continue_maximally().unwrap());
    }

    #[test]
    fn tmp_dump_ifelse_ext_text1_json() {
        let ink = include_str!("../../conformance-tests/inkfiles/conditional/ifelse-ext-text1.ink");
        let json = Compiler::new().compile(ink).expect("compile to json");
        println!("{json}");
    }

    #[test]
    fn tmp_dump_condopt_json() {
        let ink = include_str!("../../conformance-tests/inkfiles/conditional/condopt.ink");
        let json = Compiler::new().compile(ink).expect("compile to json");
        println!("{json}");
    }

    #[test]
    fn tmp_dump_condtext_json() {
        let ink = include_str!("../../conformance-tests/inkfiles/conditional/condtext.ink");
        let json = Compiler::new().compile(ink).expect("compile to json");
        println!("{json}");
    }

    #[test]
    fn tmp_dump_conditionals_json() {
        let ink = r#"
{false:not true|true}
{
   - 4 > 5: not true
   - 5 > 4: true
}
{ 2*2 > 3:
   - true
   - not true
}
{
   - 1 > 3: not true
   - { 2+2 == 4:
        - true
        - not true
   }
}
{ 2*3:
   - 1+7: not true
   - 9: not true
   - 1+1+1+3: true
   - 9-3: also true but not printed
}
{ true:
    great
    right?
}
"#;
        let json = Compiler::new().compile(ink).expect("compile to json");
        println!("{json}");
    }

}
