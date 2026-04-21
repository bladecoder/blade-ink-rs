use std::rc::Rc;

use bladeink::story::Story as RuntimeStory;

use crate::{
    bootstrap::legacy_root::{Compiler as BootstrapCompiler, CompilerOptions as BootstrapOptions},
    error::CompilerError,
    file_handler::FileHandler,
    ink_parser::InkParser,
    parsed_hierarchy::Story,
    runtime_export, stats, wave1,
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
            CompilerError::unsupported_feature(
                "Compiler::parse requires the source to be provided via Compiler::from_source"
                    .to_owned(),
            )
        })?;

        let parser = InkParser::new(source, self.options.source_filename.clone());
        if let Some(file_handler) = &self.options.file_handler {
            return parser
                .parse_story_with_file_handler(self.options.count_all_visits, |filename| {
                    file_handler.load_ink_file_contents(filename)
                });
        }

        parser.parse_story(self.options.count_all_visits)
    }

    pub fn compile_story(&self) -> Result<RuntimeStory, CompilerError> {
        let parsed = self.parse()?;
        self.compile_story_from_source(parsed.source())
    }

    pub fn compile_story_from_source(&self, source: &str) -> Result<RuntimeStory, CompilerError> {
        if Self::bootstrap_disabled() {
            if let Some(story) = self.try_compile_basic_story(source)? {
                return Ok(story);
            }

            if let Some(story) = self.try_compile_wave1_story(source)? {
                return Ok(story);
            }

            self.ensure_bootstrap_allowed("compilation fallback")?;
        }

        let json = self.bootstrap().compile(source)?;
        RuntimeStory::new(&json).map_err(|err| CompilerError::invalid_source(err.to_string()))
    }

    pub fn compile_story_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<RuntimeStory, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
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
        self.ensure_bootstrap_allowed("stats generation")?;
        self.bootstrap().compile_to_stats(source)
    }

    pub fn compile_to_stats_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<stats::Stats, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
    {
        self.ensure_bootstrap_allowed("stats generation with file handler")?;
        self.bootstrap()
            .compile_to_stats_with_file_handler(source, file_handler)
    }

    pub fn compile_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<String, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
    {
        self.compile_json_with_file_handler(source, file_handler)
    }

    pub fn compile_json_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: F,
    ) -> Result<String, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
    {
        if Self::bootstrap_disabled() {
            if let Some(story) =
                self.try_compile_runtime_story_with_file_handler(source, &file_handler)?
            {
                return story
                    .to_compiled_json()
                    .map_err(|err| CompilerError::invalid_source(err.to_string()));
            }

            self.ensure_bootstrap_allowed("compilation with file handler")?;
        }

        self.bootstrap()
            .compile_with_file_handler(source, file_handler)
    }

    fn compile_internal(&self, source: &str) -> Result<String, CompilerError> {
        if let Some(file_handler) = &self.options.file_handler {
            if Self::bootstrap_disabled() {
                if let Some(story) =
                    self.try_compile_runtime_story_with_file_handler(source, &|filename| {
                        file_handler.load_ink_file_contents(filename)
                    })?
                {
                    return story
                        .to_compiled_json()
                        .map_err(|err| CompilerError::invalid_source(err.to_string()));
                }

                self.ensure_bootstrap_allowed("compilation with file handler")?;
            }

            return self
                .bootstrap()
                .compile_with_file_handler(source, |filename| {
                    file_handler.load_ink_file_contents(filename)
                });
        }

        if Self::bootstrap_disabled() {
            if let Some(story) = self.try_compile_runtime_story(source)? {
                return story
                    .to_compiled_json()
                    .map_err(|err| CompilerError::invalid_source(err.to_string()));
            }

            if let Some(story) = self.try_compile_wave1_story(source)? {
                return story
                    .to_compiled_json()
                    .map_err(|err| CompilerError::invalid_source(err.to_string()));
            }

            self.ensure_bootstrap_allowed("compilation fallback")?;
        }

        self.bootstrap().compile(source)
    }

    fn bootstrap(&self) -> BootstrapCompiler {
        BootstrapCompiler::with_options(BootstrapOptions {
            count_all_visits: self.options.count_all_visits,
            source_filename: self.options.source_filename.clone(),
        })
    }

    fn try_compile_basic_story(&self, source: &str) -> Result<Option<RuntimeStory>, CompilerError> {
        self.try_compile_runtime_story(source)
    }

    fn try_compile_runtime_story(&self, source: &str) -> Result<Option<RuntimeStory>, CompilerError> {
        let parsed = InkParser::new(source, self.options.source_filename.clone())
            .parse_story(self.options.count_all_visits)?;
        match runtime_export::export_story(&parsed) {
            Ok(story) => Ok(Some(story)),
            Err(CompilerError::UnsupportedFeature { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn try_compile_runtime_story_with_file_handler<F>(
        &self,
        source: &str,
        file_handler: &F,
    ) -> Result<Option<RuntimeStory>, CompilerError>
    where
        F: Fn(&str) -> Result<String, CompilerError>,
    {
        let parsed = InkParser::new(source, self.options.source_filename.clone())
            .parse_story_with_file_handler(self.options.count_all_visits, file_handler)?;
        match runtime_export::export_story(&parsed) {
            Ok(story) => Ok(Some(story)),
            Err(CompilerError::UnsupportedFeature { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn try_compile_wave1_story(&self, source: &str) -> Result<Option<RuntimeStory>, CompilerError> {
        match wave1::compile(source, self.options.count_all_visits) {
            Ok(compiled) => Ok(Some(compiled.story)),
            Err(error @ CompilerError::UnsupportedFeature { .. }) => {
                if Self::bootstrap_disabled() {
                    Err(error.with_message_prefix(
                        "bootstrap fallback disabled by BLADEINK_DISABLE_BOOTSTRAP=1; ",
                    ))
                } else {
                    Ok(None)
                }
            }
            Err(error) => Err(error),
        }
    }

    fn ensure_bootstrap_allowed(&self, operation: &str) -> Result<(), CompilerError> {
        if Self::bootstrap_disabled() {
            return Err(CompilerError::unsupported_feature(format!(
                "bootstrap fallback disabled by BLADEINK_DISABLE_BOOTSTRAP=1; {operation} still requires the legacy compiler"
            )));
        }

        Ok(())
    }

    fn bootstrap_disabled() -> bool {
        std::env::var("BLADEINK_DISABLE_BOOTSTRAP")
            .map(|value| Self::bootstrap_disabled_value(&value))
            .unwrap_or(false)
    }

    fn bootstrap_disabled_value(value: &str) -> bool {
        value == "1" || value.eq_ignore_ascii_case("true")
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        process::Command,
        rc::Rc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::Compiler;
    use crate::{
        CompilerError, CompilerOptions,
        file_handler::FileHandler,
        parsed_hierarchy::{ObjectKind, ParsedNodeKind},
    };
    use bladeink::story::Story as RuntimeStory;
    use serde_json::Value as JsonValue;

    #[test]
    fn bootstrap_disabled_value_accepts_numeric_and_boolean_true() {
        assert!(Compiler::bootstrap_disabled_value("1"));
        assert!(Compiler::bootstrap_disabled_value("true"));
        assert!(Compiler::bootstrap_disabled_value("TRUE"));
        assert!(!Compiler::bootstrap_disabled_value("0"));
        assert!(!Compiler::bootstrap_disabled_value("false"));
    }

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
        let compiler = Compiler::new();
        let runtime_story = compiler
            .try_compile_runtime_story(ink)
            .expect("runtime export")
            .expect("supported by runtime export");

        let json = runtime_story
            .to_compiled_json()
            .expect("runtime serialization");
        let mut story = RuntimeStory::new(&json).expect("runtime reads its own JSON");

        assert_eq!("Score: 1\nsad\n", story.continue_maximally().unwrap());
    }

    #[test]
    fn runtime_export_matches_inklecate_for_wave8_subset() {
        assert_matches_inklecate_json("Line.\nOther line.\n");
        assert_matches_inklecate_json(
            "We arrived into London at 9.45pm exactly.\n-> hurry_home\n\n=== hurry_home ===\nWe hurried home to Savile Row as fast as we could. -> END\n",
        );
        assert_matches_inklecate_json(
            "LIST list = a, (b), c, (d), e\n{list}\n{(a, c) + (b, e)}\n{(a, b, c) ^ (c, b, e)}\n{list ? (b, d, e)}\n{list ? (d, b)}\n{list !? (c)}\n",
        );
    }

    fn assert_matches_inklecate_json(ink: &str) {
        let temp_dir = unique_temp_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let ink_path = temp_dir.join("story.ink");
        let inklecate_json_path = temp_dir.join("inklecate.json");
        fs::write(&ink_path, ink).expect("write temp ink");

        let output = Command::new("inklecate")
            .arg("-o")
            .arg(&inklecate_json_path)
            .arg(&ink_path)
            .output()
            .expect("run inklecate from PATH");
        assert!(
            output.status.success(),
            "inklecate failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let options = CompilerOptions {
            count_all_visits: false,
            source_filename: Some("story.ink".to_owned()),
            ..Default::default()
        };
        let compiler = Compiler::with_options(options);
        let runtime_story = compiler
            .try_compile_runtime_story(ink)
            .expect("runtime export")
            .expect("supported by runtime export");
        let rust_json = runtime_story
            .to_compiled_json()
            .expect("runtime serialization");

        let expected: JsonValue =
            serde_json::from_str(&fs::read_to_string(&inklecate_json_path).expect("read oracle"))
                .expect("parse inklecate json");
        let actual: JsonValue = serde_json::from_str(&rust_json).expect("parse rust json");
        assert_eq!(expected, actual);

        let _ = fs::remove_dir_all(temp_dir);
    }

    fn unique_temp_dir() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "bladeink-compiler-oracle-{}-{nanos}",
            std::process::id()
        ))
    }
}
